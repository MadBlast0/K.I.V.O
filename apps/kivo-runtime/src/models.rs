//! Speech models on this PC (DISTRIBUTION §4, DIST-12/13). Models are never bundled: the first
//! run downloads the speech-recognition model in the background, verifies it and installs it
//! atomically, and the Island says so while it happens. Licences and attribution travel with the
//! manifest so the UI can show them before a download.

use crate::core::Core;
use crate::infer::{Engines, Infer};
use kivo_core::KivoConfig;
use kivo_ipc::protocol::{ModelItem, SpeechStatus};
use kivo_store::models::{HttpFetcher, ModelKind, ModelManifest, ModelStore, Progress, catalog};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

pub struct Models {
    store: ModelStore,
    core: Arc<Core>,
    infer: Infer,
    downloads: Mutex<HashMap<String, (CancellationToken, Progress)>>,
    /// Threads the hardware recommendation allows (PLAN-01).
    recommended_threads: std::sync::atomic::AtomicUsize,
}

impl Models {
    pub fn new(root: PathBuf, core: Arc<Core>, infer: Infer) -> Self {
        Self {
            store: ModelStore::new(root),
            core,
            infer,
            downloads: Mutex::default(),
            recommended_threads: std::sync::atomic::AtomicUsize::new(4),
        }
    }

    /// The hardware recommendation's thread count (PLAN-01); speech models never use more.
    pub fn recommend_threads(&self, threads: usize) {
        self.recommended_threads
            .store(threads.max(1), std::sync::atomic::Ordering::Relaxed);
    }

    /// The model the settings ask for, or the language's default (VOICE-36).
    pub fn wanted_stt(config: &KivoConfig) -> String {
        let chosen = config.voice.stt_engine.trim();
        if !chosen.is_empty() {
            return chosen.to_owned();
        }
        kivo_voice::language::pack(&config.general.language)
            .and_then(|p| p.stt_engines.first().cloned())
            .unwrap_or_else(|| kivo_voice::moonshine::MODEL_ID.to_owned())
    }

