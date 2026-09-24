//! Computer use (CAP-09–CAP-12) on the real runtime pieces, with a scripted vision model in place
//! of Anthropic / OpenAI / Gemini computer use (their wire formats are tested in `kivo-brain`),
//! the fake input device (nothing reaches the real desktop) and a fake window:
//!
//! - a brain asks for `computer.use`; the task is confirmed with its estimated cost (High risk);
//! - in watch mode (the first tasks) every step asks on the Island (Allow / Skip), with the target
//!   highlighted, and a skipped step doesn't run;
//! - each step becomes one of KIVO's input tools in screen pixels, through the permission engine;
//! - the controller live activity shows while it runs and is gone after;
//! - with approval set to Never the task's grant covers its steps, and the step limit stops it.

#![cfg(windows)]

use async_trait::async_trait;
use kivo_brain::computer::{ComputerUseProvider, CuAction, CuSession, Screenshot};
use kivo_brain::testing::{Script, ScriptedBrain};
use kivo_brain::{NormalizedError, PrivacyClass};
use kivo_core::config::ApproveSteps;
use kivo_core::{Capability, SessionState};
use kivo_platform::{Rect, WindowId, WindowInfo};
use kivo_runtime::scripted::{self, Rig};
use kivo_testkit::InputAction;
use serde_json::json;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

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

/// A vision model that answers from a script, one action per screenshot.
struct ScriptedVision {
    actions: Mutex<VecDeque<CuAction>>,
    shots: Mutex<Vec<(u32, u32)>>,
}

