//! A scripted KIVO for end-to-end tests and the `e2e` benchmark (BENCH-08): the real turn engine,
//! speech worker and models, with a scripted microphone (Windows' own voice speaking the
//! request) and fake apps, windows and system controls, so a journey like "mute" never touches
//! the real desktop.

use crate::core::Core;
use crate::engine::{self, Engine};
use crate::infer::{self, Infer};
use crate::{activity, speaker, voice};
use kivo_ipc::protocol::SpeechStatus;
use kivo_platform::{AppEntry, SpeechSynth};
use kivo_store::Database;
use kivo_testkit::{FakeApps, FakeAudio, FakeNotifications, FakeSystemControl, FakeWindows};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

/// Windows' own voice, at 16 kHz mono, as the microphone would hear it.
pub fn spoken(text: &str) -> Vec<f32> {
    let audio = kivo_platform_windows::WindowsSpeech
        .synthesize(text, None)
        .expect("Windows has a voice");
    let mut clip = kivo_audio::RateConverter::convert_all(audio.rate, 16_000, &audio.samples);
    // A moment of quiet at the end, so the pipeline sees the sentence finish.
    clip.extend(std::iter::repeat_n(0.0, 16_000));
    clip
}

/// Windows' voice stand-in: records what KIVO says in-process (a failure, ARCH-09).
#[derive(Default)]
pub struct Heard(pub Mutex<Vec<String>>);

impl SpeechSynth for Heard {
    fn voices(&self) -> kivo_platform::PlatformResult<Vec<kivo_platform::SystemVoice>> {
        Ok(Vec::new())
    }
    fn synthesize(
        &self,
        text: &str,
        _voice: Option<&str>,
    ) -> kivo_platform::PlatformResult<kivo_platform::SynthAudio> {
        self.0.lock().unwrap().push(text.to_owned());
        Ok(kivo_platform::SynthAudio {
            rate: 16_000,
            samples: vec![0.0; 160],
        })
    }
}

/// A whole KIVO turn engine on fakes: the real speech worker and models, a scripted microphone,
/// and apps, windows and system controls that only record what they were asked.
pub struct Rig {
    pub engine: Arc<Engine>,
    pub infer: Infer,
    pub heard: Arc<Heard>,
    pub listener: Arc<voice::Listener>,
    pub system: Arc<kivo_testkit::FakeSystemInfo>,
    pub windows: Arc<FakeWindows>,
    pub core: Arc<Core>,
    pub apps: Arc<FakeApps>,
    pub recorder: activity::Recorder,
    pub db: Arc<Mutex<Database>>,
    /// The scripted microphone: queue what the user says next with `say_next`.
    pub audio: Arc<FakeAudio>,
    /// Where "take a screenshot" saves: a folder of this rig's own under the temp folder.
    pub screenshots: PathBuf,
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.screenshots);
    }
}

/// A rig speaking with the Windows voices. `worker` is the `kivo-infer` program.
pub fn rig(
    worker: PathBuf,
    clip: Vec<f32>,
    model_dir: PathBuf,
) -> (
    Rig,
    tokio::task::JoinHandle<()>,
    tokio::task::JoinHandle<()>,
) {
    rig_with_voice(worker, clip, model_dir, ("system".to_owned(), None))
}

