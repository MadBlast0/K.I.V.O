//! M7 acceptance journeys (plan §71, §139): KIVO keeps working with the network off, and a
//! confidential document is summarized by a brain on this PC while the cloud is blocked.
//!
//! The voice half of the offline journey drives the real speech models (the keyword spotter and
//! Moonshine); where they aren't installed it says so and runs the rest.

#![cfg(windows)]

use kivo_brain::testing::{Script, ScriptedBrain};
use kivo_brain::{NormalizedError, Part, PrivacyClass, Role};
use kivo_core::routine::{Routine, RoutineStep, Trigger};
use kivo_core::task::{OnError, StepAction};
use kivo_core::{Capability, SessionState};
use kivo_platform::Paths;
use kivo_runtime::scripted::{self, Rig, spoken};
use kivo_store::models::ModelStore;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn worker() -> PathBuf {
    let mut dir = std::env::current_exe().expect("test binary");
    dir.pop();
    dir.pop();
    dir.join("kivo-infer.exe")
}

static ONE_AT_A_TIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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

fn brain(id: &str, name: &str, privacy: PrivacyClass, script: Vec<Script>) -> Arc<ScriptedBrain> {
    let mut b = ScriptedBrain::new(id, privacy, script);
    b.info.name = name.into();
    Arc::new(b)
}

async fn idle(rig: &Rig) {
    until("the turn to end", Duration::from_secs(30), || {
        rig.core.state().borrow().session == SessionState::Idle
            && rig
                .core
                .turn_view()
                .is_some_and(|t| t.answer.is_some() || t.error.is_some())
    })
    .await;
}

/// What the brain was shown as tool results in its `n`th request.
fn tool_results(b: &ScriptedBrain, n: usize) -> String {
    b.requests.lock().unwrap()[n]
        .messages
        .iter()
        .filter(|m| m.role == Role::Tool)
        .flat_map(|m| &m.parts)
        .filter_map(|p| match p {
            Part::ToolResult { content, .. } => Some(content.clone()),
            _ => None,
        })
        .collect()
}

/// PLAN-21 (plan §139): "Summarize this confidential document locally." KIVO sees the privacy
/// requirement, blocks the cloud brain, picks the local one, reads the document on this PC and
/// answers with the summary.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_confidential_document_is_summarized_on_this_pc() {
    let _one = ONE_AT_A_TIME.lock().await;
    let (rig, worker_task, pump) = scripted::rig(worker(), Vec::new(), PathBuf::from("no-model"));
    rig.core.update_config(|c| {
        c.capabilities.set(Capability::SpeakResponses, false);
        c.voice.speak_typed_replies = false;
    });
    let document = rig.screenshots.join("Board minutes.txt");
    std::fs::write(
        &document,
        "Board minutes. The merger with Northwind closes in May. Headcount stays flat.",
    )
    .unwrap();
    let cloud = brain("anthropic", "Anthropic", PrivacyClass::Cloud, vec![]);
    let local = brain(
        "ollama",
        "Ollama",
        PrivacyClass::Local,
        vec![
            Script::tool(
                "files__read",
                json!({ "path": document.display().to_string() }),
            ),
            Script::text("The merger with Northwind closes in May, and headcount stays flat."),
        ],
    );
    rig.brains.insert(cloud.clone());
    rig.brains.insert(local.clone());
    rig.engine
        .say(&format!(
            "Summarize this confidential document locally: {}",
            document.display()
        ))
        .await
        .unwrap();
    idle(&rig).await;
    let view = rig.core.turn_view().unwrap();
    assert_eq!(
        view.answer.as_deref(),
        Some("The merger with Northwind closes in May, and headcount stays flat."),
        "{view:?}"
    );
    let chip = view.brain.expect("the brain chip");
    assert_eq!(chip.name, "Ollama");
    assert!(
        cloud.requests.lock().unwrap().is_empty(),
        "the cloud never saw it"
    );
    assert_eq!(local.requests.lock().unwrap().len(), 2);
    assert!(
        tool_results(&local, 1).contains("Northwind"),
        "read on this PC"
    );
    rig.core.quit();
    let _ = tokio::time::timeout(Duration::from_secs(5), worker_task).await;
    pump.abort();
}

fn keyword_model() -> Option<PathBuf> {
    std::env::var_os("KIVO_KWS_DIR")
        .map(PathBuf::from)
        .or_else(|| Paths::user().map(|p| p.models().join(kivo_store::models::KEYWORD_SPOTTER)))
        .filter(|d| d.join("encoder.onnx").is_file())
}

