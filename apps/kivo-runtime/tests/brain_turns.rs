//! M3, end to end: requests the grammar can't handle go to a brain (BRAINS §1). The brains here
//! are scripted (`kivo_brain::testing::ScriptedBrain`) or a small ACP agent run by Node, so the
//! whole runtime path is real — routing with its reason, context assembly, the streamed answer,
//! tool calls through the permission engine, failover, cancellation, usage and cost, the
//! conversation thread — while no request leaves this PC. Apps and system controls are fakes.

#![cfg(windows)]

use kivo_brain::testing::{Script, ScriptedBrain};
use kivo_brain::{BrainEvent, NormalizedError, Part, PrivacyClass, Role, StopReason, Usage};
use kivo_core::config::BrainConnection;
use kivo_core::{Capability, SessionState};
use kivo_runtime::scripted::{self, Rig};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn worker() -> PathBuf {
    let mut dir = std::env::current_exe().expect("test binary");
    dir.pop();
    dir.pop();
    dir.join("kivo-infer.exe")
}

/// One journey at a time: they share the speech worker's CPU.
static ONE_AT_A_TIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn until(what: &str, timeout: Duration, mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

struct Running {
    rig: Rig,
    worker: tokio::task::JoinHandle<()>,
    pump: tokio::task::JoinHandle<()>,
}

impl Running {
    async fn stop(self) {
        self.rig.core.quit();
        let _ = tokio::time::timeout(Duration::from_secs(5), self.worker).await;
        self.pump.abort();
    }
}

/// A rig whose replies are shown, not spoken (typed requests), unless `speak`.
fn start(speak: bool) -> Running {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(std::env::var("KIVO_TEST_LOG").unwrap_or_else(|_| "warn".into()))
        .with_test_writer()
        .try_init();
    let (rig, worker, pump) = scripted::rig(worker(), Vec::new(), PathBuf::from("no-model"));
    rig.core.update_config(|c| {
        c.capabilities.set(Capability::SpeakResponses, speak);
        c.voice.speak_typed_replies = speak;
    });
    Running { rig, worker, pump }
}

fn brain(id: &str, name: &str, privacy: PrivacyClass, script: Vec<Script>) -> Arc<ScriptedBrain> {
    let mut b = ScriptedBrain::new(id, privacy, script);
    b.info.name = name.into();
    Arc::new(b)
}

async fn answered(rig: &Rig) -> kivo_ipc::protocol::TurnView {
    until("the turn to end", Duration::from_secs(20), || {
        rig.core.state().borrow().session == SessionState::Idle
            && rig
                .core
                .turn_view()
                .is_some_and(|t| t.answer.is_some() || t.error.is_some())
    })
    .await;
    rig.core.turn_view().expect("the card still shows the turn")
}

/// PLAN-17, BRAIN-21, BRAIN-34/35, CONV-01, CONV-05: an open question is answered by the default
/// brain; the card shows which brain and why; usage is metered with a cost estimate; the thread
/// keeps both messages; the request is assembled with the persona, guardrails and live context.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_open_question_is_answered_by_the_default_brain_with_a_reason() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start(false);
    let claude = brain(
        "anthropic",
        "Anthropic",
        PrivacyClass::Cloud,
        vec![Script::text(
            "Paris is the capital of France. It is on the Seine.",
        )],
    );
    r.rig.brains.insert(claude.clone());
    r.rig.core.update_config(|c| c.brains.show_cost = true);

    r.rig
        .engine
        .say("what's the capital of France")
        .await
        .unwrap();
    let view = answered(&r.rig).await;
    assert_eq!(
        view.answer.as_deref(),
        Some("Paris is the capital of France. It is on the Seine.")
    );
    let chip = view.brain.expect("the card shows the brain");
    assert_eq!(chip.name, "Anthropic");
    assert_eq!(chip.profile, "Default");
    assert!(
        chip.reason.contains("your default brain"),
        "{}",
        chip.reason
    );
    assert!(!chip.local);
    assert!(
        chip.cost.is_some_and(|c| c > 0.0),
        "an estimate from the bundled prices: {:?}",
        chip.cost
    );
    assert!(chip.context_used > 0 && chip.context_budget > 0);

    // What the brain was sent: the guardrails, live context and the request, and tools.
    let request = claude.requests.lock().unwrap()[0].clone();
    let system = request.system_text();
    assert!(system.contains("neutral and factual"), "{system}");
    assert!(system.contains("local time"), "{system}");
    assert!(
        request.system[0].cacheable,
        "the stable prefix is cacheable"
    );
    assert_eq!(
        request
            .messages
            .last()
            .map(kivo_brain::Message::text)
            .as_deref(),
        Some("what's the capital of France")
    );
    assert!(request.tools.len() <= 20);
    assert!(
        request.tools.iter().all(|t| !t.name.contains('.')),
        "wire names"
    );

    // Activity, usage and the thread.
    let items = r.rig.recorder.recent(None, 20);
    assert!(
        items
            .iter()
            .any(|i| i.kind == "brain" && i.title.contains("Default · Anthropic")),
        "{items:?}"
    );
    {
        let db = r.rig.db.lock().unwrap();
        let usage = db.usage_since(0).unwrap();
        assert_eq!(usage.len(), 1);
        assert_eq!(usage[0].provider, "anthropic");
        assert!(usage[0].turn_id.is_some());
        let threads = db.conversations(10).unwrap();
        assert_eq!(threads.len(), 1);
        let messages = db.messages(&threads[0].id).unwrap();
        assert_eq!(
            messages.iter().map(|m| m.role.as_str()).collect::<Vec<_>>(),
            ["user", "assistant"]
        );
    }
    r.stop().await;
}

