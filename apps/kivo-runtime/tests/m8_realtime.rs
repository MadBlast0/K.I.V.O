//! Realtime conversation mode (BRAIN-33) on the real runtime pieces, with a scripted
//! speech-to-speech provider in place of OpenAI Realtime / Gemini Live (their wire formats are
//! tested against local WebSocket servers in `kivo-brain`):
//!
//! - the fast path runs first: "mute" never opens a session;
//! - "let's talk about …" opens one with the persona's instructions and the offered tools;
//! - the model's tool calls go through the permission engine: a Low-risk one runs, a Medium-risk
//!   one asks on the card, and declining it declines only that action;
//! - transcripts show on the card and go to the thread; "that's all" closes the session;
//! - the card's "Talk live" opens one without a request, and silence closes it.

#![cfg(windows)]

use async_trait::async_trait;
use kivo_brain::realtime::{Control, RealtimeConfig, RealtimeProvider, RealtimeSession, RtEvent};
use kivo_brain::testing::{Script, ScriptedBrain};
use kivo_brain::{NormalizedError, PrivacyClass};
use kivo_core::{Capability, SessionState};
use kivo_runtime::scripted::{self, Rig};
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

fn worker() -> PathBuf {
    let mut dir = std::env::current_exe().expect("test binary");
    dir.pop();
    dir.pop();
    dir.join("kivo-infer.exe")
}

async fn until(what: &str, timeout: Duration, mut check: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + timeout;
    while !check() {
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// The test's end of a session: what it sends as the provider, and what KIVO sent it.
struct Line {
    events: mpsc::UnboundedSender<RtEvent>,
    control: mpsc::UnboundedReceiver<Control>,
    _audio: mpsc::Receiver<Vec<f32>>,
}

impl Line {
    async fn next_control(&mut self) -> Control {
        tokio::time::timeout(Duration::from_secs(15), self.control.recv())
            .await
            .expect("KIVO answers the provider in time")
            .expect("the session is open")
    }
}

#[derive(Default)]
struct ScriptedLive {
    connects: Mutex<Vec<RealtimeConfig>>,
    lines: Mutex<Vec<Line>>,
}

#[async_trait]
impl RealtimeProvider for ScriptedLive {
    fn id(&self) -> &'static str {
        "openai"
    }
    fn name(&self) -> String {
        "Scripted Live".into()
    }
    fn default_model(&self) -> &'static str {
        "scripted-live"
    }
    fn cost_per_minute(&self) -> f64 {
        0.05
    }
    async fn connect(
        &self,
        config: RealtimeConfig,
        _cancel: tokio_util::sync::CancellationToken,
    ) -> Result<RealtimeSession, NormalizedError> {
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let (control_tx, control_rx) = mpsc::unbounded_channel();
        let (audio_tx, audio_rx) = mpsc::channel(64);
        self.connects.lock().unwrap().push(config);
        self.lines.lock().unwrap().push(Line {
            events: events_tx,
            control: control_rx,
            _audio: audio_rx,
        });
        Ok(RealtimeSession {
            audio: audio_tx,
            control: control_tx,
            events: events_rx,
        })
    }
}

async fn take_line(live: &ScriptedLive) -> Line {
    until("a session to open", Duration::from_secs(15), || {
        !live.lines.lock().unwrap().is_empty()
    })
    .await;
    live.lines.lock().unwrap().remove(0)
}

