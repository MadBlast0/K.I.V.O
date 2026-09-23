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
use kivo_platform::Paths;
use kivo_runtime::scripted::{self, Rig, spoken};
use kivo_store::models::ModelStore;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// The worker built beside this test binary.
fn worker() -> PathBuf {
    let mut dir = std::env::current_exe().expect("test binary");
    dir.pop(); // deps
    dir.pop();
    dir.join("kivo-infer.exe")
}

fn rig(
    clip: Vec<f32>,
    model_dir: PathBuf,
) -> (
    Rig,
    tokio::task::JoinHandle<()>,
    tokio::task::JoinHandle<()>,
) {
    scripted::rig(worker(), clip, model_dir)
}

fn rig_with_voice(
    clip: Vec<f32>,
    model_dir: PathBuf,
    voice: (String, Option<PathBuf>),
) -> (
    Rig,
    tokio::task::JoinHandle<()>,
    tokio::task::JoinHandle<()>,
) {
    scripted::rig_with_voice(worker(), clip, model_dir, voice)
}

/// The journeys run one at a time: each drives real speech models, and in parallel they starve
/// each other of CPU (one person uses KIVO at a time).
static ONE_AT_A_TIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
    let _turn = ONE_AT_A_TIME.lock().await;
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

    let pressed = Instant::now();
    rig.engine
        .talk(TurnSource::PushToTalk)
        .await
        .expect("KIVO starts listening");
    // The listening cue is queued at once (VOICE-24: wake → audio ≤ 150 ms).
    until("the listening cue", Duration::from_millis(150), || {
        rig.engine.speaker.busy()
    })
    .await;
    eprintln!("press → listening cue: {:?}", pressed.elapsed());
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
    let _turn = ONE_AT_A_TIME.lock().await;
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
    let _turn = ONE_AT_A_TIME.lock().await;
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
    let _turn = ONE_AT_A_TIME.lock().await;
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
    let _turn = ONE_AT_A_TIME.lock().await;
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

/// VOICE-09: with Kokoro chosen, the worker loads it and a reply is spoken in its voice. Runs
/// where a Kokoro folder is available (`KIVO_KOKORO_DIR`, or the installed model).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn replies_can_be_spoken_by_kokoro() {
    let _turn = ONE_AT_A_TIME.lock().await;
    let dir = std::env::var_os("KIVO_KOKORO_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            Paths::user()
                .and_then(|p| ModelStore::new(p.models()).installed("kokoro-82m"))
                .map(|m| m.dir)
        })
        .filter(|d| d.join("model_quantized.onnx").is_file());
    let Some(dir) = dir else {
        eprintln!("Kokoro isn't available here; skipping");
        return;
    };
    if !worker().is_file() {
        eprintln!("kivo-infer isn't built beside the tests; skipping");
        return;
    }
    let (rig, worker_task, pump) = rig_with_voice(
        Vec::new(),
        PathBuf::from("no-model"),
        ("kokoro-82m".to_owned(), Some(dir)),
    );
    rig.core
        .update_config(|c| c.voice.speak_typed_replies = true);
    rig.engine.say("mute").await.expect("accepted");
    until("the reply to be spoken", Duration::from_secs(60), || {
        rig.core.state().borrow().session == SessionState::Idle
    })
    .await;
    let spans = rig
        .db
        .lock()
        .unwrap()
        .turn_metrics("t1")
        .unwrap()
        .expect("the turn's timings were recorded");
    assert!(
        spans.get("t9FirstAudio").is_some(),
        "Kokoro produced audio: {spans}"
    );
    eprintln!("first audio at {} ms", spans["t9FirstAudio"]);
    rig.core.quit();
    let _ = tokio::time::timeout(Duration::from_secs(5), worker_task).await;
    pump.abort();
}