/// Invariant 4, BRAIN-27: the brain's tool calls go through the permission engine. Low-risk
/// "mute" and "open Chrome" run; closing an app is medium risk, and when an AI asks for it KIVO
/// asks the user first (SECURITY §2) and runs it after the click; the results go back to the
/// brain, which then answers.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_brains_tool_calls_go_through_the_permission_engine() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start(false);
    let b = brain(
        "openai",
        "OpenAI",
        PrivacyClass::Cloud,
        vec![
            Script::tool("audio__mute", serde_json::json!({})),
            Script::tool(
                "apps__launch",
                serde_json::json!({"app": {"id": "Chrome", "name": "Google Chrome"}}),
            ),
            Script::tool(
                "apps__close",
                serde_json::json!({"app": {"id": "Chrome", "name": "Google Chrome"}}),
            ),
            Script::text("I muted the sound, and opened and closed Chrome."),
        ],
    );
    r.rig.brains.insert(b.clone());
    r.rig
        .engine
        .say("I need quiet, then test that my browser opens and closes")
        .await
        .unwrap();
    until("the close question", Duration::from_secs(10), || {
        r.rig.core.turn_view().and_then(|t| t.confirm).is_some()
    })
    .await;
    assert!(r.rig.control.volume.lock().unwrap().muted, "mute ran");
    assert_eq!(*r.rig.apps.launched.lock().unwrap(), ["Chrome"]);
    assert!(r.rig.apps.closed.lock().unwrap().is_empty(), "not yet");
    assert_eq!(
        r.rig.core.state().borrow().session,
        SessionState::AwaitingConfirmation
    );
    let confirm = r.rig.core.turn_view().unwrap().confirm.unwrap();
    assert_eq!(confirm.tool, "apps.close");
    r.rig
        .engine
        .answer_confirmation(&confirm.call_id, true, false)
        .await
        .unwrap();
    let view = answered(&r.rig).await;
    assert_eq!(*r.rig.apps.closed.lock().unwrap(), ["Chrome"]);
    assert_eq!(
        view.answer.as_deref(),
        Some("I muted the sound, and opened and closed Chrome.")
    );
    // The brain saw every result.
    let requests = b.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 4);
    let results: Vec<&Part> = requests[3]
        .messages
        .iter()
        .filter(|m| m.role == Role::Tool)
        .flat_map(|m| &m.parts)
        .collect();
    assert_eq!(results.len(), 3, "{results:?}");
    assert!(results.iter().all(|p| matches!(
        p,
        Part::ToolResult {
            is_error: false,
            ..
        }
    )));
    // Audited like any other action.
    let audit = r.rig.recorder.audit_rows(20);
    assert!(
        audit
            .iter()
            .any(|a| a.tool == "apps.close" && a.decision == "confirm")
    );
    assert!(
        audit
            .iter()
            .any(|a| a.tool == "apps.launch" && a.decision == "allow")
    );
    assert!(
        audit
            .iter()
            .any(|a| a.tool == "audio.mute" && a.decision == "allow")
    );
    r.stop().await;
}

/// A declined action ends the turn; the brain is not asked again.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn declining_the_brains_action_ends_the_turn() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start(false);
    let b = brain(
        "openai",
        "OpenAI",
        PrivacyClass::Cloud,
        vec![Script::tool("system__shutdown", serde_json::json!({}))],
    );
    r.rig.brains.insert(b.clone());
    r.rig
        .engine
        .say("I'm done for the day, shut down the PC")
        .await
        .unwrap();
    until("the question", Duration::from_secs(10), || {
        r.rig.core.turn_view().and_then(|t| t.confirm).is_some()
    })
    .await;
    let confirm = r.rig.core.turn_view().unwrap().confirm.unwrap();
    assert_eq!(confirm.strength, kivo_core::tool::Strength::Strong);
    r.rig
        .engine
        .answer_confirmation(&confirm.call_id, false, false)
        .await
        .unwrap();
    until("idle", Duration::from_secs(5), || {
        r.rig.core.state().borrow().session == SessionState::Idle
    })
    .await;
    assert!(r.rig.control.power.lock().unwrap().is_empty());
    assert_eq!(b.requests.lock().unwrap().len(), 1);
    r.stop().await;
}

/// BRAIN-22: a rate-limited brain fails over to the next one in the same privacy class, and the
/// card says so. A local brain is never used as the fallback for a cloud one.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_busy_brain_fails_over_within_its_privacy_class() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start(false);
    r.rig.brains.insert(brain(
        "anthropic",
        "Anthropic",
        PrivacyClass::Cloud,
        vec![Script::error(NormalizedError::RateLimited {
            retry_after: None,
        })],
    ));
    r.rig.brains.insert(brain(
        "openai",
        "OpenAI",
        PrivacyClass::Cloud,
        vec![Script::text("Here you go.")],
    ));
    let local = brain(
        "ollama",
        "Ollama",
        PrivacyClass::Local,
        vec![Script::text("from the local model")],
    );
    r.rig.brains.insert(local.clone());
    r.rig
        .engine
        .say("give me a haiku about rain")
        .await
        .unwrap();
    let view = answered(&r.rig).await;
    assert_eq!(view.answer.as_deref(), Some("Here you go."));
    let chip = view.brain.unwrap();
    assert_eq!(chip.name, "OpenAI");
    assert!(chip.reason.contains("took over"), "{}", chip.reason);
    assert!(local.requests.lock().unwrap().is_empty());
    r.stop().await;
}

/// PLAN-05, M3-X3: a provider that is down gives a plain message, and KIVO carries on: the
/// next request is answered.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failing_provider_is_reported_plainly_and_kivo_carries_on() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start(false);
    r.rig.brains.insert(brain(
        "gemini",
        "Google Gemini",
        PrivacyClass::Cloud,
        vec![
            Script::error(NormalizedError::ProviderDown("503".into())),
            Script::text("Back again."),
        ],
    ));
    r.rig.engine.say("tell me something nice").await.unwrap();
    let view = answered(&r.rig).await;
    assert_eq!(
        view.error.as_deref(),
        Some("Google Gemini isn’t responding right now.")
    );
    r.rig.engine.say("tell me something nice").await.unwrap();
    until("the second answer", Duration::from_secs(10), || {
        r.rig.core.turn_view().and_then(|t| t.answer).as_deref() == Some("Back again.")
    })
    .await;
    r.stop().await;
}

