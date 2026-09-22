//! The M1 acceptance journey, end to end, with no microphone and nobody talking (plan §138,
//! PLAN-19, BENCH-08): Windows' own voice says "Open Chrome", that audio is fed in as if it came
//! from the microphone, and KIVO transcribes it, routes it through the grammar and the permission
//! engine, launches the app and answers — all recorded in Activity and the audit log.
//!
//! It runs only where the speech model is installed (the first run downloads it); elsewhere it
//! reports that and passes, so CI without the model still goes green.

#![cfg(windows)]

use kivo_core::event::TurnSource;
use kivo_core::{Capability, SessionState};
use kivo_ipc::protocol::SpeechStatus;
use kivo_platform::{AppEntry, Paths, SpeechSynth};
use kivo_store::Database;
use kivo_store::models::ModelStore;
use kivo_testkit::{FakeApps, FakeAudio, FakeNotifications, FakeSystemControl, FakeWindows};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use kivo_runtime::core::Core;
use kivo_runtime::engine::{self, Engine};
use kivo_runtime::infer::{self, Infer};
use kivo_runtime::{activity, speaker, voice};

/// The worker built beside this test binary.
fn worker() -> PathBuf {
    let mut dir = std::env::current_exe().expect("test binary");
    dir.pop(); // deps
    dir.pop();
    dir.join("kivo-infer.exe")
}

