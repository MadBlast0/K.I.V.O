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
    eprintln!(
        "DEBUG heard: {:?}",
        rig.core.turn_view().unwrap().transcript
    );
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

/// SEC-10 / CONV-26: a decision waits for its answer. The question is asked, the card stays
/// (it used to end with the question), and a click approves it. The power controls are fakes.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_decision_waits_for_its_answer_and_a_click_approves_it() {
    let _turn = ONE_AT_A_TIME.lock().await;
    let (rig, worker_task, pump) = rig(Vec::new(), PathBuf::from("no-model"));
    rig.core.update_config(|c| {
        c.capabilities.set(Capability::SpeakResponses, false);
    });
    rig.engine.say("shut down").await.expect("accepted");
    until("the question", Duration::from_secs(10), || {
        rig.core.turn_view().and_then(|t| t.confirm).is_some()
    })
    .await;
    // The card is still waiting a while later: nothing ended the turn.
    tokio::time::sleep(Duration::from_secs(2)).await;
    let view = rig.core.turn_view().expect("the turn is still there");
    let confirm = view.confirm.expect("still asking");
    assert_eq!(
        rig.core.state().borrow().session,
        SessionState::AwaitingConfirmation
    );
    assert!(!view.answering, "a typed request waits for a click");
    assert!(rig.control.power.lock().unwrap().is_empty());
    rig.engine
        .answer_confirmation(&confirm.call_id, true, false)
        .await
        .expect("approved");
    until("the power action", Duration::from_secs(10), || {
        !rig.control.power.lock().unwrap().is_empty()
    })
    .await;
    rig.core.quit();
    let _ = tokio::time::timeout(Duration::from_secs(5), worker_task).await;
    pump.abort();
}

/// CONV-26/27/28: a spoken request that needs a decision is answered by voice. In Ask mode
/// "Open Chrome" needs a yes: KIVO asks, listens without the wake word, hears "yes, go ahead" and
/// approves (the owner, as recognition is off). A high-risk "shut down" answered "yes" is not
/// done: voice alone is never enough, and the card waits for a click. Apps and power are fakes.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn decisions_are_answered_by_voice_but_high_risk_needs_a_click() {
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
    rig.core
        .set_mode(kivo_core::config::PermissionMode::Ask)
        .expect("Ask mode");
    // The request, then the answer when KIVO next listens.
    rig.audio.say_next(spoken("Open Chrome."));
    rig.audio.say_next(spoken("Yes, go ahead."));
    rig.engine
        .talk(TurnSource::PushToTalk)
        .await
        .expect("KIVO starts listening");
    until("Chrome to open", Duration::from_secs(40), || {
        !rig.apps.launched.lock().unwrap().is_empty()
    })
    .await;
    assert!(
        rig.recorder
            .audit_rows(10)
            .iter()
            .any(|a| a.tool == "apps.launch" && a.confirmed_by.as_deref() == Some("voice")),
        "approved by voice"
    );
    until("the turn to end", Duration::from_secs(20), || {
        rig.core.state().borrow().session == SessionState::Idle
    })
    .await;
    rig.core.clear_turn();

    // High risk: a spoken yes is refused and the card stays for a click.
    rig.audio.say_next(spoken("Shut down the computer."));
    rig.audio.say_next(spoken("Yes."));
    rig.engine
        .talk(TurnSource::PushToTalk)
        .await
        .expect("KIVO starts listening");
    until("the question", Duration::from_secs(30), || {
        rig.core.turn_view().and_then(|t| t.confirm).is_some()
    })
    .await;
    until(
        "the spoken answer to be heard",
        Duration::from_secs(30),
        || {
            rig.audio.script.lock().unwrap().is_empty()
                && !rig.core.turn_view().is_some_and(|t| t.answering)
        },
    )
    .await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(
        rig.control.power.lock().unwrap().is_empty(),
        "not shut down"
    );
    assert!(
        rig.core.turn_view().and_then(|t| t.confirm).is_some(),
        "the card still waits"
    );
    rig.core.quit();
    let _ = tokio::time::timeout(Duration::from_secs(5), worker_task).await;
    pump.abort();
}