/// SECURITY §6, invariant 11: sensitive data goes only to a brain on this PC; with none, nothing
/// is sent and KIVO says so.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sensitive_requests_stay_on_this_pc() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start(false);
    let cloud = brain(
        "anthropic",
        "Anthropic",
        PrivacyClass::Cloud,
        vec![Script::text("cloud")],
    );
    r.rig.brains.insert(cloud.clone());
    r.rig
        .engine
        .say("is my card 4111 1111 1111 1111 still valid")
        .await
        .unwrap();
    let view = answered(&r.rig).await;
    assert_eq!(
        view.answer.as_deref(),
        Some("That needs a brain on this PC, and none is running, so I didn’t send it anywhere.")
    );
    assert!(cloud.requests.lock().unwrap().is_empty());

    let local = brain(
        "ollama",
        "Ollama",
        PrivacyClass::Local,
        vec![Script::text("It looks like a test number.")],
    );
    r.rig.brains.insert(local.clone());
    r.rig
        .engine
        .say("is my card 4111 1111 1111 1111 still valid")
        .await
        .unwrap();
    until("the local answer", Duration::from_secs(10), || {
        r.rig.core.turn_view().and_then(|t| t.answer).as_deref()
            == Some("It looks like a test number.")
    })
    .await;
    let chip = r.rig.core.turn_view().unwrap().brain.unwrap();
    assert!(
        chip.local && chip.reason.contains("sensitive"),
        "{}",
        chip.reason
    );
    assert!(cloud.requests.lock().unwrap().is_empty());
    r.stop().await;
}

/// M3-X2: cancelling in the middle of a streamed answer stops it at once.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancel_stops_a_streaming_answer_within_100_ms() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start(false);
    let mut slow = Script::text("one two three four five six seven eight nine ten");
    slow.pause = Duration::from_millis(200);
    slow.hang = true;
    let b = brain("openai", "OpenAI", PrivacyClass::Cloud, vec![slow]);
    r.rig.brains.insert(b.clone());
    r.rig.engine.say("count slowly").await.unwrap();
    until("the answer to start", Duration::from_secs(5), || {
        r.rig
            .core
            .turn_view()
            .and_then(|t| t.answer)
            .is_some_and(|a| !a.is_empty())
    })
    .await;
    let pressed = Instant::now();
    r.rig
        .engine
        .cancel(kivo_core::event::CancelReason::UserButton);
    until("idle", Duration::from_millis(100), || {
        r.rig.core.state().borrow().session == SessionState::Idle
    })
    .await;
    let took = pressed.elapsed();
    eprintln!("cancel → idle: {took:?}");
    // Nothing more arrives in the card once it's stopped.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(r.rig.core.turn_view().is_none_or(
        |t| t.answer.as_deref() != Some("one two three four five six seven eight nine ten")
    ));
    r.stop().await;
}

/// CONV-01: a follow-up in the same voice session continues the thread, so the brain sees the
/// earlier turn; "new topic" starts a new one.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn follow_ups_continue_the_thread_and_new_topic_starts_another() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start(false);
    let b = brain(
        "anthropic",
        "Anthropic",
        PrivacyClass::Cloud,
        vec![
            Script::text("It's 24 degrees in Paris."),
            Script::text("Tomorrow will be 26."),
            Script::text("Rust is a systems language."),
            Script::text("Rain the day after."),
            Script::text("A sunny weekend."),
        ],
    );
    r.rig.brains.insert(b.clone());
    r.rig.engine.say("weather in Paris").await.unwrap();
    answered(&r.rig).await;
    r.rig.engine.say("and tomorrow?").await.unwrap();
    until("the second answer", Duration::from_secs(10), || {
        r.rig.core.turn_view().and_then(|t| t.answer).as_deref() == Some("Tomorrow will be 26.")
    })
    .await;
    until("idle", Duration::from_secs(5), || {
        r.rig.core.state().borrow().session == SessionState::Idle
    })
    .await;
    r.rig.engine.say("new topic: what is Rust").await.unwrap();
    until("the third answer", Duration::from_secs(10), || {
        r.rig.core.turn_view().and_then(|t| t.answer).as_deref()
            == Some("Rust is a systems language.")
    })
    .await;
    let requests = b.requests.lock().unwrap().clone();
    let texts = |i: usize| -> Vec<String> {
        requests[i]
            .messages
            .iter()
            .map(kivo_brain::Message::text)
            .collect()
    };
    assert_eq!(
        texts(1),
        [
            "weather in Paris",
            "It's 24 degrees in Paris.",
            "and tomorrow?"
        ]
    );
    assert_eq!(texts(2), ["what is Rust"]);
    assert_eq!(r.rig.db.lock().unwrap().conversations(10).unwrap().len(), 2);

    // "Continue" in Chat (CONV-03): the next voice request joins the Paris thread again.
    let paris = r
        .rig
        .db
        .lock()
        .unwrap()
        .conversations(10)
        .unwrap()
        .into_iter()
        .find(|c| c.title == "weather in Paris")
        .unwrap()
        .id;
    until("idle", Duration::from_secs(5), || {
        r.rig.core.state().borrow().session == SessionState::Idle
    })
    .await;
    r.rig.engine.continue_thread(&paris);
    r.rig.engine.say("and the day after?").await.unwrap();
    until("the fourth answer", Duration::from_secs(10), || {
        r.rig.core.turn_view().and_then(|t| t.answer).as_deref() == Some("Rain the day after.")
    })
    .await;
    let requests = b.requests.lock().unwrap().clone();
    let fourth: Vec<String> = requests[3]
        .messages
        .iter()
        .map(kivo_brain::Message::text)
        .collect();
    assert_eq!(
        fourth.first().map(String::as_str),
        Some("weather in Paris"),
        "{fourth:?}"
    );

    // "Start each conversation fresh" (CONV-31): a new session never joins an earlier thread.
    until("idle", Duration::from_secs(5), || {
        r.rig.core.state().borrow().session == SessionState::Idle
    })
    .await;
    r.rig.core.update_config(|c| {
        c.brains.voice_session_minutes = 0;
        c.context.fresh_start = true;
    });
    r.rig.engine.say("and the weekend in Paris?").await.unwrap();
    until("the fifth answer", Duration::from_secs(10), || {
        r.rig.core.turn_view().and_then(|t| t.answer).as_deref() == Some("A sunny weekend.")
    })
    .await;
    let requests = b.requests.lock().unwrap().clone();
    let fifth: Vec<String> = requests[4]
        .messages
        .iter()
        .map(kivo_brain::Message::text)
        .collect();
    assert_eq!(fifth, ["and the weekend in Paris?"]);
    assert_eq!(r.rig.db.lock().unwrap().conversations(10).unwrap().len(), 3);
    r.stop().await;
}