/// Windows' own voice, at 16 kHz mono, as the microphone would hear it.
fn spoken(text: &str) -> Vec<f32> {
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
struct Heard(Mutex<Vec<String>>);

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

struct Rig {
    engine: Arc<Engine>,
    infer: Infer,
    heard: Arc<Heard>,
    listener: Arc<voice::Listener>,
    system: Arc<kivo_testkit::FakeSystemInfo>,
    windows: Arc<FakeWindows>,
    core: Arc<Core>,
    apps: Arc<FakeApps>,
    recorder: activity::Recorder,
    db: Arc<Mutex<Database>>,
}

fn rig(
    clip: Vec<f32>,
    model_dir: PathBuf,
) -> (
    Rig,
    tokio::task::JoinHandle<()>,
    tokio::task::JoinHandle<()>,
) {
    let core = Arc::new(Core::with_config(kivo_core::KivoConfig::default(), None));
    let db = Arc::new(Mutex::new(Database::in_memory().unwrap()));
    let recorder = activity::Recorder::new(Arc::clone(&db), true);
    let (infer, mut events, sender) = Infer::new(worker());
    infer.configure(
        infer::Engines {
            stt: model_dir
                .is_dir()
                .then(|| (kivo_voice::moonshine::MODEL_ID.to_owned(), model_dir)),
            tts: Some("system".to_owned()),
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
    let catalog = Arc::new(RwLock::new(vec![chrome]));
    let env = Arc::new(kivo_tools::Env {
        apps: apps.clone(),
        windows: windows.clone(),
        control: Arc::new(FakeSystemControl::default()),
        screen: Arc::new(NoScreen),
        notifications: Arc::new(FakeNotifications::default()),
        catalog: Arc::clone(&catalog),
        screenshots: std::env::temp_dir(),
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
        },
        worker_task,
        pump,
    )
}

struct NoScreen;

impl kivo_platform::Screen for NoScreen {
    fn capture(
        &self,
        _target: kivo_platform::CaptureTarget,
    ) -> kivo_platform::PlatformResult<kivo_platform::Image> {
        Err(kivo_platform::PlatformError::Unsupported)
    }
}

/// Waits for `check` to hold, or gives up.
async fn until(what: &str, timeout: Duration, mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("timed out waiting for {what}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_spoken_command_opens_the_app_and_kivo_answers() {
    let paths = Paths::user().expect("per-user folders");
    let Some(model) = ModelStore::new(paths.models()).installed(kivo_voice::moonshine::MODEL_ID)
    else {
        eprintln!("the speech model isn't installed on this machine; skipping the spoken journey");
        return;
    };
    if !worker().is_file() {
        eprintln!("kivo-infer isn't built beside the tests; skipping");
        return;
    }
    let _ = tracing_subscriber::fmt()
        .with_env_filter("debug,ort=warn")
        .with_test_writer()
        .try_init();
    let (rig, worker_task, pump) = rig(spoken("Open Chrome."), model.dir);

    rig.engine
        .talk(TurnSource::PushToTalk)
        .await
        .expect("KIVO starts listening");
    let deadline = Instant::now() + Duration::from_secs(30);
    while rig.apps.launched.lock().unwrap().is_empty() {
        assert!(
            Instant::now() < deadline,
            "the app never opened; the Island showed {:?} in {:?}",
            rig.core.turn_view(),
            rig.core.state().borrow().session
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(*rig.apps.launched.lock().unwrap(), ["Chrome"]);

    // What the Island showed: the transcript, the step and the answer.
    let turn = rig.core.turn_view().expect("the Island shows the turn");
    assert!(
        turn.transcript.to_lowercase().contains("chrome"),
        "heard: {}",
        turn.transcript
    );
    assert_eq!(turn.steps.len(), 1);
    assert_eq!(turn.steps[0].title, "Open Google Chrome");
    until("the answer", Duration::from_secs(10), || {
        rig.core.turn_view().and_then(|t| t.answer).is_some()
    })
    .await;
    assert_eq!(
        rig.core.turn_view().unwrap().answer.as_deref(),
        Some("Opening Google Chrome.")
    );

    // The turn ends by itself and the Island clears (UX-10).
    until("the turn to end", Duration::from_secs(20), || {
        rig.core.state().borrow().session == SessionState::Idle && rig.core.turn_view().is_none()
    })
    .await;

    // Activity has the whole story, and the audit chain is intact (ARCH-23, SEC-22).
    let timeline = rig.recorder.recent(None, 10);
    let kinds: Vec<&str> = timeline.iter().map(|a| a.kind.as_str()).collect();
    assert!(
        kinds.contains(&"transcript") && kinds.contains(&"tool") && kinds.contains(&"reply"),
        "{kinds:?}"
    );
    let audit = rig.recorder.audit_rows(10);
    assert!(
        audit
            .iter()
            .any(|a| a.tool == "apps.launch" && a.decision == "allow")
    );
    assert_eq!(
        rig.db.lock().unwrap().verify_audit().unwrap().broken_at,
        None
    );

    // The timings were kept (ARCH-28): end of speech to the answer is the M1 budget.
    let spans = rig
        .db
        .lock()
        .unwrap()
        .turn_metrics("t1")
        .unwrap()
        .expect("the turn's timings were recorded");
    let at = |span: &str| spans.get(span).and_then(serde_json::Value::as_u64);
    let (end_of_speech, done) = (at("t4EndOfSpeech").unwrap(), at("t8ToolDone").unwrap());
    eprintln!("end of speech → action: {} ms", done - end_of_speech);
    assert!(at("t5FinalTranscript").is_some() && at("t10Complete").is_some());

    rig.core.quit();
    let _ = tokio::time::timeout(Duration::from_secs(5), worker_task).await;
    pump.abort();
}

/// A typed request takes the same path, with no speech at all (UX §8).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_typed_command_is_treated_like_a_spoken_one() {
    let (rig, worker_task, pump) = rig(Vec::new(), PathBuf::from("no-model"));
    // Speaking replies is on by default, but typed requests stay silent unless asked (UX §8).
    rig.core.update_config(|c| {
        c.capabilities.set(Capability::SpeakResponses, false);
    });

    // The window in front, on a second monitor: the Island is anchored there (UX-06).
    rig.windows
        .windows
        .lock()
        .unwrap()
        .push(kivo_platform::WindowInfo {
            id: kivo_platform::WindowId(7),
            title: "Notes".into(),
            app_id: "notes".into(),
            bounds: kivo_platform::Rect {
                x: 1920,
                y: 0,
                width: 1600,
                height: 900,
            },
            minimized: false,
        });
    rig.engine
        .say("mute")
        .await
        .expect("the request is accepted");
    assert_eq!(
        rig.core.turn_view().and_then(|t| t.anchor),
        Some(kivo_ipc::protocol::ScreenPoint { x: 2720, y: 450 })
    );
    until("the sound to be muted", Duration::from_secs(10), || {
        rig.core.turn_view().and_then(|t| t.answer).is_some()
    })
    .await;
    assert_eq!(
        rig.core.turn_view().unwrap().answer.as_deref(),
        Some("Muted.")
    );
    // Routed by the grammar, with no AI (BRAIN-05).
    let metrics = rig.engine.router_metrics();
    assert_eq!((metrics.requests, metrics.fast_path), (1, 1));
    assert!((metrics.fast_path_ratio - 1.0).abs() < f64::EPSILON);
    until("the turn to end", Duration::from_secs(20), || {
        rig.core.state().borrow().session == SessionState::Idle
    })
    .await;
    rig.core.quit();
    let _ = tokio::time::timeout(Duration::from_secs(5), worker_task).await;
    pump.abort();
}

/// ARCH-09: the speech worker crashes in the middle of a turn. The turn fails with a message that
/// is shown and spoken (in-process, since the worker's voices are gone), KIVO keeps running, and
/// the worker comes back on the next use.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_crashed_speech_worker_fails_the_turn_aloud_and_comes_back() {
    let paths = Paths::user().expect("per-user folders");
    let Some(model) = ModelStore::new(paths.models()).installed(kivo_voice::moonshine::MODEL_ID)
    else {
        eprintln!("the speech model isn't installed on this machine; skipping");
        return;
    };
    if !worker().is_file() {
        eprintln!("kivo-infer isn't built beside the tests; skipping");
        return;
    }
    // Twenty seconds of quiet: the turn is still listening when the worker dies.
    let (rig, worker_task, pump) = rig(vec![0.0; 16_000 * 20], model.dir);
    rig.engine
        .talk(TurnSource::PushToTalk)
        .await
        .expect("KIVO starts listening");
    assert!(
        rig.infer.wait_ready(Duration::from_secs(30)).await,
        "the worker started"
    );
    let first = rig.infer.worker_pid().expect("a worker is running");

    // The crash: the worker process is killed from outside.
    let killed = std::process::Command::new("taskkill")
        .args(["/F", "/PID", &first.to_string()])
        .output()
        .expect("taskkill runs");
    assert!(killed.status.success(), "{killed:?}");

    let expected = kivo_core::text::t("turn.ttsLost");
    until("the turn to fail", Duration::from_secs(10), || {
        rig.core.turn_view().and_then(|t| t.error).as_deref() == Some(expected.as_str())
    })
    .await;
    until("the failure to be spoken", Duration::from_secs(10), || {
        rig.heard.0.lock().unwrap().contains(&expected)
    })
    .await;
    assert_ne!(rig.core.state().borrow().session, SessionState::Listening);

    // The next use starts a new worker.
    rig.infer.warm();
    assert!(
        rig.infer.wait_ready(Duration::from_secs(30)).await,
        "the worker came back"
    );
    assert_ne!(rig.infer.worker_pid(), Some(first));

    rig.core.quit();
    let _ = tokio::time::timeout(Duration::from_secs(5), worker_task).await;
    pump.abort();
}

/// ARCH-26: Stop, Esc, the Island's X and the emergency stop all cancel the turn through
/// `Engine::cancel`, and every layer has stopped within 100 ms: recognition and the microphone
/// while listening, the voice while speaking. (The tool layer's own ≤ 100 ms check is in
/// `kivo-tools`; barge-in (M2) takes the same path.)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancelling_stops_every_layer_within_100_ms() {
    const BUDGET: Duration = Duration::from_millis(100);
    if !worker().is_file() {
        eprintln!("kivo-infer isn't built beside the tests; skipping");
        return;
    }
    let paths = Paths::user().expect("per-user folders");
    let model = ModelStore::new(paths.models()).installed(kivo_voice::moonshine::MODEL_ID);

    // While listening (needs the speech model): the mic closes and the session is idle.
    if let Some(model) = model {
        let (rig, worker_task, pump) = rig(vec![0.0; 16_000 * 20], model.dir);
        for reason in [
            kivo_core::event::CancelReason::Hotkey,
            kivo_core::event::CancelReason::UserButton,
            kivo_core::event::CancelReason::EmergencyStop,
        ] {
            rig.engine
                .talk(TurnSource::PushToTalk)
                .await
                .expect("listening");
            until("listening", Duration::from_secs(10), || {
                rig.listener.is_listening()
            })
            .await;
            let start = Instant::now();
            if reason == kivo_core::event::CancelReason::EmergencyStop {
                rig.engine.stop_everything();
            } else {
                rig.engine.cancel(reason);
            }
            until("everything to stop", BUDGET, || {
                !rig.listener.is_listening()
                    && rig.core.state().borrow().session == SessionState::Idle
            })
            .await;
            eprintln!("{reason:?} while listening: {:?}", start.elapsed());
        }
        rig.core.quit();
        let _ = tokio::time::timeout(Duration::from_secs(5), worker_task).await;
        pump.abort();
    } else {
        eprintln!("the speech model isn't installed; skipping the listening half");
    }

    // While speaking: the voice stops.
    let (rig, worker_task, pump) = rig(Vec::new(), PathBuf::from("no-model"));
    // Typed requests are silent unless asked (UX §8); this one should speak.
    rig.core
        .update_config(|c| c.voice.speak_typed_replies = true);
    rig.engine.say("mute").await.expect("accepted");
    until("KIVO to speak", Duration::from_secs(20), || {
        rig.engine.speaker.speaking()
    })
    .await;
    let start = Instant::now();
    rig.engine
        .cancel(kivo_core::event::CancelReason::UserButton);
    until("the voice to stop", BUDGET, || {
        !rig.engine.speaker.speaking() && rig.core.state().borrow().session == SessionState::Idle
    })
    .await;
    eprintln!("cancel while speaking: {:?}", start.elapsed());
    rig.core.quit();
    let _ = tokio::time::timeout(Duration::from_secs(5), worker_task).await;
    pump.abort();
}

/// UX-11: over a fullscreen app (or in Focus) the Island stays out of the way as the setting says
/// (hidden by default, or a tiny pill) and KIVO answers with sounds only, never speech.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn over_a_fullscreen_app_kivo_stays_quiet() {
    if !worker().is_file() {
        eprintln!("kivo-infer isn't built beside the tests; skipping");
        return;
    }
    let (rig, worker_task, pump) = rig(Vec::new(), PathBuf::from("no-model"));
    rig.system.snapshot.lock().unwrap().fullscreen_app = true;
    rig.core
        .update_config(|c| c.voice.speak_typed_replies = true);
    for (setting, quiet) in [
        (
            kivo_core::config::FullscreenBehavior::Hide,
            kivo_ipc::protocol::QuietIsland::Hidden,
        ),
        (
            kivo_core::config::FullscreenBehavior::TinyPill,
            kivo_ipc::protocol::QuietIsland::Tiny,
        ),
    ] {
        rig.core
            .update_config(|c| c.overlay.in_fullscreen = setting);
        rig.engine.say("mute").await.expect("accepted");
        assert_eq!(rig.core.turn_view().and_then(|t| t.quiet), Some(quiet));
        until("the answer", Duration::from_secs(20), || {
            rig.core.turn_view().and_then(|t| t.answer).is_some()
        })
        .await;
        until("the turn to end", Duration::from_secs(20), || {
            rig.core.state().borrow().session == SessionState::Idle
        })
        .await;
        rig.core.clear_turn();
    }
    // Neither turn produced spoken audio (t9 is the first audio of a spoken reply).
    for turn in ["t1", "t2"] {
        let spans = rig
            .db
            .lock()
            .unwrap()
            .turn_metrics(turn)
            .unwrap()
            .expect("the turn's timings were recorded");
        assert!(spans.get("t9FirstAudio").is_none(), "{turn} spoke: {spans}");
    }
    rig.core.quit();
    let _ = tokio::time::timeout(Duration::from_secs(5), worker_task).await;
    pump.abort();
}