/// The keyword model: `KIVO_KWS_DIR` (the unpacked release archive) or the installed one.
fn keyword_model() -> Option<PathBuf> {
    std::env::var_os("KIVO_KWS_DIR")
        .map(PathBuf::from)
        .or_else(|| Paths::user().map(|p| p.models().join(kivo_store::models::KEYWORD_SPOTTER)))
        .filter(|d| d.join("encoder.onnx").is_file())
}

/// VOICE-05/13/14, UX-45: hands-free. Windows' voice says "Hey Kivo, mute." in one breath with no
/// key pressed; the keyword spotter wakes KIVO, recognition hears the whole sentence from just
/// before the wake word ended, the wake phrase is removed, and KIVO mutes. Then, within the
/// follow-up window, "Open Chrome." works without the wake word.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn hey_kivo_wakes_kivo_hands_free_and_a_follow_up_needs_no_wake_word() {
    let _turn = ONE_AT_A_TIME.lock().await;
    let paths = Paths::user().expect("per-user folders");
    let Some(model) = ModelStore::new(paths.models()).installed(kivo_voice::moonshine::MODEL_ID)
    else {
        eprintln!("the speech model isn't installed on this machine; skipping");
        return;
    };
    let Some(kws) = keyword_model() else {
        eprintln!("the keyword model isn't here (KIVO_KWS_DIR); skipping");
        return;
    };
    if !worker().is_file() {
        eprintln!("kivo-infer isn't built beside the tests; skipping");
        return;
    }
    let (rig, worker_task, pump) = rig(Vec::new(), model.dir);
    rig.core.update_config(|c| {
        c.voice.follow_up_seconds = 8;
    });
    // One continuous "microphone": the wake phrase and request, a pause for KIVO's answer, then
    // the follow-up.
    let mut mic = vec![0.0; 16_000];
    mic.extend(spoken("Hey Kivo, mute."));
    mic.extend(vec![0.0; 16_000 * 5]);
    mic.extend(spoken("Open Chrome."));
    mic.extend(vec![0.0; 16_000 * 3]);
    rig.audio.say_next(mic);
    rig.listener
        .set_hands_free(Some(kivo_runtime::voice::HandsFree {
            model_dir: kws,
            // The built-in word as KIVO sets it up, with its pronunciation variants.
            wake: rig
                .db
                .lock()
                .unwrap()
                .wake_words()
                .unwrap()
                .iter()
                .flat_map(kivo_runtime::wake::keywords_for)
                .collect(),
            stop: kivo_runtime::wake::stop_words(),
        }));
    let deadline = Instant::now() + Duration::from_secs(40);
    while !rig
        .recorder
        .audit_rows(10)
        .iter()
        .any(|a| a.tool == "audio.mute" && a.decision == "allow")
    {
        if Instant::now() > deadline {
            for a in rig.recorder.recent(None, 20) {
                eprintln!("activity: {} | {} | {}", a.kind, a.title, a.status);
            }
            panic!("timed out waiting for the sound to be muted");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let first = rig
        .recorder
        .recent(None, 20)
        .into_iter()
        .find(|a| a.kind == "transcript")
        .map(|a| a.title)
        .unwrap_or_default();
    assert!(
        !first.to_lowercase().contains("kivo"),
        "the wake phrase was removed: {first:?}"
    );
    until("Chrome to open", Duration::from_secs(40), || {
        !rig.apps.launched.lock().unwrap().is_empty()
    })
    .await;
    rig.listener.set_hands_free(None);
    rig.core.quit();
    let _ = tokio::time::timeout(Duration::from_secs(5), worker_task).await;
    pump.abort();
}

/// VOICE-31: barge-in. KIVO is busy talking (a long reply is playing) when the user says "Open
/// Chrome." over it, with no wake word. The listener ducks KIVO's voice, hears the whole request
/// from just before the user started, and KIVO acts on it: only barge-in can start this turn,
/// since no wake word is said and no follow-up window is open.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn talking_over_kivo_interrupts_it_and_is_heard_as_a_new_request() {
    let _turn = ONE_AT_A_TIME.lock().await;
    let paths = Paths::user().expect("per-user folders");
    let Some(model) = ModelStore::new(paths.models()).installed(kivo_voice::moonshine::MODEL_ID)
    else {
        eprintln!("the speech model isn't installed on this machine; skipping");
        return;
    };
    let Some(kws) = keyword_model() else {
        eprintln!("the keyword model isn't here (KIVO_KWS_DIR); skipping");
        return;
    };
    if !worker().is_file() {
        eprintln!("kivo-infer isn't built beside the tests; skipping");
        return;
    }
    let (rig, worker_task, pump) = rig(Vec::new(), model.dir);
    rig.core.update_config(|c| c.voice.follow_up_seconds = 0);
    let mut mic = vec![0.0; 16_000 * 2];
    mic.extend(spoken("Open Chrome."));
    mic.extend(vec![0.0; 16_000 * 3]);
    rig.audio.say_next(mic);
    // KIVO is in the middle of a long spoken reply (12 s of a soft tone at 24 kHz).
    #[allow(clippy::cast_precision_loss)]
    let reply: Vec<f32> = (0..24_000 * 12)
        .map(|i| (i as f32 / 24_000.0 * 180.0 * std::f32::consts::TAU).sin() * 0.05)
        .collect();
    rig.engine.speaker.speak(&reply, 24_000);
    rig.listener.set_busy(true);
    rig.listener
        .set_hands_free(Some(kivo_runtime::voice::HandsFree {
            model_dir: kws,
            wake: rig
                .db
                .lock()
                .unwrap()
                .wake_words()
                .unwrap()
                .iter()
                .flat_map(kivo_runtime::wake::keywords_for)
                .collect(),
            stop: kivo_runtime::wake::stop_words(),
        }));
    until(
        "Chrome to open after talking over KIVO",
        Duration::from_secs(40),
        || !rig.apps.launched.lock().unwrap().is_empty(),
    )
    .await;
    let heard = rig
        .recorder
        .recent(None, 20)
        .into_iter()
        .find(|a| a.kind == "transcript")
        .map(|a| a.title)
        .unwrap_or_default();
    assert!(
        heard.to_lowercase().contains("open"),
        "the whole request was heard, from before barge-in was detected: {heard:?}"
    );
    rig.listener.set_hands_free(None);
    rig.core.quit();
    let _ = tokio::time::timeout(Duration::from_secs(5), worker_task).await;
    pump.abort();
}