/// SEC-20/21: a file from a folder the user keeps on this PC is read, but its content isn't shown
/// to a cloud brain; a request with a private label goes to a local brain instead.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn private_folders_and_labels_stay_on_this_pc() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start(false);
    let private = r.rig.screenshots.join("Legal");
    std::fs::create_dir_all(&private).unwrap();
    let contract = private.join("contract.txt");
    std::fs::write(&contract, "Secret terms: the fee is 90,000.").unwrap();
    let cloud = brain(
        "openai",
        "OpenAI",
        PrivacyClass::Cloud,
        vec![
            Script::tool(
                "files__read",
                serde_json::json!({"path": contract.display().to_string()}),
            ),
            Script::text("I can't see that file."),
        ],
    );
    r.rig.brains.insert(cloud.clone());
    r.rig.core.update_config(|c| {
        c.privacy.sensitive_folders = vec![private.display().to_string()];
        c.privacy.labels = vec![kivo_core::config::PrivacyLabel {
            text: "Project Falcon".into(),
            class: kivo_core::config::LabelClass::Sensitive,
        }];
    });
    r.rig.engine.say("read my contract").await.unwrap();
    answered(&r.rig).await;
    let requests = cloud.requests.lock().unwrap().clone();
    let result: String = requests[1]
        .messages
        .iter()
        .filter(|m| m.role == Role::Tool)
        .flat_map(|m| &m.parts)
        .filter_map(|p| match p {
            Part::ToolResult { content, .. } => Some(content.clone()),
            _ => None,
        })
        .collect();
    assert!(!result.contains("Secret terms"), "{result}");
    assert!(result.contains("stays on this PC"), "{result}");
    let kept = r.rig.db.lock().unwrap().activity(None, 50).unwrap();
    assert!(kept.iter().any(|a| a.kind == "privacy"), "{kept:?}");

    // A labelled request never goes to the cloud brain: with only it connected, it can't be
    // answered, and nothing is sent.
    until("idle", Duration::from_secs(5), || {
        r.rig.core.state().borrow().session == SessionState::Idle
    })
    .await;
    let before = cloud.requests.lock().unwrap().len();
    r.rig
        .engine
        .say("summarize the project falcon notes")
        .await
        .unwrap();
    until("idle again", Duration::from_secs(10), || {
        r.rig.core.state().borrow().session == SessionState::Idle
    })
    .await;
    assert_eq!(
        cloud.requests.lock().unwrap().len(),
        before,
        "nothing went to the cloud"
    );
    r.stop().await;
}

/// BRAIN-36: at a limit set to "block", a cloud request isn't sent; a local brain still works.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_spending_limit_can_block_cloud_requests() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start(false);
    let b = brain(
        "openai",
        "OpenAI",
        PrivacyClass::Cloud,
        vec![Script {
            events: vec![
                BrainEvent::TextDelta("Expensive answer.".into()),
                BrainEvent::Usage(Usage {
                    input_tokens: 2_000_000,
                    output_tokens: 0,
                    cached_tokens: 0,
                }),
                BrainEvent::Done(StopReason::EndTurn),
            ],
            pause: Duration::ZERO,
            hang: false,
        }],
    );
    r.rig.brains.insert(b.clone());
    r.rig.brains.set_limits(&[kivo_brain::cost::Limit {
        scope: kivo_brain::cost::Scope::Overall,
        period: kivo_brain::cost::Period::Daily,
        amount: 0.01,
        warnings: vec![80],
        at_limit: kivo_brain::cost::AtLimit::Block,
    }]);
    r.rig.engine.say("write me an essay").await.unwrap();
    answered(&r.rig).await;
    r.rig.engine.say("write me another essay").await.unwrap();
    until("the refusal", Duration::from_secs(10), || {
        r.rig
            .core
            .turn_view()
            .and_then(|t| t.answer)
            .is_some_and(|a| a.contains("spending limit"))
    })
    .await;
    assert_eq!(
        b.requests.lock().unwrap().len(),
        1,
        "the second wasn't sent"
    );
    r.stop().await;
}

/// BRAIN-28, M3-X1's shape: a spoken answer starts playing before the brain has finished.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_spoken_answer_starts_before_the_brain_finishes() {
    let _one = ONE_AT_A_TIME.lock().await;
    if !worker().is_file() {
        eprintln!("kivo-infer isn't built beside the tests; skipping");
        return;
    }
    let r = start(true);
    let mut slow = Script::text(
        "Sure. Here is the first sentence of a longer answer. And here is a second one that \
         arrives much later. Finally a third.",
    );
    slow.pause = Duration::from_millis(120);
    r.rig.brains.insert(brain(
        "anthropic",
        "Anthropic",
        PrivacyClass::Cloud,
        vec![slow],
    ));
    let asked = Instant::now();
    r.rig.engine.say("tell me something long").await.unwrap();
    until("the voice to start", Duration::from_secs(15), || {
        r.rig.engine.speaker.busy()
    })
    .await;
    let first_audio = asked.elapsed();
    let answer_then = r
        .rig
        .core
        .turn_view()
        .and_then(|t| t.answer)
        .unwrap_or_default();
    eprintln!(
        "first audio after {first_audio:?}, with {} chars shown",
        answer_then.len()
    );
    assert!(
        !answer_then.ends_with("Finally a third."),
        "speech began before the answer was complete"
    );
    until("the turn to end", Duration::from_secs(40), || {
        r.rig.core.state().borrow().session != SessionState::Speaking
            && r.rig.core.state().borrow().session != SessionState::Thinking
    })
    .await;
    r.stop().await;
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        [dir.join(name), dir.join(format!("{name}.exe"))]
            .into_iter()
            .find(|p| p.is_file())
    })
}

