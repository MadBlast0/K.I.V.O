//! Speech models on this PC (DISTRIBUTION §4, DIST-12/13). Models are never bundled and never
//! fetched unasked: the user chooses one (the Voice page, onboarding), and it downloads in the
//! background with anything it needs (`requires`), verified and installed atomically. Licences and attribution travel with the
//! manifest so the UI can show them before a download.

use crate::core::Core;
use crate::infer::{CudaWorker, Engines, Infer};
use kivo_core::KivoConfig;
use kivo_ipc::infer::GpuTarget;
use kivo_ipc::protocol::{ModelItem, SpeechStatus};
use kivo_store::models::{
    CUDA_RUNTIME, CUDA_WORKER, HttpFetcher, ModelKind, ModelManifest, ModelStore, Progress, catalog,
};
use kivo_voice::EngineSlot;
use kivo_voice::recommend::{Needs, Priority, Recommendation};
use kivo_voice::registry::RegistryEntry;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

/// Where the enrollment word error rates are kept.
pub const VOICE_WER_KEY: &str = "voice.enrollmentWer";
/// Where "Benchmark this engine" results are kept (BENCH-15): engine id → measurement.
pub const ENGINE_BENCH_KEY: &str = "voice.engineBenchmarks";

pub struct Models {
    store: ModelStore,
    core: Arc<Core>,
    infer: Infer,
    downloads: Mutex<HashMap<String, (CancellationToken, Progress)>>,
    /// Threads the hardware recommendation allows (PLAN-01).
    recommended_threads: std::sync::atomic::AtomicUsize,
    /// What this PC is doing, for recommendations (VOICE-44).
    system: Mutex<Option<Arc<dyn kivo_platform::SystemInfo>>>,
    /// KIVO's latest speech benchmarks on this PC: metric names with their medians, and when.
    measurements: Mutex<(Vec<(String, f64)>, i64)>,
    /// How well each recognizer heard the owner's enrollment (VOICE-23), and when.
    voice_wers: Mutex<(std::collections::BTreeMap<String, f64>, i64)>,
    /// "Benchmark this engine" runs (BENCH-15).
    engine_benchmarks: Mutex<std::collections::BTreeMap<String, kivo_voice::registry::Measured>>,
    /// The last download failure per model, until it is tried again (UX-61 "Error").
    errors: Mutex<HashMap<String, String>>,
    /// Where the cloud speech services' keys are (Credential Manager; VOICE-10/11).
    secrets: Mutex<Option<Arc<dyn kivo_platform::Secrets>>>,
    /// The profile and GPU choice the engines were last configured with.
    applied: Mutex<Option<(crate::profiles::Resolved, Option<GpuTarget>)>>,
    /// The PC's graphics cards, read once (cards don't come and go while KIVO runs): the CUDA pack
    /// is offered only with an NVIDIA card, and the Graphics backend setting lists what they allow.
    gpus: std::sync::OnceLock<Vec<kivo_platform::GpuInfo>>,
}

impl Models {
    pub fn new(root: PathBuf, core: Arc<Core>, infer: Infer) -> Self {
        Self {
            store: ModelStore::new(root),
            core,
            infer,
            downloads: Mutex::default(),
            recommended_threads: std::sync::atomic::AtomicUsize::new(4),
            system: Mutex::default(),
            measurements: Mutex::default(),
            voice_wers: Mutex::default(),
            engine_benchmarks: Mutex::default(),
            errors: Mutex::default(),
            secrets: Mutex::new(None),
            applied: Mutex::new(None),
            gpus: std::sync::OnceLock::new(),
        }
    }

    /// Word error rates on the owner's enrollment recordings, per recognizer (VOICE-23).
    pub fn set_voice_wers(&self, wers: std::collections::BTreeMap<String, f64>, at: i64) {
        *lock(&self.voice_wers) = (wers, at);
    }

    pub fn voice_wers(&self) -> std::collections::BTreeMap<String, f64> {
        lock(&self.voice_wers).0.clone()
    }

    /// Keeps a "Benchmark this engine" result (BENCH-15); the caller saves them.
    pub fn set_engine_benchmark(
        &self,
        id: &str,
        measured: kivo_voice::registry::Measured,
    ) -> std::collections::BTreeMap<String, kivo_voice::registry::Measured> {
        let mut runs = lock(&self.engine_benchmarks);
        runs.insert(id.to_owned(), measured);
        runs.clone()
    }

    /// What the PC can report about processes and load, once the runtime has set it.
    pub fn system_info(&self) -> Option<Arc<dyn kivo_platform::SystemInfo>> {
        lock(&self.system).clone()
    }

    pub fn set_system(&self, system: Arc<dyn kivo_platform::SystemInfo>) {
        *lock(&self.system) = Some(system);
    }