/// A rig with a chosen voice (`("kokoro-82m", Some(dir))`).
pub fn rig_with_voice(
    worker: PathBuf,
    clip: Vec<f32>,
    model_dir: PathBuf,
    voice: (String, Option<PathBuf>),
) -> (
    Rig,
    tokio::task::JoinHandle<()>,
    tokio::task::JoinHandle<()>,
) {
    let core = Arc::new(Core::with_config(kivo_core::KivoConfig::default(), None));
    let db = Arc::new(Mutex::new(Database::in_memory().unwrap()));
    let recorder = activity::Recorder::new(Arc::clone(&db), true);
    let (infer, mut events, sender) = Infer::new(worker);
    infer.configure(
        infer::Engines {
            stt: model_dir
                .is_dir()
                .then(|| (kivo_voice::moonshine::MODEL_ID.to_owned(), model_dir)),
            tts: Some(voice),
            threads: 4,
        },
        Duration::from_secs(600),
    );
    let chrome = AppEntry {
        id: "Chrome".into(),
        name: "Google Chrome".into(),
        aliases: vec!["Chrome".into()],
        exe: None,
    };
    let apps = Arc::new(FakeApps {
        installed: vec![chrome.clone()],
        ..Default::default()
    });
    let windows = Arc::new(FakeWindows::default());
    let audio = Arc::new(FakeAudio::microphone(clip));
    let speaker = Arc::new(speaker::Speaker::new(audio.clone(), None));
    let mic = Arc::clone(&audio);
    let catalog = Arc::new(RwLock::new(vec![chrome]));
    static RIGS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let screenshots = std::env::temp_dir().join(format!(
        "kivo-scripted-{}-{}",
        std::process::id(),
        RIGS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let env = Arc::new(kivo_tools::Env {
        apps: apps.clone(),
        windows: windows.clone(),
        control: Arc::new(FakeSystemControl::default()),
        screen: Arc::new(PlainScreen),
        notifications: Arc::new(FakeNotifications::default()),
        catalog: Arc::clone(&catalog),
        screenshots: screenshots.clone(),
    });
    let registry = Arc::new(kivo_tools::Registry::new(kivo_tools::builtin(&env)));
    let heard = Arc::new(Heard::default());
    let system = Arc::new(kivo_testkit::FakeSystemInfo::default());
    let engine = Arc::new(Engine::new(engine::Parts {
        core: Arc::clone(&core),
        infer: infer.clone(),
        speaker: Arc::clone(&speaker),
        registry,
        recorder: recorder.clone(),
        apps: apps.clone(),
        windows: windows.clone(),
        app_catalog: catalog,
        router: kivo_intent::IntentRouter::new(kivo_intent::Grammar::bundled("en").unwrap()),
        system: system.clone(),
        fallback_voice: Some(heard.clone()),
    }));
    engine.refresh_apps();
    let (signals, mut voice_signals) = tokio::sync::mpsc::unbounded_channel();
    let (levels, _levels_rx) = tokio::sync::watch::channel(0.0);
    let listener = Arc::new(voice::start(voice::Pipeline {
        audio,
        device: None,
        vad_model: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/models/silero_vad.onnx"),
        infer: infer.clone(),
        speaker,
        levels,
        signals,
        qos: Arc::new(kivo_testkit::FakeThreadQos::default()),
    }));
    engine.set_listener(Arc::clone(&listener));
    core.set_speech_status(SpeechStatus::Ready);

    let worker_task = tokio::spawn(infer::supervise(infer.clone(), sender, core.shutdown()));
    let pump = {
        let engine = Arc::clone(&engine);
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    signal = voice_signals.recv() => match signal {
                        Some(signal) => engine::handle_signal(&engine, signal).await,
                        None => break,
                    },
                    event = events.recv() => match event {
                        Some(event) => engine::handle_infer_event(&engine, event).await,
                        None => break,
                    },
                }
            }
        })
    };
    (
        Rig {
            engine,
            infer,
            heard,
            listener,
            system,
            windows,
            core,
            apps,
            recorder,
            db,
            audio: mic,
            screenshots,
        },
        worker_task,
        pump,
    )
}

/// A screen that is one small grey picture, so a screenshot never captures this PC's desktop.
pub struct PlainScreen;

impl kivo_platform::Screen for PlainScreen {
    fn capture(
        &self,
        _target: kivo_platform::CaptureTarget,
    ) -> kivo_platform::PlatformResult<kivo_platform::Image> {
        Ok(kivo_platform::Image {
            width: 16,
            height: 9,
            rgba: [128, 128, 128, 255].repeat(16 * 9),
        })
    }
}