/// A small ACP agent: streams a message, asks to edit a file, reports the edit and finishes; the
/// session id counts prompts, so a follow-up can be seen reusing the session. `die` makes it exit
/// in the middle of a prompt.
fn fake_agent(die: bool) -> String {
    format!(
        r#"
const rl = require('readline').createInterface({{ input: process.stdin }});
const send = m => process.stdout.write(JSON.stringify(m) + '\n');
let prompts = 0, next = 1000, waiting = null;
const update = u => send({{ jsonrpc: '2.0', method: 'session/update', params: {{ sessionId: 's1', update: u }} }});
rl.on('line', l => {{
  const m = JSON.parse(l);
  if (m.method === 'initialize') send({{ jsonrpc: '2.0', id: m.id, result: {{ protocolVersion: 1, agentCapabilities: {{ loadSession: true }} }} }});
  else if (m.method === 'session/new' || m.method === 'session/load') send({{ jsonrpc: '2.0', id: m.id, result: {{ sessionId: 's1' }} }});
  else if (m.method === 'session/prompt') {{
    prompts++;
    if ({die}) {{ update({{ sessionUpdate: 'agent_message_chunk', content: {{ type: 'text', text: 'Starting.' }} }}); setTimeout(() => process.exit(3), 50); return; }}
    update({{ sessionUpdate: 'plan', entries: [{{ content: 'Fix the bug', status: 'in_progress', priority: 'high' }}] }});
    update({{ sessionUpdate: 'agent_message_chunk', content: {{ type: 'text', text: 'Prompt ' + prompts + '. ' }} }});
    const ask = next++;
    waiting = {{ prompt: m.id, ask }};
    send({{ jsonrpc: '2.0', id: ask, method: 'session/request_permission', params: {{ sessionId: 's1',
      toolCall: {{ toolCallId: 't' + prompts, title: 'Edit main.rs', kind: 'edit' }},
      options: [{{ optionId: 'yes', name: 'Allow', kind: 'allow_once' }}, {{ optionId: 'no', name: 'Reject', kind: 'reject_once' }}] }} }});
  }} else if (waiting && m.id === waiting.ask) {{
    const allowed = m.result && m.result.outcome && m.result.outcome.optionId === 'yes';
    update({{ sessionUpdate: 'tool_call', toolCallId: 'e1', title: 'Edit main.rs', kind: 'edit', status: allowed ? 'completed' : 'failed' }});
    update({{ sessionUpdate: 'agent_message_chunk', content: {{ type: 'text', text: allowed ? 'I fixed the bug.' : 'I left it alone.' }} }});
    send({{ jsonrpc: '2.0', id: waiting.prompt, result: {{ stopReason: 'end_turn' }} }});
    waiting = null;
  }}
}});"#
    )
}

fn connect_agent(r: &Running, node: &std::path::Path, die: bool) {
    r.rig.agents.set_command(
        "claude-code",
        kivo_brain::acp::AgentCommand {
            program: node.to_path_buf(),
            args: vec!["-e".into(), fake_agent(die)],
            env: Vec::new(),
        },
    );
    r.rig.core.update_config(|c| {
        c.capabilities.set(Capability::CliAgents, true);
        c.brains.connections.push(BrainConnection {
            id: "claude-code".into(),
            enabled: true,
            ..BrainConnection::default()
        });
    });
    let config = r.rig.core.config();
    r.rig.brains.reload(&config);
    r.rig.brains.set_agents(
        [(
            "claude-code".to_owned(),
            kivo_runtime::brains::FoundAgent {
                program: node.to_path_buf(),
                version: Some("test".into()),
                signed_in: Some(true),
            },
        )]
        .into(),
    );
}

/// BRAIN-14/15, CONV-02, CONV-12, M3 deliverable: "use Claude Code …" goes to the agent over
/// ACP; its request to edit a file is decided by KIVO's permission engine and shown on the card;
/// its plan and work show as steps; a follow-up ("Tell Claude Code …") reuses the same session,
/// whose id is stored for resuming.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claude_code_asks_for_permission_on_the_card_and_follow_ups_reuse_its_session() {
    let _one = ONE_AT_A_TIME.lock().await;
    let Some(node) = which("node") else {
        eprintln!("node isn't installed; skipping");
        return;
    };
    let r = start(false);
    connect_agent(&r, &node, false);
    r.rig
        .engine
        .say("use Claude Code to fix the bug in main.rs")
        .await
        .unwrap();
    until("the agent's question", Duration::from_secs(20), || {
        r.rig.core.turn_view().and_then(|t| t.confirm).is_some()
    })
    .await;
    let view = r.rig.core.turn_view().unwrap();
    let confirm = view.confirm.unwrap();
    assert_eq!(confirm.tool, "agent.edit");
    assert_eq!(confirm.action, "Claude Code: Edit main.rs");
    assert!(
        view.brain
            .unwrap()
            .reason
            .contains("because you asked for Claude Code")
    );
    assert!(view.steps.iter().any(|s| s.title == "Fix the bug"));
    r.rig
        .engine
        .answer_confirmation(&confirm.call_id, true, false)
        .await
        .unwrap();
    let view = answered(&r.rig).await;
    // A fix is only called done after a check (BRAIN-31); this folder has no project to check.
    assert_eq!(
        view.answer.as_deref(),
        Some(
            "Prompt 1. I fixed the bug.\n\nI couldn’t find how to check this project, so I can’t confirm it’s fixed."
        )
    );
    assert!(
        view.steps
            .iter()
            .any(|s| s.title == "Edit main.rs" && s.status == kivo_core::event::StepStatus::Done)
    );
    assert!(
        r.rig
            .db
            .lock()
            .unwrap()
            .agent_session("claude-code", &std::env::temp_dir().to_string_lossy())
            .unwrap()
            .is_some_and(|s| s.id == "s1"),
        "the session id is stored (CONV-02)"
    );

    // "Tell Claude Code …": the same session (the agent counts its prompts).
    r.rig
        .engine
        .say("tell Claude Code to also add a test")
        .await
        .unwrap();
    until("the second question", Duration::from_secs(20), || {
        r.rig.core.turn_view().and_then(|t| t.confirm).is_some()
    })
    .await;
    let confirm = r.rig.core.turn_view().unwrap().confirm.unwrap();
    // Declining doesn't end the turn: the agent is told no and carries on.
    r.rig
        .engine
        .answer_confirmation(&confirm.call_id, false, false)
        .await
        .unwrap();
    let view = answered(&r.rig).await;
    assert_eq!(
        view.answer.as_deref(),
        Some(
            "Prompt 2. I left it alone.\n\nI couldn’t find how to check this project, so I can’t confirm it’s fixed."
        )
    );
    assert_eq!(r.rig.agents.running().await.len(), 1);
    r.rig.agents.stop_all().await;
    r.stop().await;
}