/// VOICE-47: the chosen voice can't load (its files are missing). KIVO still answers, with the
/// Windows voices for the rest of the session, and says so with a visible notice; the request
/// itself is unaffected.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_voice_that_fails_to_load_falls_back_to_the_windows_voices_with_a_notice() {
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
    let broken = tempfile::tempdir().expect("an empty folder");
    let (rig, worker_task, pump) = rig_with_voice(
        spoken("Kivo, mute."),
        model.dir,
        ("kokoro-82m".into(), Some(broken.path().to_path_buf())),
    );
    let mut events = rig.core.bus.subscribe();
    rig.engine
        .talk(TurnSource::PushToTalk)
        .await
        .expect("KIVO starts listening");
    let notice = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let kivo_core::Received::Event(event) = events.recv().await
                && let kivo_core::EventKind::System(kivo_core::event::SystemEvent::SpeechFallback {
                    slot,
                    to,
                    message,
                    ..
                }) = &event.kind
            {
                return (slot.clone(), to.clone(), message.clone());
            }
        }
    })
    .await
    .expect("a fallback notice");
    assert_eq!(notice.0, "tts");
    assert_eq!(notice.1.as_deref(), Some("system"));
    assert!(notice.2.contains("Windows voices"), "{}", notice.2);
    until("the answer", Duration::from_secs(30), || {
        rig.core.turn_view().and_then(|t| t.answer).is_some()
    })
    .await;
    assert!(
        rig.recorder
            .audit_rows(5)
            .iter()
            .any(|a| a.tool == "audio.mute" && a.decision == "allow"),
        "the request still ran"
    );
    rig.core.quit();
    let _ = tokio::time::timeout(Duration::from_secs(5), worker_task).await;
    pump.abort();
}