impl ScriptedVision {
    fn new(actions: Vec<CuAction>) -> Arc<Self> {
        Arc::new(Self {
            actions: Mutex::new(actions.into()),
            shots: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait]
impl ComputerUseProvider for ScriptedVision {
    fn id(&self) -> &str {
        "anthropic"
    }
    fn name(&self) -> &str {
        "Scripted Vision"
    }
    fn cost_per_step(&self) -> f64 {
        0.01
    }
    async fn step(
        &self,
        _session: &mut CuSession,
        shot: &Screenshot,
        _cancel: &tokio_util::sync::CancellationToken,
    ) -> Result<CuAction, NormalizedError> {
        self.shots.lock().unwrap().push((shot.width, shot.height));
        Ok(self
            .actions
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(CuAction::Click {
                x: 1,
                y: 1,
                button: "left".into(),
                count: 1,
            }))
    }
}

fn notepad() -> WindowInfo {
    WindowInfo {
        id: WindowId(4242),
        title: "Untitled - Notepad".into(),
        app_id: "notepad.exe".into(),
        // 100× the scripted screenshot (16 × 9), so a model's (1, 2) is (1100, 700) on screen.
        bounds: Rect {
            x: 1_000,
            y: 500,
            width: 1_600,
            height: 900,
        },
        minimized: false,
    }
}

fn setup(rig: &Rig, approve: ApproveSteps, max_steps: u32) {
    rig.core.update_config(|c| {
        c.capabilities.set(Capability::ComputerUse, true);
        c.capabilities.set(Capability::SpeakResponses, false);
        c.tools.cloud_vision = true;
        c.tools.computer_use.approve = approve;
        c.tools.computer_use.max_steps = max_steps;
    });
    rig.windows.windows.lock().unwrap().push(notepad());
    // The user is away from the mouse (pause-on-mouse is on by default).
    rig.system.presence.lock().unwrap().idle_seconds = 3_600;
}

async fn confirm_next(rig: &Rig, tool: &str, allow: bool) {
    until(
        &format!("the card for {tool}"),
        Duration::from_secs(15),
        || {
            rig.core
                .turn_view()
                .and_then(|v| v.confirm)
                .is_some_and(|c| c.tool == tool)
        },
    )
    .await;
    let confirm = rig.core.turn_view().unwrap().confirm.unwrap();
    rig.engine
        .answer_confirmation(&confirm.call_id, allow, false)
        .await
        .unwrap();
    until("the card to clear", Duration::from_secs(5), || {
        rig.core
            .turn_view()
            .and_then(|v| v.confirm)
            .is_none_or(|c| c.call_id != confirm.call_id)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn computer_use_watches_each_step_then_runs_under_the_tasks_grant() {
    let (rig, worker_task, pump) = scripted::rig(worker(), Vec::new(), PathBuf::from("no-model"));
    setup(&rig, ApproveSteps::FirstTasks, 25);
    let brain = Arc::new(ScriptedBrain::new(
        "anthropic",
        PrivacyClass::Cloud,
        vec![
            Script::tool(
                "computer__use",
                json!({ "task": "type hello in the document", "app": "notepad" }),
            ),
            Script::text("I typed hello in Notepad."),
        ],
    ));
    rig.brains.insert(brain.clone());
    let vision = ScriptedVision::new(vec![
        CuAction::Click {
            x: 1,
            y: 2,
            button: "left".into(),
            count: 1,
        },
        CuAction::Scroll {
            x: 8,
            y: 4,
            lines: -3,
        },
        CuAction::Type {
            text: "hello".into(),
        },
        CuAction::Done {
            summary: "Typed hello.".into(),
        },
    ]);
    rig.brains
        .set_computer(Some(vision.clone() as Arc<dyn ComputerUseProvider>));

    rig.engine
        .say("operate the computer: click into Notepad and type hello in the document")
        .await
        .unwrap();
    // The task itself: High risk, with what it may cost at most.
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    while !rig
        .core
        .turn_view()
        .and_then(|v| v.confirm)
        .is_some_and(|c| c.tool == "computer.use")
    {
        assert!(
            std::time::Instant::now() < deadline,
            "no task card: {:#?}
Activity: {:?}",
            rig.core.turn_view(),
            rig.recorder
                .recent(None, 10)
                .iter()
                .map(|a| (&a.kind, &a.title, &a.detail))
                .collect::<Vec<_>>()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let task = rig.core.turn_view().unwrap().confirm.unwrap();
    assert!(
        task.action.contains("$0.25"),
        "the estimate is shown: {}",
        task.action
    );
    rig.engine
        .answer_confirmation(&task.call_id, true, false)
        .await
        .unwrap();

    // Watch mode: each step asks, with the target highlighted and the controller showing.
    until("the first step's card", Duration::from_secs(15), || {
        rig.core
            .turn_view()
            .and_then(|v| v.confirm)
            .is_some_and(|c| c.tool == "input.click")
    })
    .await;
    let view = rig.core.turn_view().unwrap();
    let point = view.point.expect("the target is highlighted");
    assert_eq!((point.x + 14, point.y + 14), (1_100, 700));
    let controlling = rig
        .core
        .state()
        .borrow()
        .controlling
        .clone()
        .expect("the controller");
    assert_eq!((controlling.step, controlling.max_steps), (1, 25));
    assert!(view.confirm.unwrap().watch, "a watch-mode card");
    confirm_next(&rig, "input.click", true).await;
    // The scroll is skipped: it doesn't run, and the task goes on.
    confirm_next(&rig, "input.scroll", false).await;
    confirm_next(&rig, "input.type", true).await;
    until("the answer", Duration::from_secs(20), || {
        rig.core.state().borrow().session == SessionState::Idle
            && rig.core.turn_view().and_then(|v| v.answer).as_deref()
                == Some("I typed hello in Notepad.")
    })
    .await;
    let actions = rig.input.actions.lock().unwrap().clone();
    assert!(
        matches!(&actions[..], [InputAction::Click(at, _, 1), InputAction::Type(text)]
            if (at.x, at.y) == (1_100, 700) && text == "hello"),
        "{actions:?}"
    );
    assert_eq!(
        vision.shots.lock().unwrap().len(),
        4,
        "a screenshot before every step"
    );
    assert!(
        rig.core.state().borrow().controlling.is_none(),
        "the controller is gone"
    );
    assert!(
        rig.core.turn_view().unwrap().point.is_none(),
        "the highlight is gone"
    );
    let activity = rig.recorder.recent(None, 30);
    assert!(
        activity
            .iter()
            .any(|a| a.kind == "computer" && a.status == "done"),
        "{:?}",
        activity
            .iter()
            .map(|a| (&a.kind, &a.title))
            .collect::<Vec<_>>()
    );
    // The brain was told how it went.
    let requests = brain.requests.lock().unwrap().clone();
    let told = serde_json::to_string(&requests.last().unwrap().messages).unwrap();
    assert!(told.contains("Typed hello."), "{told}");

    // Approval "Never": the task's grant covers its steps, on that window only; the step limit
    // ends a task that doesn't finish.
    rig.input.actions.lock().unwrap().clear();
    setup(&rig, ApproveSteps::Never, 2);
    rig.windows.windows.lock().unwrap().truncate(1);
    let brain = Arc::new(ScriptedBrain::new(
        "anthropic",
        PrivacyClass::Cloud,
        vec![
            Script::tool(
                "computer__use",
                json!({ "task": "keep clicking", "app": "notepad" }),
            ),
            Script::text("It didn't finish."),
        ],
    ));
    rig.brains.insert(brain.clone());
    rig.brains.set_computer(Some(
        ScriptedVision::new(Vec::new()) as Arc<dyn ComputerUseProvider>
    ));
    rig.engine
        .say("operate the computer: keep clicking in Notepad")
        .await
        .unwrap();
    confirm_next(&rig, "computer.use", true).await;
    until("the answer", Duration::from_secs(20), || {
        rig.core.state().borrow().session == SessionState::Idle
            && rig.core.turn_view().and_then(|v| v.answer).as_deref() == Some("It didn't finish.")
    })
    .await;
    assert_eq!(
        rig.input.actions.lock().unwrap().len(),
        2,
        "two steps, no cards"
    );
    let told =
        serde_json::to_string(&brain.requests.lock().unwrap().last().unwrap().messages).unwrap();
    assert!(told.contains("most steps"), "{told}");

    rig.core.quit();
    let _ = tokio::time::timeout(Duration::from_secs(5), worker_task).await;
    pump.abort();
}