async fn answer(rig: &Rig, text: &str) -> String {
    let before = rig.core.turn_view().map(|t| t.id);
    rig.engine.say(text).await.unwrap();
    let mut said = String::new();
    until(
        &format!("an answer to {text:?}"),
        Duration::from_secs(20),
        || match rig.core.turn_view() {
            Some(v)
                if Some(&v.id) != before.as_ref()
                    && rig.core.state().borrow().session == SessionState::Idle =>
            {
                said = v.answer.unwrap_or_default();
                !said.is_empty()
            }
            _ => false,
        },
    )
    .await;
    said
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_live_conversation_runs_tools_through_the_permission_engine_and_closes_on_request() {
    let (rig, worker_task, pump) = scripted::rig(worker(), Vec::new(), PathBuf::from("no-model"));
    rig.core.update_config(|c| {
        c.capabilities.set(Capability::RealtimeVoice, true);
        c.capabilities.set(Capability::SpeakResponses, false);
    });
    rig.brains.insert(Arc::new(ScriptedBrain::new(
        "openai",
        PrivacyClass::Cloud,
        vec![Script::text("A chat answer.")],
    )));
    let live = Arc::new(ScriptedLive::default());
    rig.brains
        .set_realtime(Some(live.clone() as Arc<dyn RealtimeProvider>));

    // The fast path runs first: "mute" is a command, never a conversation.
    answer(&rig, "mute").await;
    assert!(rig.control.volume.lock().unwrap().muted);
    assert!(
        live.connects.lock().unwrap().is_empty(),
        "no session for a command"
    );
    rig.control.volume.lock().unwrap().muted = false;

    rig.engine.say("let's talk about Rome").await.unwrap();
    let mut line = take_line(&live).await;
    let config = live.connects.lock().unwrap()[0].clone();
    assert!(
        config.instructions.contains("live spoken conversation"),
        "{}",
        config.instructions
    );
    let offered: Vec<String> = config.tools.iter().map(|t| t.name.clone()).collect();
    assert!(offered.iter().any(|t| t == "audio__mute"), "{offered:?}");
    assert_eq!(
        line.next_control().await,
        Control::Text("let's talk about Rome".into())
    );

    line.events.send(RtEvent::Ready).unwrap();
    until(
        "the live chip and listening",
        Duration::from_secs(5),
        || {
            rig.core.turn_view().is_some_and(|v| v.live.is_some())
                && rig.core.state().borrow().session == SessionState::Listening
        },
    )
    .await;
    assert!(rig.engine.live());

    // A Low-risk tool runs at once, and the model gets its result.
    line.events
        .send(RtEvent::UserTranscript("please mute the sound".into()))
        .unwrap();
    line.events
        .send(RtEvent::ToolCall {
            id: "c1".into(),
            name: "audio__mute".into(),
            args: json!({}),
        })
        .unwrap();
    let Control::ToolResult { id, output, .. } = line.next_control().await else {
        panic!("a tool result");
    };
    assert_eq!(id, "c1");
    assert!(output.contains("\"ok\":true"), "{output}");
    assert!(rig.control.volume.lock().unwrap().muted, "mute ran");

    // What the model says shows on the card.
    line.events
        .send(RtEvent::ModelTranscript("Muted. ".into()))
        .unwrap();
    line.events
        .send(RtEvent::ModelTranscript("So, ancient Rome?".into()))
        .unwrap();
    line.events.send(RtEvent::ModelTurnDone).unwrap();
    until(
        "the model's line on the card",
        Duration::from_secs(5),
        || {
            rig.core.turn_view().and_then(|v| v.answer).as_deref()
                == Some("Muted. So, ancient Rome?")
        },
    )
    .await;

    // A Medium-risk action asks on the card; declining it declines only that action.
    line.events
        .send(RtEvent::ToolCall {
            id: "c2".into(),
            name: "apps__close".into(),
            args: json!({ "app": { "id": "Chrome", "name": "Google Chrome" } }),
        })
        .unwrap();
    until("the decision on the card", Duration::from_secs(10), || {
        rig.core.turn_view().and_then(|v| v.confirm).is_some()
    })
    .await;
    let confirm = rig.core.turn_view().unwrap().confirm.unwrap();
    assert_eq!(confirm.tool, "apps.close");
    rig.engine
        .answer_confirmation(&confirm.call_id, false, false)
        .await
        .unwrap();
    let Control::ToolResult { id, output, .. } = line.next_control().await else {
        panic!("a tool result");
    };
    assert_eq!(id, "c2");
    assert!(output.contains("declined"), "{output}");
    assert!(
        rig.apps.closed.lock().unwrap().is_empty(),
        "nothing was closed"
    );
    assert!(rig.engine.live(), "the conversation goes on");

    // "That's all" closes it: the provider is told, the card says goodbye.
    line.events
        .send(RtEvent::UserTranscript("That's all, thanks".into()))
        .unwrap();
    assert_eq!(line.next_control().await, Control::Close);
    until("the turn to end", Duration::from_secs(10), || {
        rig.core.state().borrow().session == SessionState::Idle
            && rig.core.turn_view().and_then(|v| v.answer).as_deref() == Some("Talk soon.")
    })
    .await;
    assert!(!rig.engine.live());
    assert!(rig.core.turn_view().is_some_and(|v| v.live.is_none()));
    let activity = rig.recorder.recent(None, 20);
    assert!(
        activity
            .iter()
            .any(|a| a.title == "Live conversation with Scripted Live"),
        "{:?}",
        activity.iter().map(|a| a.title.clone()).collect::<Vec<_>>()
    );
    // Both sides are in the thread, like any conversation.
    {
        let db = rig.db.lock().unwrap();
        let thread = db
            .conversations(5)
            .unwrap()
            .into_iter()
            .next()
            .expect("a thread");
        let texts: Vec<(String, String)> = db
            .messages(&thread.id)
            .unwrap()
            .into_iter()
            .map(|m| (m.role, m.text))
            .collect();
        for expected in [
            ("user", "let's talk about Rome"),
            ("user", "please mute the sound"),
            ("assistant", "Muted. So, ancient Rome?"),
        ] {
            assert!(
                texts
                    .iter()
                    .any(|(r, t)| r == expected.0 && t == expected.1),
                "{expected:?} in {texts:?}"
            );
        }
    }

    // The card's "Talk live" opens one without a request; silence closes it.
    rig.core
        .update_config(|c| c.brains.realtime.silence_seconds = 5);
    rig.engine.start_live().await.unwrap();
    let line = take_line(&live).await;
    line.events.send(RtEvent::Ready).unwrap();
    until(
        "the quiet session to close",
        Duration::from_secs(15),
        || {
            rig.core.state().borrow().session == SessionState::Idle
                && rig
                    .core
                    .turn_view()
                    .and_then(|v| v.answer)
                    .is_some_and(|a| a.contains("gone quiet"))
        },
    )
    .await;

    // With the capability off, "Talk live" is refused.
    rig.core.update_config(|c| {
        c.capabilities.set(Capability::RealtimeVoice, false);
    });
    assert!(rig.engine.start_live().await.is_err());

    rig.core.quit();
    let _ = tokio::time::timeout(Duration::from_secs(5), worker_task).await;
    pump.abort();
}