/// VOICE-45: choosing a recognizer tests it in a separate worker before it becomes the choice —
/// a Windows voice says a sentence and the new engine must hear it — and only then is the setting
/// saved. Choosing one that doesn't fit the language is refused with the engines that do.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_new_speech_engine_is_tested_before_it_is_used() {
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
    let models = std::sync::Arc::new(kivo_runtime::models::Models::new(
        paths.models(),
        std::sync::Arc::clone(&rig.core),
        rig.infer.clone(),
    ));
    let switcher = std::sync::Arc::new(
        kivo_runtime::switch::Switcher::new(
            std::sync::Arc::clone(&rig.core),
            std::sync::Arc::clone(&rig.engine),
            models,
        )
        .with_test_voice(std::sync::Arc::new(kivo_platform_windows::WindowsSpeech)),
    );
    let mut events = rig.core.bus.subscribe();
    assert!(rig.core.config().voice.stt_engine.is_empty());
    switcher
        .start(
            kivo_ipc::infer::InferSlot::Stt,
            kivo_voice::moonshine::MODEL_ID,
            None,
        )
        .expect("it fits");
    let mut stages = Vec::new();
    let outcome =
        tokio::time::timeout(Duration::from_secs(60), async {
            loop {
                if let kivo_core::Received::Event(event) = events.recv().await
                    && let kivo_core::EventKind::System(
                        kivo_core::event::SystemEvent::EngineSwitch { stage, message, .. },
                    ) = &event.kind
                {
                    stages.push(stage.clone());
                    if stage == "ready" || stage == "failed" {
                        return message.clone();
                    }
                    if stage == "testing" {
                        assert!(
                            rig.core.config().voice.stt_engine.is_empty(),
                            "the choice waits for the test"
                        );
                    }
                }
            }
        })
        .await
        .expect("the switch finished");
    assert_eq!(outcome, None, "it passed: {stages:?}");
    assert_eq!(stages, ["checking", "loading", "testing", "ready"]);
    assert_eq!(
        rig.core.config().voice.stt_engine,
        kivo_voice::moonshine::MODEL_ID
    );

    rig.core
        .update_config(|c| c.general.language = "es-ES".into());
    let refused = switcher
        .start(
            kivo_ipc::infer::InferSlot::Stt,
            kivo_voice::moonshine::MODEL_ID,
            None,
        )
        .unwrap_err();
    assert!(refused.contains("Moonshine Base (Spanish)"), "{refused}");
    rig.core.quit();
    let _ = tokio::time::timeout(Duration::from_secs(5), worker_task).await;
    pump.abort();
}