/// PLAN-04 (plan §71): with the network off, KIVO still wakes, hears and speaks, runs native
/// commands, files and UI tools, a local brain and a routine. The cloud brain's checks fail on the
/// network, so KIVO knows it's offline and routes to the brain on this PC.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn everything_local_works_with_the_network_off() {
    let _one = ONE_AT_A_TIME.lock().await;
    let paths = Paths::user().expect("per-user folders");
    let speech = ModelStore::new(paths.models()).installed(kivo_voice::moonshine::MODEL_ID);
    let model_dir = speech
        .as_ref()
        .map_or_else(|| PathBuf::from("no-model"), |m| m.dir.clone());
    let (rig, worker_task, pump) = scripted::rig(worker(), Vec::new(), model_dir);
    rig.core.update_config(|c| {
        c.capabilities.set(Capability::SpeakResponses, true);
        c.voice.follow_up_seconds = 0;
    });
    let notes = rig.screenshots.join("Shopping list.txt");
    std::fs::write(&notes, "Milk, eggs, coffee.").unwrap();
    let cloud = brain("anthropic", "Anthropic", PrivacyClass::Cloud, vec![]);
    let local = brain(
        "ollama",
        "Ollama",
        PrivacyClass::Local,
        vec![
            Script::tool(
                "files__read",
                json!({ "path": notes.display().to_string() }),
            ),
            Script::text("Milk, eggs and coffee."),
        ],
    );
    rig.brains.insert(cloud.clone());
    rig.brains.insert(local.clone());
    // The network is off: the cloud brain's last check failed on it.
    rig.brains.failed(
        "anthropic",
        &NormalizedError::Network("no route to host".into()),
    );
    assert!(rig.brains.offline());

    // Wake, speech in and speech out, and a native command, with the real models when present.
    match (speech.is_some() && worker().is_file(), keyword_model()) {
        (true, Some(kws)) => {
            let mut mic = vec![0.0; 16_000];
            mic.extend(spoken("Hey Kivo, mute."));
            mic.extend(vec![0.0; 16_000 * 4]);
            rig.audio.say_next(mic);
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
            until("muted by voice", Duration::from_secs(40), || {
                rig.control.volume.lock().unwrap().muted
            })
            .await;
            until("the spoken answer", Duration::from_secs(20), || {
                !rig.audio.played.lock().unwrap().is_empty()
            })
            .await;
            rig.listener.set_hands_free(None);
        }
        _ => {
            eprintln!("the speech models aren't installed here; the voice half is skipped");
            rig.engine.say("mute").await.unwrap();
            idle(&rig).await;
            assert!(rig.control.volume.lock().unwrap().muted);
        }
    }
    until("idle", Duration::from_secs(20), || {
        rig.core.state().borrow().session == SessionState::Idle
    })
    .await;
    rig.core.update_config(|c| {
        c.capabilities.set(Capability::SpeakResponses, false);
        c.voice.speak_typed_replies = false;
    });

    // An app, by command: no AI.
    rig.engine.say("open Chrome").await.unwrap();
    idle(&rig).await;
    assert_eq!(*rig.apps.launched.lock().unwrap(), ["Chrome"]);

    // A question: the brain on this PC answers, reading the file, "because this PC is offline".
    rig.engine.say("read my shopping list file").await.unwrap();
    idle(&rig).await;
    let view = rig.core.turn_view().unwrap();
    assert_eq!(
        view.answer.as_deref(),
        Some("Milk, eggs and coffee."),
        "{view:?}"
    );
    let chip = view.brain.expect("the brain chip");
    assert_eq!(chip.name, "Ollama");
    assert!(chip.reason.contains("offline"), "{}", chip.reason);
    let read = tool_results(&local, 1);
    assert!(read.contains("Milk"), "{read}");
    assert!(cloud.requests.lock().unwrap().is_empty());

    // A routine (local automation): its phrase sets the volume.
    rig.routines
        .save(
            Routine {
                id: String::new(),
                name: "Quiet".into(),
                description: String::new(),
                enabled: true,
                triggers: vec![Trigger::Phrase {
                    phrases: vec!["quiet time".into()],
                    lang: "en".into(),
                }],
                steps: vec![RoutineStep {
                    id: "v".into(),
                    action: StepAction::Tool {
                        tool: "audio.volume_set".into(),
                        args: json!({ "number": 15 }),
                    },
                    on_error: OnError::Stop,
                    delay_ms: None,
                    parallel_group: None,
                    confirm: false,
                }],
                variables: Vec::new(),
                grants: Vec::new(),
                starter: None,
            },
            true,
        )
        .unwrap();
    rig.core.clear_turn();
    rig.engine.say("quiet time").await.unwrap();
    until("the routine to run", Duration::from_secs(10), || {
        (rig.control.volume.lock().unwrap().level - 0.15).abs() < 0.01
    })
    .await;
    rig.core.quit();
    let _ = tokio::time::timeout(Duration::from_secs(5), worker_task).await;
    pump.abort();
}