    /// Where a GPU-capable recognizer should run now (PLAN-09): asks the PC how busy its GPU is.
    /// What the PC is doing, for the profile and the GPU policy: its snapshot, and the GPU's load
    /// when the recognizer `stt` could use the GPU (reading it takes a moment).
    fn sample(&self, stt: Option<&str>) -> (Option<kivo_platform::SystemSnapshot>, Option<u8>) {
        let Some(system) = lock(&self.system).clone() else {
            return (None, None);
        };
        let load = stt
            .filter(|id| crate::gpu::can_use_gpu(id))
            .and_then(|_| system.gpu_load());
        (system.snapshot().ok(), load)
    }

    /// The profile in effect and where the recognizer `stt` runs, from a sample of the PC.
    fn decide(
        &self,
        config: &KivoConfig,
        stt: Option<&str>,
        machine: Option<&kivo_platform::SystemSnapshot>,
        gpu_load: Option<u8>,
    ) -> (crate::profiles::Resolved, Option<GpuTarget>) {
        let resolved = crate::profiles::resolve(config, machine);
        let on_gpu_now = self.infer.configured().gpu.is_some();
        let gpu = match (stt, machine) {
            (Some(id), Some(m))
                if resolved.gpu
                    && config.performance.gpu_allowed()
                    && crate::gpu::can_use_gpu(id) =>
            {
                crate::gpu::target(
                    config.performance.graphics_backend,
                    resolved.profile,
                    m,
                    gpu_load,
                    on_gpu_now,
                    self.cuda_worker().is_some(),
                )
            }
            _ => None,
        };
        (resolved, gpu)
    }