/// VOICE-19, SEC-26: while KIVO is busy, "Kivo, stop" stops it without the wake word. KIVO is in
/// the middle of a 12-second reply; the user says it over the reply and KIVO falls silent long
/// before the reply would have ended.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn saying_stop_while_kivo_talks_stops_it() {
    let _turn = ONE_AT_A_TIME.lock().await;
    let paths = Paths::user().expect("per-user folders");
    let Some(model) = ModelStore::new(paths.models()).installed(kivo_voice::moonshine::MODEL_ID)
    else {
        eprintln!("the speech model isn't installed on this machine; skipping");
        return;
    };
    let Some(kws) = keyword_model() else {
        eprintln!("the keyword model isn't here (KIVO_KWS_DIR); skipping");
        return;
    };
    if !worker().is_file() {
        eprintln!("kivo-infer isn't built beside the tests; skipping");
        return;
    }
    let (rig, worker_task, pump) = rig(Vec::new(), model.dir);
    let mut mic = vec![0.0; 16_000 * 2];
    mic.extend(spoken("Kivo, stop."));
    mic.extend(vec![0.0; 16_000 * 8]);
    rig.audio.say_next(mic);
    #[allow(clippy::cast_precision_loss)]
    let reply: Vec<f32> = (0..24_000 * 12)
        .map(|i| (i as f32 / 24_000.0 * 180.0 * std::f32::consts::TAU).sin() * 0.05)
        .collect();
    let started = Instant::now();
    rig.engine.speaker.speak(&reply, 24_000);
    rig.listener.set_busy(true);
    rig.listener
        .set_hands_free(Some(kivo_runtime::voice::HandsFree {
            model_dir: kws,
            wake: rig
                .db
                .lock()
                .unwrap()
                .wake_words()
                .unwrap()
                .iter()
                .flat_map(kivo_runtime::wake::keywords_for)
                .collect(),
            stop: kivo_runtime::wake::stop_words(),
        }));
    until("KIVO to fall silent", Duration::from_secs(9), || {
        !rig.engine.speaker.speaking()
    })
    .await;
    assert!(
        started.elapsed() < Duration::from_secs(9),
        "stopped by the user, not by the reply ending"
    );
    rig.listener.set_hands_free(None);
    rig.core.quit();
    let _ = tokio::time::timeout(Duration::from_secs(5), worker_task).await;
    pump.abort();
}

/// A stand-in voice model for the guest test: a voice is the pitch of its zero crossings, so the
/// owner's enrollment (a 180 Hz voice) and Windows' voice are clearly different people.
struct PitchVoice;

impl kivo_voice::SpeakerVerifier for PitchVoice {
    fn info(&self) -> &kivo_voice::EngineInfo {
        static INFO: std::sync::OnceLock<kivo_voice::EngineInfo> = std::sync::OnceLock::new();
        INFO.get_or_init(kivo_voice::speaker::info)
    }
    fn embed(&self, audio: &[f32]) -> kivo_voice::VoiceResult<Vec<f32>> {
        let crossings = audio
            .windows(2)
            .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
            .count();
        #[allow(clippy::cast_precision_loss)]
        let angle = crossings as f32 / audio.len().max(1) as f32 * 40.0;
        Ok(vec![angle.cos(), angle.sin(), 0.2])
    }
    fn score(&self, embedding: &Vec<f32>, profile: &[Vec<f32>]) -> f32 {
        kivo_voice::speaker::similarity(embedding, &kivo_voice::speaker::centroid(profile))
    }
}

/// M2-X3, CONV-28, VOICE-22: with the owner enrolled and "Prefer owner" on, a request in another
/// voice becomes a guest turn, and the guest's spoken "yes" does not approve the action.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_guest_cannot_approve_by_voice() {
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
    let voice = tempfile::tempdir().expect("a voice folder");
    let voice_id = std::sync::Arc::new(kivo_runtime::voiceid::VoiceId::new(
        voice.path().to_path_buf(),
        std::sync::Arc::new(kivo_testkit::FakeSecrets::default()),
        std::sync::Arc::clone(&rig.db),
        || Some(std::sync::Arc::new(PitchVoice) as std::sync::Arc<dyn kivo_voice::SpeakerVerifier>),
    ));
    // The owner enrolls in a 180 Hz voice.
    #[allow(clippy::cast_precision_loss)]
    let owner = |seconds: f32| -> Vec<f32> {
        (0..(16_000.0 * seconds) as usize)
            .map(|i| (i as f32 / 16_000.0 * 180.0 * std::f32::consts::TAU).sin() * 0.3)
            .collect()
    };
    for prompt in 0..8 {
        assert!(voice_id.keep_take(prompt, owner(2.5)).ok);
    }
    assert!(voice_id.finish_enrollment().expect("enrolled").enrolled);
    rig.engine.set_voice_id(std::sync::Arc::clone(&voice_id));
    rig.listener.set_keep_audio(true);
    rig.core.update_config(|c| {
        c.capabilities.set(Capability::SpeakResponses, false);
        c.capabilities.set(Capability::SpeakerRecognition, true);
        c.voice.speaker_mode = kivo_core::config::SpeakerMode::PreferOwner;
    });
    rig.core
        .set_mode(kivo_core::config::PermissionMode::Ask)
        .expect("Ask mode");
    // Windows' voice is not the owner's: a guest asks, and says yes.
    rig.audio.say_next(spoken("Open Chrome."));
    rig.audio.say_next(spoken("Yes, go ahead."));
    rig.engine
        .talk(TurnSource::PushToTalk)
        .await
        .expect("KIVO starts listening");
    until(
        "a guest turn asking for OK",
        Duration::from_secs(40),
        || {
            rig.core
                .turn_view()
                .is_some_and(|t| t.guest && t.confirm.is_some())
        },
    )
    .await;
    until(
        "the guest's yes to be refused",
        Duration::from_secs(30),
        || {
            rig.recorder
                .recent(None, 20)
                .iter()
                .any(|a| a.kind == "reply" && a.status == "refused" && a.title.contains("owner"))
        },
    )
    .await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(
        rig.apps.launched.lock().unwrap().is_empty(),
        "a guest's yes approves nothing"
    );
    assert!(
        rig.core.turn_view().is_some_and(|t| t.confirm.is_some()),
        "the card still waits for the owner"
    );
    rig.core.quit();
    let _ = tokio::time::timeout(Duration::from_secs(5), worker_task).await;
    pump.abort();
}