    /// Everything KIVO can install, with what is already here (DIST-13).
    pub fn list(&self) -> Vec<ModelItem> {
        let downloads = self
            .downloads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        catalog()
            .into_iter()
            .map(|m| {
                let installed = self.store.installed(&m.id);
                #[allow(clippy::cast_possible_truncation, reason = "0–100")]
                let downloading = downloads.get(&m.id).map(|(_, p)| percent(*p));
                ModelItem {
                    kind: serde_json::to_value(m.kind)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_owned))
                        .unwrap_or_default(),
                    size: m.download_size(),
                    installed: installed.is_some(),
                    disk_bytes: installed.map_or(0, |i| i.bytes),
                    downloading,
                    residency: self.core.residency(&m.id),
                    id: m.id,
                    name: m.name,
                    license: m.license,
                    attribution: m.attribution,
                    source: m.source,
                    languages: m.languages,
                }
            })
            .collect()
    }

    fn manifest(id: &str) -> Option<ModelManifest> {
        catalog().into_iter().find(|m| m.id == id)
    }

    /// Where an installed model's files are.
    pub fn installed_dir(&self, id: &str) -> Option<PathBuf> {
        self.store.installed(id).map(|i| i.dir)
    }

    /// Starts a download in the background (does nothing if it is installed or already running).
    pub fn install(self: &Arc<Self>, id: &str) -> Result<(), String> {
        let manifest = Self::manifest(id)
            .ok_or_else(|| kivo_core::text::tf("turn.unknownModel", &[("id", &id)]))?;
        {
            let mut downloads = self
                .downloads
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if downloads.contains_key(id) || self.store.installed(id).is_some() {
                return Ok(());
            }
            // A child of the shutdown token: quitting KIVO stops the download (Remove too).
            downloads.insert(
                id.to_owned(),
                (
                    self.core.shutdown().child_token(),
                    Progress {
                        done: 0,
                        total: manifest.download_size(),
                    },
                ),
            );
        }
        let models = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            let id = manifest.id.clone();
            let cancel = models
                .downloads
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(&id)
                .map_or_else(CancellationToken::new, |(c, _)| c.child_token());
            // Only the recognition model decides whether KIVO can hear (Home's note, the Island).
            let hearing = manifest.kind == ModelKind::Stt;
            tracing::info!(model = id, "downloading a model");
            if hearing {
                models
                    .core
                    .set_speech_status(SpeechStatus::Downloading { percent: 0 });
            }
            let mut last_percent = None;
            let result =
                models
                    .store
                    .install(&manifest, &HttpFetcher::new(), &cancel, &mut |progress| {
                        let percent = percent(progress);
                        if last_percent != Some(percent) {
                            last_percent = Some(percent);
                            models.changed(&id, Some(percent), false);
                        }
                        if let Some(entry) = models
                            .downloads
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .get_mut(&id)
                        {
                            entry.1 = progress;
                        }
                        if hearing {
                            models
                                .core
                                .set_speech_status(SpeechStatus::Downloading { percent });
                        }
                    });
            models
                .downloads
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&id);
            match result {
                Ok(installed) => {
                    tracing::info!(model = id, bytes = installed.bytes, "model installed");
                    models.changed(&id, None, true);
                    models.apply_engines(&models.core.config());
                }
                Err(e) => {
                    tracing::error!(%e, model = id, "the model download failed");
                    models.changed(&id, None, false);
                    if hearing {
                        models.core.set_speech_status(SpeechStatus::Failed {
                            message: e.to_string(),
                        });
                    }
                }
            }
        });
        Ok(())
    }

    /// Tells the Control Center a model changed (DIST-13: its list updates by push).
    fn changed(&self, id: &str, percent: Option<u8>, installed: bool) {
        self.core
            .bus
            .publish(kivo_core::Event::new(kivo_core::EventKind::System(
                kivo_core::event::SystemEvent::ModelChanged {
                    id: id.to_owned(),
                    percent,
                    installed,
                },
            )));
    }

    /// Deletes a model and its unfinished download.
    pub fn remove(self: &Arc<Self>, id: &str) -> Result<(), String> {
        if let Some((cancel, _)) = self
            .downloads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(id)
        {
            cancel.cancel();
        }
        self.store.remove(id).map_err(|e| e.to_string())?;
        self.changed(id, None, false);
        self.apply_engines(&self.core.config());
        Ok(())
    }

    /// Tells the worker which engines to use (they load on the first request, VOICE-34) and the
    /// UI whether KIVO can hear.
    pub fn apply_engines(self: &Arc<Self>, config: &KivoConfig) {
        let wanted = Self::wanted_stt(config);
        let stt = self
            .installed_dir(&wanted)
            .map(|dir| (wanted.clone(), dir))
            .filter(|(id, _)| speech_may_use(id, config));
        let ready = stt.is_some();
        let tts = (!config.voice.tts_engine.is_empty())
            .then(|| config.voice.tts_engine.clone())
            .map(|id| self.tts_engine(id, config));
        let warm_minutes = u64::from(
            config
                .performance
                .stt_warm_minutes
                .max(config.performance.tts_warm_minutes),
        );
        self.infer.configure(
            Engines {
                stt,
                tts,
                threads: threads_for(config).min(
                    self.recommended_threads
                        .load(std::sync::atomic::Ordering::Relaxed),
                ),
            },
            std::time::Duration::from_secs(warm_minutes.max(1) * 60),
        );
        if ready {
            self.core.set_speech_status(SpeechStatus::Ready);
        } else if !matches!(self.core.speech_status(), SpeechStatus::Downloading { .. }) {
            self.core.set_speech_status(SpeechStatus::Missing);
        }
    }

    /// The voice to load for the chosen `id`: a model-backed voice (Kokoro) needs its download,
    /// which starts here if it is missing, and the Windows voices speak meanwhile; a cloud voice
    /// the privacy mode rules out is replaced by them too (VOICE-07).
    fn tts_engine(self: &Arc<Self>, id: String, config: &KivoConfig) -> (String, Option<PathBuf>) {
        let system = (kivo_voice::system_tts::ENGINE_ID.to_owned(), None);
        if !speech_may_use(&id, config) {
            return system;
        }
        let needs_model = kivo_voice::engine(&id).is_some_and(|e| e.model.is_some());
        if !needs_model {
            return (id, None);
        }
        match self.installed_dir(&id) {
            Some(dir) => (id, Some(dir)),
            None => {
                if let Err(e) = self.install(&id) {
                    tracing::warn!(%e, voice = id, "couldn't start the voice download");
                }
                system
            }
        }
    }

    /// At startup: note what is installed (nothing is loaded yet), and fetch the speech model if
    /// it is missing.
    pub fn ensure_speech(self: &Arc<Self>, config: &KivoConfig) {
        self.apply_engines(config);
        let wanted = Self::wanted_stt(config);
        if self.installed_dir(&wanted).is_none()
            && let Err(e) = self.install(&wanted)
        {
            tracing::error!(%e, "couldn't start the speech-model download");
            self.core
                .set_speech_status(SpeechStatus::Failed { message: e });
        }
    }
}