/// PLAN-19, the §138 journey "Kivo, mute": said aloud, handled by the grammar with no AI, and a
/// brief confirmation.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn saying_mute_mutes_with_no_ai_and_a_brief_answer() {
    let _turn = ONE_AT_A_TIME.lock().await;
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
    let (rig, worker_task, pump) = rig(spoken("Kivo, mute."), model.dir);
    rig.engine
        .talk(TurnSource::PushToTalk)
        .await
        .expect("KIVO starts listening");
    until("the answer", Duration::from_secs(30), || {
        rig.core.turn_view().and_then(|t| t.answer).is_some()
    })
    .await;
    assert_eq!(
        rig.core.turn_view().unwrap().answer.as_deref(),
        Some("Muted."),
        "a brief confirmation"
    );
    let metrics = rig.engine.router_metrics();
    assert_eq!(
        (metrics.requests, metrics.fast_path),
        (1, 1),
        "the grammar handled it: no AI"
    );
    assert!(
        rig.recorder
            .audit_rows(5)
            .iter()
            .any(|a| a.tool == "audio.mute" && a.decision == "allow")
    );
    rig.core.quit();
    let _ = tokio::time::timeout(Duration::from_secs(5), worker_task).await;
    pump.abort();
}

/// M1-X1: "Mute", "open Chrome" and "take a screenshot", spoken one after another, each act within
/// 500 ms of the end of speech, offline (local speech worker, grammar, no network).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_m1_commands_act_within_500_ms_of_the_end_of_speech() {
    const BUDGET_MS: u64 = 500;
    let _turn = ONE_AT_A_TIME.lock().await;
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
    let (rig, worker_task, pump) = rig(Vec::new(), model.dir);
    rig.core.update_config(|c| {
        c.capabilities.set(Capability::SpeakResponses, false);
    });
    // The budget is for KIVO with its speech model loaded, as it is from the moment a request
    // starts (VOICE-34). A cold disk right after a build can take seconds to load it.
    rig.infer.warm();
    assert!(
        rig.infer.wait_ready(Duration::from_secs(60)).await,
        "the speech worker loads"
    );
    for (said, tool) in [
        ("Mute.", "audio.mute"),
        ("Open Chrome.", "apps.launch"),
        ("Take a screenshot.", "screen.screenshot"),
    ] {
        rig.audio.say_next(spoken(said));
        rig.engine
            .talk(TurnSource::PushToTalk)
            .await
            .expect("KIVO starts listening");
        let turn = rig.core.turn_view().expect("a turn").id;
        until(said, Duration::from_secs(60), || {
            rig.db
                .lock()
                .unwrap()
                .turn(&turn)
                .unwrap()
                .and_then(|t| t.outcome)
                .is_some()
        })
        .await;
        until("KIVO to settle", Duration::from_secs(20), || {
            rig.core.state().borrow().session == SessionState::Idle
        })
        .await;
        rig.core.clear_turn();
        assert!(
            rig.recorder
                .audit_rows(10)
                .iter()
                .any(|a| a.tool == tool && a.decision == "allow"),
            "{said} ran {tool}"
        );
        let spans = rig
            .db
            .lock()
            .unwrap()
            .turn_metrics(&turn)
            .unwrap()
            .expect("timings");
        let at = |span: &str| spans.get(span).and_then(serde_json::Value::as_u64);
        let took = at("t8ToolDone").unwrap() - at("t4EndOfSpeech").unwrap();
        eprintln!("{said} end of speech → action: {took} ms");
        assert!(took <= BUDGET_MS, "{said} took {took} ms");
    }
    assert!(
        std::fs::read_dir(&rig.screenshots)
            .map(|d| d.count() == 1)
            .unwrap_or(false),
        "the screenshot was saved in the rig's folder"
    );
    assert_eq!(*rig.apps.launched.lock().unwrap(), ["Chrome"]);
    rig.core.quit();
    let _ = tokio::time::timeout(Duration::from_secs(5), worker_task).await;
    pump.abort();
}