/// M3-X3: an agent process that dies mid-request is reported, KIVO keeps running, and the next
/// request starts the agent again.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_agent_that_dies_is_reported_and_restarted_next_time() {
    let _one = ONE_AT_A_TIME.lock().await;
    let Some(node) = which("node") else {
        eprintln!("node isn't installed; skipping");
        return;
    };
    let r = start(false);
    connect_agent(&r, &node, true);
    r.rig
        .engine
        .say("use Claude Code to fix the bug")
        .await
        .unwrap();
    let view = answered(&r.rig).await;
    assert_eq!(
        view.error.as_deref(),
        Some("Claude Code isn’t responding right now.")
    );
    assert!(r.rig.agents.running().await.is_empty());
    // A working agent next time.
    connect_agent(&r, &node, false);
    r.rig
        .engine
        .say("use Claude Code to fix the bug")
        .await
        .unwrap();
    until("the question", Duration::from_secs(20), || {
        r.rig.core.turn_view().and_then(|t| t.confirm).is_some()
    })
    .await;
    r.rig
        .engine
        .cancel(kivo_core::event::CancelReason::UserButton);
    r.rig.agents.stop_all().await;
    r.stop().await;
}

/// UX-21/22, BRAIN-18, CONV-06, MEM-03, BRAIN-06, SEC-19: the Control Center's brain requests.
/// Keys are tested before they're stored and never come back; Chat threads, compaction into a
/// running summary, stated preferences in the next request, the usage summary and CSV, the
/// context preview, and "that's not what I meant".
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_control_centers_brain_requests() {
    use kivo_brain::testing::{MockServer, Reply};
    use kivo_ipc::method;
    use serde_json::{Value, json};
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start(false);
    let discovery = kivo_runtime::discovery::Discovery::new(
        Arc::clone(&r.rig.db),
        Vec::new(),
        Arc::clone(&r.rig.brains),
    );
    let exports = tempfile::tempdir().unwrap();
    let rpc = Arc::new(
        kivo_runtime::brains_rpc::BrainsRpc::new(
            Arc::clone(&r.rig.core),
            Arc::clone(&r.rig.engine),
            discovery,
            r.rig.recorder.clone(),
            r.rig.control.clone(),
            Arc::new(|_: &std::path::Path, _: &[String]| Ok(())),
        )
        .with_exports(exports.path().to_path_buf()),
    );
    let call = |name: &'static str, params: Value| {
        let rpc = Arc::clone(&rpc);
        async move { rpc.call(name, params).await.expect("handled") }
    };

    // The catalog: no-key sign-in first, free options labelled (BRAIN-17, CONV-08).
    let catalog = call(method::BRAINS_CATALOG, Value::Null).await.unwrap();
    let first = &catalog["brains"][0];
    assert_eq!(first["signIn"], "cliLogin");
    assert!(
        catalog["brains"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["id"] == "gemini-cli" && b["free"].is_string())
    );

    // A key is tested by the runtime before it's stored, and never sent back.
    let api = MockServer::start(|r| {
        if r.header("authorization") == Some("Bearer sk-good-123") {
            Reply::json(200, &json!({"data": [{"id": "gpt-4o"}]}))
        } else {
            Reply::json(401, &json!({"error": {"message": "bad key"}}))
        }
    })
    .await;
    call(
        method::BRAINS_CONNECT,
        json!({"id": "openai", "baseUrl": format!("{}/v1", api.url)}),
    )
    .await
    .unwrap();
    let bad = call(
        method::BRAINS_SET_KEY,
        json!({"id": "openai", "key": "sk-nope"}),
    )
    .await;
    assert!(bad.is_err(), "a refused key isn't stored");
    let good = call(
        method::BRAINS_SET_KEY,
        json!({"id": "openai", "key": "sk-good-123"}),
    )
    .await
    .unwrap();
    assert_eq!(good["health"]["state"], "ready");
    let list = call(method::BRAINS_LIST, Value::Null).await.unwrap();
    let text = list.to_string();
    assert!(!text.contains("sk-good-123"), "no key in the list (SEC-19)");
    let openai = list["connected"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == "openai")
        .unwrap()
        .clone();
    assert_eq!(openai["hasKey"], true);
    let config = serde_json::to_string(&r.rig.core.config()).unwrap();
    assert!(!config.contains("sk-good-123") && config.contains("secret://kivo/openai/api-key"));
    call(method::BRAINS_DISCONNECT, json!({"id": "openai"}))
        .await
        .unwrap();

    // BRAIN-11: a custom OpenAI-compatible endpoint needs its address, has its key tested like
    // any other, and counts as a cloud brain (privacy mode applies to it).
    assert!(
        call(
            method::BRAINS_CONNECT,
            json!({"id": "custom-mybox", "name": "My box"})
        )
        .await
        .is_err(),
        "no address, no brain"
    );
    call(
        method::BRAINS_CONNECT,
        json!({"id": "custom-mybox", "name": "My box", "baseUrl": format!("{}/v1", api.url)}),
    )
    .await
    .unwrap();
    let custom = call(
        method::BRAINS_SET_KEY,
        json!({"id": "custom-mybox", "key": "sk-good-123"}),
    )
    .await
    .unwrap();
    assert_eq!(custom["health"]["state"], "ready");
    let list = call(method::BRAINS_LIST, Value::Null).await.unwrap();
    let mybox = list["connected"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == "custom-mybox")
        .unwrap()
        .clone();
    assert_eq!(mybox["name"], "My box");
    assert_eq!(mybox["privacy"], "cloud");
    call(method::BRAINS_DISCONNECT, json!({"id": "custom-mybox"}))
        .await
        .unwrap();

    // Chat: a thread, three exchanges, then "Compact now" (CONV-06) and a stated preference.
    let b = brain(
        "anthropic",
        "Anthropic",
        PrivacyClass::Cloud,
        vec![
            Script::text("One."),
            Script::text("Two."),
            Script::text("Three."),
            Script::text("The user counted to three with KIVO."),
            Script::text("Four, Sam."),
        ],
    );
    r.rig.brains.insert(b.clone());
    let thread = call(method::CHAT_NEW, json!({"title": "Counting"}))
        .await
        .unwrap();
    let id = thread["id"].as_str().unwrap().to_owned();
    for (i, text) in ["say one", "say two", "say three"].iter().enumerate() {
        call(method::CHAT_SEND, json!({"thread": id, "text": text}))
            .await
            .unwrap();
        until("the answer", Duration::from_secs(10), || {
            r.rig.core.state().borrow().session == SessionState::Idle
                && r.rig.core.turn_view().and_then(|t| t.answer).is_some()
        })
        .await;
        let messages = call(method::CHAT_THREAD, json!({"id": id})).await.unwrap();
        assert_eq!(messages["messages"].as_array().unwrap().len(), (i + 1) * 2);
    }
    call(method::CHAT_COMPACT, json!({"id": id})).await.unwrap();
    let t = call(method::CHAT_THREAD, json!({"id": id})).await.unwrap();
    assert_eq!(
        t["thread"]["summary"],
        "The user counted to three with KIVO."
    );
    call(
        method::MEMORY_SET_PREFERENCE,
        json!({"key": "name", "value": "Call me Sam"}),
    )
    .await
    .unwrap();
    call(method::CHAT_SEND, json!({"thread": id, "text": "say four"}))
        .await
        .unwrap();
    until("the fourth answer", Duration::from_secs(10), || {
        r.rig.core.turn_view().and_then(|t| t.answer).as_deref() == Some("Four, Sam.")
    })
    .await;
    let last = b.requests.lock().unwrap().last().cloned().unwrap();
    let system = last.system_text();
    assert!(
        system.contains("Earlier in this conversation: The user counted to three"),
        "{system}"
    );
    assert!(system.contains("Call me Sam"), "{system}");
    // The summarized first exchange isn't sent verbatim any more; the last four turns are.
    let sent: Vec<String> = last
        .messages
        .iter()
        .map(kivo_brain::Message::text)
        .collect();
    assert!(!sent.contains(&"say one".to_owned()), "{sent:?}");
    assert!(sent.contains(&"say three".to_owned()) && sent.contains(&"say four".to_owned()));
    let found = call(method::CHAT_SEARCH, json!({"query": "three"}))
        .await
        .unwrap();
    assert!(!found.as_array().unwrap().is_empty());

    // Usage: every request metered, with a CSV export.
    let usage = call(method::USAGE_SUMMARY, json!({"days": 7}))
        .await
        .unwrap();
    assert_eq!(usage["requests"], 5, "4 answers and the summary");
    assert!(usage["total"].as_f64().unwrap() > 0.0);
    // BRAIN-37: today's spend, the AI/speech split per day, and the costliest requests named by
    // what was said.
    assert!((usage["today"].as_f64().unwrap() - usage["total"].as_f64().unwrap()).abs() < 1e-9);
    let day = &usage["byDay"][0];
    assert!(day["ai"].as_f64().unwrap() > 0.0 && day["speech"].as_f64() == Some(0.0));
    let top = usage["top"].as_array().unwrap();
    assert!(!top.is_empty() && top[0]["kind"] == "turn");
    assert!(top.iter().any(|t| t["title"] == "say three"), "{top:?}");
    assert!(usage["turns"].as_u64().unwrap() >= 4);
    assert!(usage["freeTurns"].as_u64().unwrap() <= usage["turns"].as_u64().unwrap());
    let csv = call(method::USAGE_EXPORT, json!({"days": 7}))
        .await
        .unwrap();
    assert_eq!(csv["csv"].as_str().unwrap().lines().count(), 6);
    // Saved to the Downloads folder for the Usage page's CSV button.
    let saved = call(method::USAGE_EXPORT, json!({"days": 7, "save": true}))
        .await
        .unwrap();
    let file = saved["file"].as_str().unwrap();
    assert!(file.ends_with(".csv") && file.contains("KIVO usage"));
    assert_eq!(std::fs::read_to_string(file).unwrap().lines().count(), 6);

    // The context preview (Settings → Context).
    let preview = call(method::BRAINS_CONTEXT, json!({"thread": id}))
        .await
        .unwrap();
    assert_eq!(preview["brain"], "Anthropic");
    assert!(preview["layers"][0]["tokens"].as_u64().unwrap() > 0);
    // "Preview what the AI sees": the whole request, secrets redacted; a session's cost.
    let text = preview["preview"].as_str().unwrap();
    assert!(text.starts_with("[system, cached]"), "{text}");
    assert!(text.contains("[user]"), "{text}");
    assert!(preview["previewTokens"].as_u64().unwrap() > 0);
    // Priced when the model's price is known; this test's model isn't in the table.
    assert!(preview.get("sessionCost").is_some(), "{preview}");
    assert!(
        preview["layers"]
            .as_array()
            .unwrap()
            .iter()
            .all(|l| l["on"] == true)
    );
    // Settings → Context turns layers off (CONV-31).
    r.rig.core.update_config(|c| {
        c.context.about_me = false;
        c.context.skills = false;
        c.context.live_fields = vec!["local time".into()];
    });
    let preview = call(method::BRAINS_CONTEXT, json!({"thread": id}))
        .await
        .unwrap();
    let layer = |id: &str| {
        preview["layers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["id"] == id)
            .unwrap()
            .clone()
    };
    assert_eq!(layer("instructions")["on"], false);
    assert_eq!(layer("instructions")["tokens"], 0);
    assert_eq!(layer("skills")["on"], false);
    let text = preview["preview"].as_str().unwrap();
    assert!(
        text.contains("local time") && !text.contains("permission mode"),
        "{text}"
    );

    // Chat's power features (CONV-03): branch, export, continue.
    let thread = call(method::CHAT_THREAD, json!({"id": id})).await.unwrap();
    let first = thread["messages"][0]["id"].as_i64().unwrap();
    let branch = call(method::CHAT_BRANCH, json!({"id": id, "message": first}))
        .await
        .unwrap();
    assert!(
        branch["title"].as_str().unwrap().ends_with("(branch)"),
        "{branch}"
    );
    let copied = call(method::CHAT_THREAD, json!({"id": branch["id"]}))
        .await
        .unwrap();
    assert_eq!(copied["messages"].as_array().unwrap().len(), 1);
    let md = call(method::CHAT_EXPORT, json!({"id": id})).await.unwrap();
    let md = md["text"].as_str().unwrap();
    assert!(
        md.starts_with("# ") && md.contains("**You**") && md.contains("**KIVO"),
        "{md}"
    );
    let saved = call(
        method::CHAT_EXPORT,
        json!({"id": id, "json": true, "save": true}),
    )
    .await
    .unwrap();
    let file = saved["file"].as_str().unwrap();
    assert!(file.ends_with(".json"), "{file}");
    let back: Value = serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
    assert_eq!(back["thread"]["id"], id.as_str());
    assert!(
        call(method::CHAT_CONTINUE, json!({"id": branch["id"]}))
            .await
            .is_ok()
    );
    assert!(
        call(method::CHAT_CONTINUE, json!({"id": "nope"}))
            .await
            .is_err()
    );

    // "That's not what I meant" (BRAIN-06).
    let turn = r.rig.core.turn_view().unwrap().id;
    let reported = call(
        method::CHAT_MISROUTE,
        json!({"turnId": turn, "note": "I wanted a timer"}),
    )
    .await
    .unwrap();
    assert_eq!(reported["misroutes"], 1);
    assert_eq!(r.rig.engine.router_metrics().misroutes, 1);
    r.stop().await;
}