#[allow(clippy::cast_possible_truncation, reason = "0–100")]
fn percent(progress: Progress) -> u8 {
    (progress.done * 100)
        .checked_div(progress.total)
        .map_or(0, |p| p.min(100) as u8)
}

/// Threads for the speech models: half the machine, at least one, at most four (plan §128:
/// KIVO never takes the whole CPU).
fn threads_for(config: &KivoConfig) -> usize {
    let cores = std::thread::available_parallelism().map_or(2, std::num::NonZero::get);
    let limit = match config.performance.profile {
        kivo_core::config::PerformanceProfile::Battery
        | kivo_core::config::PerformanceProfile::Gaming => 2,
        _ => 4,
    };
    (cores / 2).clamp(1, limit)
}

/// The privacy check for a speech engine (VOICE-07): audio and text go to a cloud engine only
/// when the privacy mode and the Cloud AI capability allow it.
fn speech_may_use(engine: &str, config: &KivoConfig) -> bool {
    let cloud = kivo_voice::engine(engine).is_some_and(|e| e.kind == kivo_voice::EngineKind::Cloud);
    match kivo_security::privacy::speech_egress(cloud, config.privacy.mode, &config.capabilities) {
        Ok(()) => true,
        Err(denied) => {
            tracing::info!(engine, reason = %denied.message, "speech engine not used");
            false
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_privacy_mode_never_blocks_local_speech_engines() {
        let mut config = KivoConfig::default();
        for mode in [
            kivo_core::config::PrivacyMode::StrictPrivate,
            kivo_core::config::PrivacyMode::Local,
            kivo_core::config::PrivacyMode::Cloud,
        ] {
            config.privacy.mode = mode;
            assert!(speech_may_use(kivo_voice::moonshine::MODEL_ID, &config));
            assert!(speech_may_use(kivo_voice::system_tts::ENGINE_ID, &config));
        }
    }

    #[test]
    fn the_default_speech_model_follows_the_language_then_the_setting() {
        let mut config = KivoConfig::default();
        assert_eq!(Models::wanted_stt(&config), "moonshine-base-en");
        config.voice.stt_engine = "whisper-turbo".into();
        assert_eq!(Models::wanted_stt(&config), "whisper-turbo");
    }

    #[test]
    fn progress_becomes_a_percentage() {
        assert_eq!(percent(Progress { done: 0, total: 0 }), 0);
        assert_eq!(
            percent(Progress {
                done: 50,
                total: 200
            }),
            25
        );
        assert_eq!(
            percent(Progress {
                done: 999,
                total: 200
            }),
            100
        );
    }

    #[test]
    fn model_threads_stay_within_a_sensible_share_of_the_machine() {
        let mut config = KivoConfig::default();
        let balanced = threads_for(&config);
        assert!((1..=4).contains(&balanced));
        config.performance.profile = kivo_core::config::PerformanceProfile::Battery;
        assert!(threads_for(&config) <= 2);
    }

    #[test]
    fn the_catalogue_lists_what_is_installable_with_its_licence() {
        let dir = tempfile::tempdir().unwrap();
        let core = Arc::new(Core::default());
        let (infer, _events, _tx) = Infer::new(PathBuf::from("kivo-infer"));
        let models = Models::new(dir.path().to_path_buf(), core, infer);
        let list = models.list();
        let stt = list.iter().find(|m| m.id == "moonshine-base-en").unwrap();
        assert!(!stt.installed && stt.downloading.is_none());
        assert_eq!(stt.license, "MIT");
        assert!(stt.size > 100_000_000 && !stt.attribution.is_empty());
    }
}