/// VOICE-30/31 with a speaker's echo (M2-X2, simulated): KIVO's reply is real speech, and the
/// fake device sits in a "room" where the microphone hears the speaker 40 ms later at a third of
/// its level, until KIVO stops. The user talks over the reply: the echo path is found, KIVO's
/// own voice doesn't interrupt it, and the user's request gets through.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn talking_over_kivo_works_with_speakers_echoing_its_voice() {
    let _turn = ONE_AT_A_TIME.lock().await;
    let paths = Paths::user().expect("per-user folders");
    let Some(model) = ModelStore::new(paths.models()).installed(kivo_voice::moonshine::MODEL_ID)
    else {
        eprintln!("the speech model isn't installed on this machine; skipping");
        return;
    };
    let Some(kws) = keyword_model() else {
        eprintln!("the keyword model isn't here (KIVO_KWS_DIR); skipping");
        return;
    };
    if !worker().is_file() {
        eprintln!("kivo-infer isn't built beside the tests; skipping");
        return;
    }
    let (rig, worker_task, pump) = rig(Vec::new(), model.dir);
    rig.core.update_config(|c| c.voice.follow_up_seconds = 0);
    rig.audio.set_room(40, 0.3);
    let reply = spoken(
        "Here is the forecast for the rest of the week. Tomorrow will be sunny with a light \
         breeze, and the weekend brings some rain in the afternoon, so take an umbrella.",
    );
    // The user says nothing for 3 s, then talks over the reply.
    let mut mic = vec![0.0; 16_000 * 3];
    // A short, distinct command: "Chrome" was sometimes heard as "crew" under the echo residue.
    mic.extend(spoken("Mute the sound."));
    mic.extend(vec![0.0; 16_000 * 6]);
    rig.audio.say_next(mic);
    rig.listener.set_busy(true);
    rig.listener
        .set_hands_free(Some(kivo_runtime::voice::HandsFree {
            model_dir: kws,
            wake: rig
                .db
                .lock()
                .unwrap()
                .wake_words()
                .unwrap()
                .iter()
                .flat_map(kivo_runtime::wake::keywords_for)
                .collect(),
            stop: kivo_runtime::wake::stop_words(),
        }));
    until("the microphone to open", Duration::from_secs(10), || {
        rig.listener.hands_free_on()
    })
    .await;
    rig.engine.speaker.speak(&reply, 16_000);
    let deadline = Instant::now() + Duration::from_secs(40);
    while !rig.control.volume.lock().unwrap().muted {
        assert!(
            Instant::now() < deadline,
            "KIVO never muted over its own echo; Activity: {:?}",
            rig.recorder
                .recent(None, 10)
                .into_iter()
                .map(|a| (a.kind, a.title))
                .collect::<Vec<_>>()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let heard = rig
        .recorder
        .recent(None, 20)
        .into_iter()
        .find(|a| a.kind == "transcript")
        .map(|a| a.title)
        .unwrap_or_default();
    assert!(
        !heard.to_lowercase().contains("umbrella"),
        "KIVO's own voice wasn't taken for the user's: {heard:?}"
    );
    rig.listener.set_hands_free(None);
    rig.core.quit();
    let _ = tokio::time::timeout(Duration::from_secs(5), worker_task).await;
    pump.abort();
}

/// VOICE-23: the owner's enrollment recordings (here, Windows' voice reading the prompts) are
/// transcribed by every installed recognizer in separate workers, and each gets a word error
/// rate on this voice, which the recommendation uses.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_installed_recognizer_is_scored_on_the_owners_voice() {
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
    let models = std::sync::Arc::new(kivo_runtime::models::Models::new(
        paths.models(),
        std::sync::Arc::clone(&rig.core),
        rig.infer.clone(),
    ));
    let switcher = kivo_runtime::switch::Switcher::new(
        std::sync::Arc::clone(&rig.core),
        std::sync::Arc::clone(&rig.engine),
        std::sync::Arc::clone(&models),
    );
    let takes: Vec<(String, Vec<f32>)> = kivo_runtime::voiceid::PROMPTS
        .iter()
        .skip(3)
        .map(|p| ((*p).to_owned(), spoken(p)))
        .collect();
    let wers = switcher.voice_wer(&takes).await;
    eprintln!("enrollment WER: {wers:?}");
    let base = wers
        .get(kivo_voice::moonshine::MODEL_ID)
        .copied()
        .expect("Moonshine Base was scored");
    assert!(base < 0.35, "Windows' clear voice is mostly heard: {base}");
    models.set_voice_wers(wers.clone(), 1);
    let registry = models.registry();
    let entry = registry
        .iter()
        .find(|e| e.engine.id == kivo_voice::moonshine::MODEL_ID)
        .unwrap();
    assert_eq!(
        entry
            .measured
            .as_ref()
            .and_then(|m| m.voice_word_error_rate),
        Some(base)
    );
    rig.core.quit();
    let _ = tokio::time::timeout(Duration::from_secs(5), worker_task).await;
    pump.abort();
}