/// M3-X2 for agents: cancelling mid-stream stops the turn within 100 ms and the agent is told
/// (`session/cancel`), ending its prompt as cancelled.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancel_stops_an_agent_mid_stream() {
    let _one = ONE_AT_A_TIME.lock().await;
    let Some(node) = which("node") else {
        eprintln!("node isn't installed; skipping");
        return;
    };
    let marker = std::env::temp_dir().join(format!("kivo-agent-cancel-{}", std::process::id()));
    let _ = std::fs::remove_file(&marker);
    let script = format!(
        r#"
const fs = require('fs');
const rl = require('readline').createInterface({{ input: process.stdin }});
const send = m => process.stdout.write(JSON.stringify(m) + '\n');
let timer = null, prompt = null;
rl.on('line', l => {{
  const m = JSON.parse(l);
  if (m.method === 'initialize') send({{ jsonrpc: '2.0', id: m.id, result: {{ protocolVersion: 1, agentCapabilities: {{}} }} }});
  else if (m.method === 'session/new') send({{ jsonrpc: '2.0', id: m.id, result: {{ sessionId: 's1' }} }});
  else if (m.method === 'session/prompt') {{
    prompt = m.id;
    let n = 0;
    timer = setInterval(() => send({{ jsonrpc: '2.0', method: 'session/update', params: {{ sessionId: 's1', update: {{ sessionUpdate: 'agent_message_chunk', content: {{ type: 'text', text: 'word' + (n++) + ' ' }} }} }} }}), 150);
  }} else if (m.method === 'session/cancel') {{
    clearInterval(timer);
    fs.writeFileSync({marker:?}, 'cancelled');
    send({{ jsonrpc: '2.0', id: prompt, result: {{ stopReason: 'cancelled' }} }});
  }}
}});"#,
        marker = marker.to_string_lossy()
    );
    let r = start(false);
    r.rig.agents.set_command(
        "claude-code",
        kivo_brain::acp::AgentCommand {
            program: node.clone(),
            args: vec!["-e".into(), script],
            env: Vec::new(),
        },
    );
    connect_agent_only(&r, &node);
    r.rig
        .engine
        .say("use Claude Code to refactor everything")
        .await
        .unwrap();
    until(
        "the agent to start talking",
        Duration::from_secs(20),
        || {
            r.rig
                .core
                .turn_view()
                .and_then(|t| t.answer)
                .is_some_and(|a| a.contains("word1"))
        },
    )
    .await;
    let pressed = Instant::now();
    r.rig
        .engine
        .cancel(kivo_core::event::CancelReason::UserButton);
    until("idle", Duration::from_millis(100), || {
        r.rig.core.state().borrow().session == SessionState::Idle
    })
    .await;
    eprintln!("cancel → idle: {:?}", pressed.elapsed());
    until("the agent to be told", Duration::from_secs(5), || {
        marker.is_file()
    })
    .await;
    let _ = std::fs::remove_file(&marker);
    r.rig.agents.stop_all().await;
    r.stop().await;
}

/// Connects Claude Code as found on this PC (the command itself is set by the caller).
fn connect_agent_only(r: &Running, node: &std::path::Path) {
    r.rig.core.update_config(|c| {
        c.capabilities.set(Capability::CliAgents, true);
        c.brains.connections.push(BrainConnection {
            id: "claude-code".into(),
            enabled: true,
            ..BrainConnection::default()
        });
    });
    let config = r.rig.core.config();
    r.rig.brains.reload(&config);
    r.rig.brains.set_agents(
        [(
            "claude-code".to_owned(),
            kivo_runtime::brains::FoundAgent {
                program: node.to_path_buf(),
                version: None,
                signed_in: Some(true),
            },
        )]
        .into(),
    );
}