    /// The CUDA worker and NVIDIA's runtime libraries (VOICE-50), once the CUDA pack is installed:
    /// the worker from the pack (a release build), else the one beside the runtime (a development
    /// build, `cargo build -p kivo-infer-cuda --features cuda`).
    pub fn cuda_worker(&self) -> Option<CudaWorker> {
        let libraries = self.store.installed(CUDA_RUNTIME)?.dir;
        let beside = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join(CUDA_WORKER)));
        let program = [Some(libraries.join(CUDA_WORKER)), beside]
            .into_iter()
            .flatten()
            .find(|p| p.is_file())?;
        Some(CudaWorker { program, libraries })
    }

    /// The PC's graphics cards (empty until the system is known).
    fn gpus(&self) -> &[kivo_platform::GpuInfo] {
        if let Some(gpus) = self.gpus.get() {
            return gpus;
        }
        let Some(system) = lock(&self.system).clone() else {
            return &[];
        };
        self.gpus
            .get_or_init(|| system.snapshot().map(|m| m.gpus).unwrap_or_default())
    }

    fn has_nvidia(&self) -> bool {
        self.gpus()
            .iter()
            .any(|g| g.vendor == kivo_platform::GpuVendor::Nvidia)
    }

    /// Settings → Performance → Graphics backend (VOICE-50): the choices this PC has, where the
    /// recognizer runs now, and whether CUDA is installed.
    pub fn graphics(&self) -> serde_json::Value {
        let nvidia = self.has_nvidia();
        let old_driver = self
            .gpus()
            .iter()
            .filter_map(kivo_platform::GpuInfo::nvidia_driver)
            .all(|(major, _)| major < crate::gpu::CUDA_MIN_DRIVER);
        serde_json::json!({
            "options": crate::gpu::options(self.gpus()),
            "running": self.infer.effective_gpu(),
            "cuda": nvidia.then(|| serde_json::json!({
                "model": CUDA_RUNTIME,
                "installed": self.store.installed(CUDA_RUNTIME).is_some(),
                "ready": self.cuda_worker().is_some(),
                "driverTooOld": old_driver,
            })),
        })
    }

    /// Where engine `id` would run now: the graphics card the GPU policy gives it, or `None`
    /// for the processor (test workers and benchmarks run it there too).
    pub fn gpu_for(&self, id: &str, config: &KivoConfig) -> Option<GpuTarget> {
        if !crate::gpu::can_use_gpu(id) {
            return None;
        }
        let (machine, load) = self.sample(Some(id));
        self.decide(config, Some(id), machine.as_ref(), load).1
    }

    /// The threads speech models may use now (the hardware recommendation, PLAN-01).
    pub fn speech_threads(&self) -> usize {
        self.recommended_threads
            .load(std::sync::atomic::Ordering::Relaxed)
            .max(1)
    }

    /// The profile in effect right now (the Performance page).
    pub fn effective_profile(&self, config: &KivoConfig) -> kivo_core::config::PerformanceProfile {
        lock(&self.applied).as_ref().map_or_else(
            || crate::profiles::resolve(config, None).profile,
            |(r, _)| r.profile,
        )
    }

    /// Checks the profile and the GPU policy again (PLAN-08, VOICE-35): Auto follows a game in
    /// front or running on battery, and the recognizer leaves the GPU for games and busy GPUs and
    /// comes back once it's free. Returns whether anything changed.
    pub fn recheck(self: &Arc<Self>, config: &KivoConfig) -> bool {
        let stt = self.infer.configured().stt.map(|(id, _)| id);
        let (machine, load) = self.sample(stt.as_deref());
        let decided = self.decide(config, stt.as_deref(), machine.as_ref(), load);
        if lock(&self.applied).as_ref() == Some(&decided) {
            return false;
        }
        tracing::info!(profile = ?decided.0.profile, gpu = ?decided.1, "speech engines adjust");
        self.apply_with(config, machine.as_ref(), load);
        true
    }

    /// `recheck`, by its older name (the GPU part of it, VOICE-35).
    pub fn recheck_gpu(self: &Arc<Self>, config: &KivoConfig) -> bool {
        self.recheck(config)
    }

    pub fn set_secrets(&self, secrets: Arc<dyn kivo_platform::Secrets>) {
        *lock(&self.secrets) = Some(secrets);
    }

    /// How the worker reaches cloud engine `id`: the user's key for its vendor, and the Azure
    /// region. `None` for a local engine, or when no key is saved yet.
    pub fn cloud_access(
        &self,
        id: &str,
        config: &KivoConfig,
    ) -> Option<kivo_ipc::infer::CloudLoad> {
        let provider = kivo_voice::cloud::provider(id)?;
        let secrets = lock(&self.secrets).clone()?;
        let key = secrets
            .get(&crate::brains::key_handle(provider.vendor))
            .ok()
            .flatten()?;
        Some(kivo_ipc::infer::CloudLoad {
            key: key.expose().clone(),
            base_url: None,
            region: (provider.vendor == "azure-speech" && !config.voice.azure_region.is_empty())
                .then(|| config.voice.azure_region.clone()),
        })
    }

    fn has_key(&self, vendor: &str) -> bool {
        lock(&self.secrets).as_ref().is_some_and(|s| {
            s.get(&crate::brains::key_handle(vendor))
                .ok()
                .flatten()
                .is_some()
        })
    }

    /// Saves the key for cloud engine `engine`'s service after checking it with one small request
    /// (VOICE-10/11); the Azure region goes to the settings. The key is never shown again.
    pub async fn save_key(
        self: &Arc<Self>,
        engine: &str,
        key: &str,
        region: Option<&str>,
    ) -> Result<(), String> {
        let provider = kivo_voice::cloud::provider(engine)
            .ok_or_else(|| kivo_core::text::t("voice.notCloud"))?;
        let key = key.trim().to_owned();
        if key.is_empty() || key.len() > 512 {
            return Err(kivo_core::text::t("voice.badKey"));
        }
        let region = region
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .map(str::to_owned);
        let access = kivo_voice::cloud::CloudAccess {
            key: key.clone(),
            base_url: None,
            region: region.clone(),
        };
        let id = engine.to_owned();
        tokio::task::spawn_blocking(move || kivo_voice::cloud::check(&id, &access))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.detail())?;
        let secrets = lock(&self.secrets)
            .clone()
            .ok_or_else(|| "no key store".to_owned())?;
        secrets
            .set(
                &crate::brains::key_handle(provider.vendor),
                kivo_core::Secret::new(key),
            )
            .map_err(|e| e.to_string())?;
        if let Some(region) = region {
            self.core.update_config(|c| c.voice.azure_region = region);
        }
        self.apply_engines(&self.core.config());
        Ok(())
    }

    /// Forgets cloud engine `engine`'s key; the engine falls back to a local one.
    pub fn delete_key(self: &Arc<Self>, engine: &str) {
        if let (Some(p), Some(secrets)) = (
            kivo_voice::cloud::provider(engine),
            lock(&self.secrets).clone(),
        ) {
            let _ = secrets.delete(&crate::brains::key_handle(p.vendor));
        }
        self.apply_engines(&self.core.config());
    }

    /// Whether cloud engine `id` can be used now: allowed by privacy, and its key saved.
    pub fn cloud_ready(&self, id: &str, config: &KivoConfig) -> bool {
        speech_may_use(id, config) && self.cloud_access(id, config).is_some()
    }

    /// Reads KIVO's latest `stt` and `tts` benchmark runs on this PC (VOICE-42).
    pub fn load_measurements(&self, db: &kivo_store::Database) {
        if let Ok(Some(raw)) = db.meta(VOICE_WER_KEY)
            && let Ok((wers, at)) =
                serde_json::from_str::<(std::collections::BTreeMap<String, f64>, i64)>(&raw)
        {
            self.set_voice_wers(wers, at);
        }
        if let Ok(Some(raw)) = db.meta(ENGINE_BENCH_KEY)
            && let Ok(runs) = serde_json::from_str(&raw)
        {
            *lock(&self.engine_benchmarks) = runs;
        }
        let mut metrics = Vec::new();
        let mut at = 0;
        // GPU runs first: an engine that runs on the graphics card shows those numbers.
        for suite in ["stt-gpu", "stt", "tts"] {
            let Ok(Some((started, json))) = db.latest_benchmark(suite) else {
                continue;
            };
            at = at.max(started);
            let parsed: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
            for metric in parsed["metrics"].as_array().into_iter().flatten() {
                if let (Some(name), Some(p50)) = (metric["name"].as_str(), metric["p50"].as_f64()) {
                    metrics.push((name.to_owned(), p50));
                }
            }
        }
        *lock(&self.measurements) = (metrics, at);
    }

    /// The engine registry with KIVO's measurements on this PC (VOICE-42).
    pub fn registry(&self) -> Vec<RegistryEntry> {
        let mut entries = kivo_voice::registry::registry();
        let (metrics, at) = lock(&self.measurements).clone();
        kivo_voice::registry::apply_measurements(&mut entries, &metrics, at);
        kivo_voice::registry::apply_engine_benchmarks(&mut entries, &lock(&self.engine_benchmarks));
        let (wers, wers_at) = lock(&self.voice_wers).clone();
        kivo_voice::registry::apply_voice_wer(&mut entries, &wers, wers_at);
        entries
    }

    /// Engines ready to run: their model is on this PC, or they need none.
    pub fn ready_engines(&self) -> Vec<String> {
        kivo_voice::engines()
            .into_iter()
            .filter(|e| {
                // A cloud engine is ready once its key is saved.
                if let Some(p) = kivo_voice::cloud::provider(&e.id) {
                    return self.has_key(p.vendor);
                }
                e.model
                    .as_deref()
                    .is_none_or(|m| self.store.installed(m).is_some())
            })
            .map(|e| e.id)
            .collect()
    }

    /// The recommendation for this PC and these settings (VOICE-44).
    pub fn recommend(
        &self,
        machine: &kivo_platform::SystemSnapshot,
        config: &KivoConfig,
        priority: Priority,
    ) -> Recommendation {
        let registry = self.registry();
        let installed = self.ready_engines();
        kivo_voice::recommend::recommend(
            machine,
            &Needs {
                gpu_speech: config.performance.gpu_allowed(),
                language: &config.general.language,
                local_only: !speech_may_leave(config),
                priority,
                installed: &installed,
                registry: &registry,
            },
        )
    }

    /// The recommendation with what the PC is doing right now, if KIVO can read it.
    pub fn recommend_now(&self, config: &KivoConfig, priority: Priority) -> Option<Recommendation> {
        let system = lock(&self.system).clone()?;
        let machine = system.snapshot().ok()?;
        Some(self.recommend(&machine, config, priority))
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
        default_stt(&config.general.language)
    }

    /// The engine KIVO listens with: the chosen one, else the first installed one for the
    /// language, else the language's default (which the user is offered to download).
    pub fn listening_engine(&self, config: &KivoConfig) -> String {
        if !config.voice.stt_engine.trim().is_empty() {
            return Self::wanted_stt(config);
        }
        let ready = self.ready_engines();
        kivo_voice::registry::compatible(EngineSlot::Stt, &config.general.language)
            .into_iter()
            .map(|e| e.engine.id)
            .find(|id| ready.contains(id))
            .unwrap_or_else(|| Self::wanted_stt(config))
    }

    /// Another installed recognizer for the language, used if the chosen one fails (VOICE-47).
    fn stt_fallback(&self, config: &KivoConfig, primary: &str) -> Option<(String, PathBuf)> {
        if !config.voice.stt_fallback {
            return None;
        }
        kivo_voice::registry::compatible(EngineSlot::Stt, &config.general.language)
            .into_iter()
            .map(|e| e.engine.id)
            .filter(|id| id != primary && speech_may_use(id, config))
            .find_map(|id| self.installed_dir(&id).map(|dir| (id, dir)))
    }

    /// Everything KIVO can install, with what is already here (DIST-13).
    pub fn list(&self) -> Vec<ModelItem> {
        let config = self.core.config();
        let nvidia = self.has_nvidia();
        let downloads = self
            .downloads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        catalog()
            .into_iter()
            .filter(|m| m.kind != ModelKind::GpuRuntime || nvidia)
            .map(|m| {
                let installed = self.store.installed(&m.id);
                #[allow(clippy::cast_possible_truncation, reason = "0–100")]
                let downloading = downloads.get(&m.id).map(|(_, p)| percent(*p));
                let error = lock(&self.errors).get(&m.id).cloned();
                let state = match (&installed, downloading) {
                    (_, Some(100)) => "installing",
                    (_, Some(_)) => "downloading",
                    (Some(i), None) if self.store.update_available(i, &m) => "updateAvailable",
                    (Some(_), None) => "ready",
                    (None, None) if error.is_some() => "error",
                    (None, None) if self.store.has_partial(&m.id) => "paused",
                    (None, None) => "notInstalled",
                };
                let in_use = [&config.voice.stt_engine, &config.voice.tts_engine]
                    .iter()
                    .any(|e| {
                        kivo_voice::engine(e).and_then(|e| e.model).as_deref()
                            == Some(m.id.as_str())
                    })
                    || Self::wanted_stt(&config) == m.id;
                ModelItem {
                    state: state.to_owned(),
                    error,
                    in_use,
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
        for needed in &manifest.requires {
            self.install(needed)?;
        }
        {
            let mut downloads = self
                .downloads
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let outdated = self
                .store
                .installed(id)
                .is_some_and(|i| self.store.update_available(&i, &manifest));
            if downloads.contains_key(id) || (self.store.installed(id).is_some() && !outdated) {
                return Ok(());
            }
            lock(&self.errors).remove(id);
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
            let update = models
                .store
                .installed(&id)
                .is_some_and(|i| models.store.update_available(&i, &manifest));
            let fetcher = HttpFetcher::new();
            let mut on_progress = |progress: Progress| {
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
            };
            let result = if update {
                models
                    .store
                    .update(&manifest, &fetcher, &cancel, &mut on_progress)
            } else {
                models
                    .store
                    .install(&manifest, &fetcher, &cancel, &mut on_progress)
            };
            let paused = cancel.is_cancelled();
            models
                .downloads
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&id);
            if paused && result.is_err() {
                // Paused or cancelled by the user: not an error. The partial files stay for a
                // resume unless Cancel removed them.
                models.changed(&id, None, false);
                if hearing {
                    models.core.set_speech_status(SpeechStatus::Missing);
                }
                return;
            }
            match result {
                Ok(installed) => {
                    tracing::info!(model = id, bytes = installed.bytes, "model installed");
                    models.changed(&id, None, true);
                    models.apply_engines(&models.core.config());
                }
                Err(e) => {
                    tracing::error!(%e, model = id, "the model download failed");
                    lock(&models.errors).insert(id.clone(), e.to_string());
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

    /// Pauses a download; installing again resumes it where it stopped (UX-61).
    pub fn pause(&self, id: &str) {
        if let Some((cancel, _)) = lock(&self.downloads).get(id) {
            cancel.cancel();
        }
    }

    /// Stops a download and drops what it had fetched (UX-61).
    pub fn cancel(&self, id: &str) -> Result<(), String> {
        let running = lock(&self.downloads).remove(id);
        if let Some((cancel, _)) = running {
            cancel.cancel();
        }
        lock(&self.errors).remove(id);
        // The download thread may still hold the partial files for a moment.
        for _ in 0..50 {
            match self.store.discard_partial(id) {
                Ok(()) => {
                    self.changed(id, None, false);
                    return Ok(());
                }
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(20)),
            }
        }
        self.store.discard_partial(id).map_err(|e| e.to_string())
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

    /// Deletes a model and its unfinished download. The recognizer KIVO listens with goes only
    /// when another one can take over, or when the user confirmed losing it (VOICE-45).
    pub fn remove(self: &Arc<Self>, id: &str, confirmed: bool) -> Result<(), String> {
        let config = self.core.config();
        if !confirmed
            && self.listening_engine(&config) == id
            && self.store.installed(id).is_some()
            && self.stt_fallback(&config, id).is_none()
        {
            return Err(kivo_core::text::t("voice.removeActive"));
        }
        if let Some((cancel, _)) = self
            .downloads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(id)
        {
            cancel.cancel();
        }
        if id == CUDA_RUNTIME {
            // The CUDA worker holds the pack's libraries open: switch to the other worker first,
            // then remove them once it has let go (a few seconds at most).
            self.infer.set_cuda(None);
            let mut tries = 0;
            while let Err(e) = self.store.remove(id) {
                tries += 1;
                if tries > 50 {
                    return Err(e.to_string());
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        } else {
            self.store.remove(id).map_err(|e| e.to_string())?;
        }
        self.changed(id, None, false);
        self.apply_engines(&self.core.config());
        Ok(())
    }

    /// What the Voice page's advanced view shows (VOICE-49): the exact engines, their models and
    /// where they are, the devices they run on, the timing settings, the thread limit, the
    /// fallback and how long models stay loaded.
    pub fn advanced(self: &Arc<Self>, config: &KivoConfig) -> serde_json::Value {
        let stt = self.listening_engine(config);
        let tts = config.voice.tts_engine.clone();
        let describe = |id: &str| {
            let engine = kivo_voice::engine(id);
            let model = engine.as_ref().and_then(|e| e.model.clone());
            serde_json::json!({
                "engine": id,
                "name": engine.as_ref().map(|e| e.name.clone()),
                "model": model,
                "path": model.as_deref().and_then(|m| self.installed_dir(m)),
                "devices": engine.as_ref().map(|e| e.accel.iter().map(|a| format!("{a:?}").to_lowercase()).collect::<Vec<_>>()),
                "streaming": engine.as_ref().is_some_and(|e| e.streaming),
                "license": engine.as_ref().map(|e| e.license.clone()),
            })
        };
        let recommended = self
            .recommended_threads
            .load(std::sync::atomic::Ordering::Relaxed);
        serde_json::json!({
            "stt": describe(&stt),
            "tts": describe(&tts),
            "sttFallback": self.stt_fallback(config, &stt).map(|(id, _)| id),
            "fallbackOn": config.voice.stt_fallback,
            "threads": threads_for(config).min(recommended.max(1)),
            "threadsSetting": config.performance.speech_threads,
            "cores": std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
            "sttWarmMinutes": config.performance.stt_warm_minutes,
            "ttsWarmMinutes": config.performance.tts_warm_minutes,
            "timing": crate::voice::timing(),
            "modelsFolder": self.store_root(),
        })
    }

    fn store_root(&self) -> PathBuf {
        self.store.root().to_path_buf()
    }

    /// Tells the worker which engines to use (they load on the first request, VOICE-34) and the
    /// UI whether KIVO can hear.
    pub fn apply_engines(self: &Arc<Self>, config: &KivoConfig) {
        let wanted = self.listening_engine(config);
        let (machine, load) = self.sample(Some(&wanted));
        self.apply_with(config, machine.as_ref(), load);
    }

    fn apply_with(
        self: &Arc<Self>,
        config: &KivoConfig,
        machine: Option<&kivo_platform::SystemSnapshot>,
        gpu_load: Option<u8>,
    ) {
        let wanted = self.listening_engine(config);
        let mut cloud = std::collections::BTreeMap::new();
        // A cloud recognizer has no model folder: its key is what it needs (VOICE-10).
        let stt = if kivo_voice::cloud::provider(&wanted).is_some() {
            self.cloud_access(&wanted, config)
                .filter(|_| speech_may_use(&wanted, config))
                .map(|access| {
                    cloud.insert(wanted.clone(), access);
                    (wanted.clone(), PathBuf::new())
                })
        } else {
            self.installed_dir(&wanted)
                .map(|dir| (wanted.clone(), dir))
                .filter(|(id, _)| speech_may_use(id, config))
        };
        let stt_fallback = self.stt_fallback(config, &wanted);
        let ready = stt.is_some() || stt_fallback.is_some();
        let tts = (!config.voice.tts_engine.is_empty())
            .then(|| config.voice.tts_engine.clone())
            .map(|id| self.tts_engine(id, config));
        if let Some((id, _)) = &tts
            && let Some(access) = self.cloud_access(id, config)
        {
            cloud.insert(id.clone(), access);
        }
        // The profile in effect (PLAN-08) and the GPU policy (PLAN-09).
        let decided = self.decide(
            config,
            stt.as_ref().map(|(id, _)| id.as_str()),
            machine,
            gpu_load,
        );
        let (resolved, gpu) = decided.clone();
        *lock(&self.applied) = Some(decided);
        self.infer.set_cuda(self.cuda_worker());
        let warm_minutes = resolved.warm_minutes;
        self.infer.configure(
            Engines {
                stt,
                stt_fallback,
                tts,
                threads: resolved.threads.min(
                    self.recommended_threads
                        .load(std::sync::atomic::Ordering::Relaxed),
                ),
                language: config.general.language.clone(),
                cloud,
                gpu,
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
        // A cloud voice needs its key; without one the Windows voices speak.
        if kivo_voice::cloud::provider(&id).is_some() {
            return if self.cloud_access(&id, config).is_some() {
                (id, None)
            } else {
                system
            };
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

    /// At startup: note what is installed (nothing is loaded yet). A missing speech model is
    /// reported, not downloaded: the user chooses what to install (owner decision).
    pub fn note_speech(self: &Arc<Self>, config: &KivoConfig) {
        self.apply_engines(config);
    }

    /// Where the voice-activity model is once installed. From the repository (development and
    /// tests) the copy in `assets/models` is used until then.
    pub fn vad_model(&self) -> PathBuf {
        let installed = self
            .store
            .dir(kivo_store::models::SILERO_VAD)
            .join("silero_vad.onnx");
        let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/models")
            .join("silero_vad.onnx");
        if !installed.is_file() && repository.is_file() {
            return repository;
        }
        installed
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The language's default recognizer: its Recommended profile, else its Multilingual one.
fn default_stt(language: &str) -> String {
    // An engine KIVO can start with before anything is downloaded for the GPU.
    let cards = kivo_voice::registry::profiles(EngineSlot::Stt, language, false);
    [
        kivo_voice::registry::Profile::Recommended,
        kivo_voice::registry::Profile::Multilingual,
    ]
    .iter()
    .find_map(|p| {
        cards
            .iter()
            .find(|c| c.profile == *p)
            .and_then(|c| c.engine.clone())
    })
    .unwrap_or_else(|| kivo_voice::moonshine::MODEL_ID.to_owned())
}

/// True when the privacy mode lets speech go to a cloud engine.
fn speech_may_leave(config: &KivoConfig) -> bool {
    kivo_security::privacy::speech_egress(true, &config.privacy, &config.capabilities).is_ok()
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
    crate::profiles::resolve(config, None).threads
}

/// The privacy check for a speech engine (VOICE-07): audio and text go to a cloud engine only
/// when the privacy mode and the Cloud AI capability allow it.
fn speech_may_use(engine: &str, config: &KivoConfig) -> bool {
    let cloud = kivo_voice::engine(engine).is_some_and(|e| e.kind == kivo_voice::EngineKind::Cloud);
    match kivo_security::privacy::speech_egress(cloud, &config.privacy, &config.capabilities) {
        Ok(()) => true,
        Err(denied) => {
            tracing::info!(engine, reason = %denied.message, "speech engine not used");
            false
        }
    }
}
/// Minutes speech models stay loaded after use (VOICE §8); none in low-memory mode, which
/// unloads them as soon as a request is done (UX §5, General).
pub fn warm_minutes(config: &KivoConfig) -> u64 {
    crate::profiles::resolve(config, None).warm_minutes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(all(windows, target_arch = "x86_64"))]
    /// Marks model `id` installed (a manifest with no files), plus `extra` files in its folder.
    fn fake_install(dir: &std::path::Path, id: &str, extra: &[&str]) {
        let mut manifest = kivo_store::models::catalog()
            .into_iter()
            .find(|m| m.id == id)
            .unwrap();
        manifest.files.clear();
        let folder = dir.join(id);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        for name in extra {
            std::fs::write(folder.join(name), b"").unwrap();
        }
    }

    #[test]
    #[cfg(all(windows, target_arch = "x86_64"))]
    fn the_recognizer_leaves_the_gpu_for_games_and_busy_gpus() {
        use kivo_ipc::infer::GpuBackend;
        let dir = tempfile::tempdir().unwrap();
        let core = Arc::new(Core::with_config(KivoConfig::default(), None));
        let (infer, _events, _sender) = Infer::new(dir.path().join("no-worker.exe"));
        let models = Arc::new(Models::new(
            dir.path().to_path_buf(),
            Arc::clone(&core),
            infer.clone(),
        ));
        const WHISPER: &str = "whisper-cpp-small";
        fake_install(dir.path(), WHISPER, &[]);
        let system = Arc::new(kivo_testkit::FakeSystemInfo::default());
        let rtx = "NVIDIA GeForce RTX 3060 Laptop GPU";
        system.snapshot.lock().unwrap().gpus = vec![
            kivo_platform::GpuInfo {
                name: "AMD Radeon(TM) Graphics".into(),
                vram_mb: 512,
                vendor: kivo_platform::GpuVendor::Amd,
                driver_version: None,
            },
            kivo_platform::GpuInfo {
                name: rtx.into(),
                vram_mb: 6_144,
                vendor: kivo_platform::GpuVendor::Nvidia,
                driver_version: Some("32.0.16.1692".into()),
            },
        ];
        *system.gpu_load.lock().unwrap() = Some(5);
        models.set_system(system.clone());
        core.update_config(|c| c.voice.stt_engine = WHISPER.into());
        models.apply_engines(&core.config());
        assert_eq!(infer.configured().stt.unwrap().0, WHISPER);
        let on = |backend| {
            Some(GpuTarget {
                backend,
                device: rtx.into(),
            })
        };
        assert_eq!(
            infer.configured().gpu,
            on(GpuBackend::Vulkan),
            "the NVIDIA card, not the integrated one listed first; Vulkan until CUDA is installed"
        );
        assert_eq!(infer.cuda(), None);
        // Something else keeps the GPU busy: off it (VOICE-35).
        *system.gpu_load.lock().unwrap() = Some(80);
        assert!(models.recheck_gpu(&core.config()));
        assert_eq!(infer.configured().gpu, None);
        // Still 40 %: not back yet; 10 %: back.
        *system.gpu_load.lock().unwrap() = Some(40);
        assert!(!models.recheck_gpu(&core.config()));
        *system.gpu_load.lock().unwrap() = Some(10);
        assert!(models.recheck_gpu(&core.config()));
        assert_eq!(infer.configured().gpu, on(GpuBackend::Vulkan));
        // The CUDA pack arrives (with its worker): CUDA on the same card, from that worker.
        fake_install(dir.path(), CUDA_RUNTIME, &[CUDA_WORKER]);
        models.apply_engines(&core.config());
        assert_eq!(infer.configured().gpu, on(GpuBackend::Cuda));
        let cuda = infer.cuda().unwrap();
        assert_eq!(
            cuda.program,
            dir.path().join(CUDA_RUNTIME).join(CUDA_WORKER)
        );
        assert_eq!(cuda.libraries, dir.path().join(CUDA_RUNTIME));
        // The user picks Vulkan, then Processor only.
        core.update_config(|c| {
            c.performance.graphics_backend = kivo_core::config::GraphicsBackend::Vulkan;
        });
        models.apply_engines(&core.config());
        assert_eq!(infer.configured().gpu, on(GpuBackend::Vulkan));
        core.update_config(|c| {
            c.performance.graphics_backend = kivo_core::config::GraphicsBackend::Processor;
        });
        models.apply_engines(&core.config());
        assert_eq!(infer.configured().gpu, None);
        core.update_config(|c| {
            c.performance.graphics_backend = kivo_core::config::GraphicsBackend::Auto;
        });
        // Removing the pack lets go of the CUDA worker first.
        models.remove(CUDA_RUNTIME, true).unwrap();
        assert_eq!(infer.cuda(), None);
        assert_eq!(infer.configured().gpu, on(GpuBackend::Vulkan));
        // The Gaming profile: off the GPU whatever it's doing.
        core.update_config(|c| {
            c.performance.profile = kivo_core::config::PerformanceProfile::Gaming
        });
        assert!(models.recheck_gpu(&core.config()));
        assert_eq!(infer.configured().gpu, None);
        // A recognizer that can't use the GPU is never moved.
        core.update_config(|c| {
            c.performance.profile = kivo_core::config::PerformanceProfile::Auto;
            c.voice.stt_engine = kivo_voice::moonshine::MODEL_ID.into();
        });
        models.apply_engines(&core.config());
        assert!(!models.recheck_gpu(&core.config()));
    }

    #[test]
    fn a_cloud_voice_needs_its_key_and_the_privacy_mode() {
        use kivo_platform::Secrets as _;
        let dir = tempfile::tempdir().unwrap();
        let core = Arc::new(Core::with_config(KivoConfig::default(), None));
        let (infer, _events, _sender) = Infer::new(dir.path().join("no-worker.exe"));
        let models = Arc::new(Models::new(
            dir.path().to_path_buf(),
            Arc::clone(&core),
            infer.clone(),
        ));
        let secrets = Arc::new(kivo_testkit::FakeSecrets::default());
        models.set_secrets(secrets.clone());
        core.update_config(|c| {
            c.voice.tts_engine = kivo_voice::cloud::ELEVENLABS.into();
            c.capabilities.set(kivo_core::Capability::CloudBrains, true);
        });
        // No key yet: the Windows voices speak, and the engine isn't "ready".
        models.apply_engines(&core.config());
        assert_eq!(infer.configured().tts.unwrap().0, "system");
        assert!(
            !models
                .ready_engines()
                .contains(&kivo_voice::cloud::ELEVENLABS.to_owned())
        );
        // With a key: the cloud voice, and the worker gets the key.
        secrets
            .set(
                &crate::brains::key_handle("elevenlabs"),
                kivo_core::Secret::new("xi-key".into()),
            )
            .unwrap();
        models.apply_engines(&core.config());
        let engines = infer.configured();
        assert_eq!(engines.tts.unwrap().0, kivo_voice::cloud::ELEVENLABS);
        assert_eq!(engines.cloud[kivo_voice::cloud::ELEVENLABS].key, "xi-key");
        assert!(
            models
                .ready_engines()
                .contains(&kivo_voice::cloud::ELEVENLABS.to_owned())
        );
        // Local-only privacy: back to the Windows voices, and no key goes anywhere.
        core.update_config(|c| c.privacy.mode = kivo_core::config::PrivacyMode::Local);
        models.apply_engines(&core.config());
        let engines = infer.configured();
        assert_eq!(engines.tts.unwrap().0, "system");
        assert!(engines.cloud.is_empty());
    }

    #[test]
    fn low_memory_mode_unloads_speech_models_after_use() {
        let mut config = KivoConfig::default();
        config.performance.stt_warm_minutes = 5;
        config.performance.tts_warm_minutes = 10;
        assert_eq!(warm_minutes(&config), 10);
        config.general.low_memory_mode = true;
        assert_eq!(warm_minutes(&config), 0);
    }

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
        config.voice.stt_engine.clear();
        config.general.language = "es-ES".into();
        assert_eq!(Models::wanted_stt(&config), "moonshine-base-es");
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