/// CONV-27 "edit": a spoken change to a waiting decision drops the action and hands the
/// corrected request to a brain. In Ask mode "Open Chrome." waits for a yes; "Change it to
/// Firefox." instead never opens Chrome, and the brain is asked with the change.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn changing_a_decision_by_voice_goes_to_a_brain() {
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
    rig.core
        .set_mode(kivo_core::config::PermissionMode::Ask)
        .expect("Ask mode");
    let brain = std::sync::Arc::new(kivo_brain::testing::ScriptedBrain::new(
        "anthropic",
        kivo_brain::PrivacyClass::Cloud,
        vec![kivo_brain::testing::Script::text("Okay, Firefox instead.")],
    ));
    rig.brains.insert(brain.clone());
    rig.audio.say_next(spoken("Open Chrome."));
    rig.audio.say_next(spoken("Change it to Firefox."));
    rig.engine
        .talk(TurnSource::PushToTalk)
        .await
        .expect("KIVO starts listening");
    until("the brain's answer", Duration::from_secs(45), || {
        rig.core.turn_view().and_then(|t| t.answer).as_deref() == Some("Okay, Firefox instead.")
    })
    .await;
    assert!(
        rig.apps.launched.lock().unwrap().is_empty(),
        "Chrome wasn't opened"
    );
    let asked = brain.requests.lock().unwrap()[0]
        .messages
        .last()
        .unwrap()
        .text()
        .to_lowercase();
    assert!(
        asked.contains("chrome") && asked.contains("firefox"),
        "{asked}"
    );
    rig.core.quit();
    let _ = tokio::time::timeout(Duration::from_secs(5), worker_task).await;
    pump.abort();
}
