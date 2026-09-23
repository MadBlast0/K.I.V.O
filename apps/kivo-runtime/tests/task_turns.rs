//! M5, end to end: background tasks through the real turn engine, task manager and permission
//! engine, on fakes — the build watcher with no brain call while it waits (PLAN-22, M5-X2),
//! reminders, task grants (SEC-09), concurrent steps, error policies (ROUT-04), timeouts and the
//! emergency stop (PLAN-03, SEC-27), validation before "done" (BRAIN-31), a crash's interrupted
//! tasks (ARCH-27) and what KIVO says without being asked (UX-40).

#![cfg(windows)]

use kivo_brain::PrivacyClass;
use kivo_brain::testing::ScriptedBrain;
use kivo_core::task::{
    Criterion, Notify, OnError, StepAction, TaskGrant, TaskKind, TaskSpec, TaskStatus, TaskStep,
};
use kivo_core::{Capability, SessionState};
use kivo_ipc::protocol::TaskView;
use kivo_runtime::scripted::{self, Rig};
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

    fn task(&self, id: &str) -> TaskView {
        self.rig.tasks.view(id).expect("the task is stored")
    }

    async fn until_status(&self, id: &str, status: TaskStatus) -> TaskView {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let t = self.task(id);
            if t.status == status {
                return t;
            }
            assert!(
                Instant::now() < deadline,
                "task never became {status:?}: {t:#?}"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

fn start() -> Running {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(std::env::var("KIVO_TEST_LOG").unwrap_or_else(|_| "warn".into()))
        .with_test_writer()
        .try_init();
    let (rig, worker, pump) = scripted::rig(worker(), Vec::new(), PathBuf::from("no-model"));
    rig.core.update_config(|c| {
        c.capabilities.set(Capability::SpeakResponses, false);
        c.voice.speak_typed_replies = false;
    });
    Running { rig, worker, pump }
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

fn step(id: &str, action: StepAction) -> TaskStep {
    TaskStep {
        id: id.into(),
        title: id.into(),
        action,
        depends_on: Vec::new(),
        on_error: OnError::Stop,
        delay_ms: None,
        confirm: false,
    }
}

fn spec(title: &str, steps: Vec<TaskStep>) -> TaskSpec {
    TaskSpec {
        title: title.into(),
        kind: TaskKind::Plan,
        owner: "you".into(),
        steps,
        success: Vec::new(),
        grants: Vec::new(),
        timeout_ms: None,
        notify: Notify::Silent,
        routine_id: None,
        turn_id: None,
        cwd: None,
    }
}

fn volume(level: u64) -> StepAction {
    StepAction::Tool {
        tool: "audio.volume_set".into(),
        args: json!({ "number": level }),
    }
}

fn volume_grant(level: u64) -> TaskGrant {
    TaskGrant {
        tool: "audio.volume_set".into(),
        args: json!({ "number": level }),
    }
}

/// PLAN-22, M5-X2: "tell me when the build finishes" finds the running build, waits on the
/// process with no brain call at all while waiting, and tells the user when it ends.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_build_watcher_waits_without_ai_and_tells_you() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start();
    // A brain is connected: it must not be asked anything while the watcher waits.
    let brain = Arc::new(ScriptedBrain::new("anthropic", PrivacyClass::Cloud, vec![]));
    r.rig.brains.insert(brain.clone());
    r.rig.processes.start(4242, "cargo.exe");
    r.rig
        .engine
        .say("tell me when the build finishes")
        .await
        .unwrap();
    let view = answered(&r.rig).await;
    assert!(
        view.answer.as_deref().unwrap_or_default().contains("cargo"),
        "{view:?}"
    );
    let tasks = r.rig.tasks.list(false, 10);
    assert_eq!(tasks.len(), 1, "one watcher task");
    let id = tasks[0].id.clone();
    r.until_status(&id, TaskStatus::Waiting).await;
    assert_eq!(
        r.rig.core.state().borrow().tasks_active,
        1,
        "counted for the tray"
    );
    // Waiting: measured over a second, no brain request and no CPU-bound work.
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(
        brain.requests.lock().unwrap().is_empty(),
        "no brain while waiting"
    );
    assert_eq!(*r.rig.processes.watched.lock().unwrap(), [4242]);
    r.rig.processes.exit(4242, 0);
    let done = r.until_status(&id, TaskStatus::Done).await;
    assert_eq!(done.result.as_deref(), Some("cargo finished."));
    assert!(brain.requests.lock().unwrap().is_empty(), "nor after");
    // Told: the Island's notice (no turn was running).
    until("the notice", Duration::from_secs(5), || {
        r.rig
            .core
            .turn_view()
            .is_some_and(|t| t.answer.as_deref() == Some("cargo finished."))
    })
    .await;
    assert_eq!(r.rig.core.state().borrow().tasks_active, 0);
    r.stop().await;
}

/// Grammar reminders become time watchers; nothing to watch is said plainly.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reminders_and_missing_builds() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start();
    let before = kivo_store::brains::now_ms();
    r.rig
        .engine
        .say("remind me in 20 minutes to stretch")
        .await
        .unwrap();
    let view = answered(&r.rig).await;
    assert!(view.answer.unwrap().starts_with("OK, I’ll remind you at"));
    let t = &r.rig.tasks.list(false, 10)[0];
    assert_eq!(t.kind, TaskKind::Reminder);
    assert_eq!(t.title, "stretch");
    let waiting = r.until_status(&t.id, TaskStatus::Waiting).await;
    assert!(
        r.rig
            .core
            .state()
            .borrow()
            .activities
            .iter()
            .any(|a| a.kind == "timer"
                && a.until.is_some_and(|u| {
                    let u = i64::try_from(u).unwrap();
                    u >= before + 20 * 60_000 && u < before + 21 * 60_000
                })),
        "a timer live activity counting down to it"
    );
    r.rig
        .tasks
        .cancel(&waiting.id, kivo_core::event::CancelReason::UserButton)
        .unwrap();
    r.until_status(&waiting.id, TaskStatus::Cancelled).await;
    assert!(r.rig.core.state().borrow().activities.is_empty());

    r.rig.core.clear_turn();
    r.rig
        .engine
        .say("tell me when the build finishes")
        .await
        .unwrap();
    let view = answered(&r.rig).await;
    assert_eq!(view.error.as_deref(), Some("I don’t see a build running."));
    r.stop().await;
}

/// SEC-09: a task runs exactly the calls it was granted at creation, in every mode — not asked,
/// refused otherwise; independent steps run side by side.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tasks_keep_to_their_grants_and_run_independent_steps_together() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start();
    r.rig
        .core
        .set_mode(kivo_core::config::PermissionMode::Ask)
        .unwrap();
    let mut granted = spec("Quiet", vec![step("v", volume(20))]);
    granted.grants = vec![volume_grant(20)];
    let id = r.rig.tasks.create(granted).unwrap();
    r.until_status(&id, TaskStatus::Done).await;
    assert!((r.rig.control.volume.lock().unwrap().level - 0.2).abs() < 0.01);

    // Granted 20, asked for 90: refused, never asked.
    let mut other = spec("Loud", vec![step("v", volume(90))]);
    other.grants = vec![volume_grant(20)];
    let id = r.rig.tasks.create(other).unwrap();
    let failed = r.until_status(&id, TaskStatus::Failed).await;
    assert!(
        failed.error.unwrap().contains("wasn’t given permission"),
        "refused by the grant"
    );
    assert!((r.rig.control.volume.lock().unwrap().level - 0.2).abs() < 0.01);
    assert!(r.rig.core.turn_view().is_none_or(|t| t.confirm.is_none()));

    // Two half-second waits side by side finish in about half a second.
    let started = Instant::now();
    let id = r
        .rig
        .tasks
        .create(spec(
            "Together",
            vec![
                step("a", StepAction::Delay { ms: 500 }),
                step("b", StepAction::Delay { ms: 500 }),
            ],
        ))
        .unwrap();
    r.until_status(&id, TaskStatus::Done).await;
    assert!(
        started.elapsed() < Duration::from_millis(900),
        "{:?}",
        started.elapsed()
    );
    r.stop().await;
}

/// ROUT-04: retry, continue and ask.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn error_policies_retry_continue_and_ask() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start();
    let missing_app = || StepAction::Tool {
        tool: "apps.launch".into(),
        args: json!({ "app": { "id": "Nope", "name": "Nope" } }),
    };
    let grant = TaskGrant {
        tool: "apps.launch".into(),
        args: json!({ "app": { "id": "Nope", "name": "Nope" } }),
    };
    // Retry twice, then stop: three attempts.
    let mut retry = spec("Retry", vec![step("a", missing_app())]);
    retry.steps[0].on_error = OnError::Retry { times: 2 };
    retry.grants = vec![grant.clone()];
    let id = r.rig.tasks.create(retry).unwrap();
    let t = r.until_status(&id, TaskStatus::Failed).await;
    assert_eq!(t.steps[0].attempts, 3);

    // Continue: the next step runs anyway.
    let mut cont = spec(
        "Continue",
        vec![step("a", missing_app()), step("b", volume(35))],
    )
    .chain();
    cont.steps[0].on_error = OnError::Continue;
    cont.grants = vec![grant.clone(), volume_grant(35)];
    let id = r.rig.tasks.create(cont).unwrap();
    let t = r.until_status(&id, TaskStatus::Done).await;
    assert_eq!(t.steps[0].status, "failed");
    assert_eq!(t.steps[1].status, "done");

    // Ask: the task waits for the user, who skips the step.
    let mut ask = spec("Ask", vec![step("a", missing_app()), step("b", volume(45))]).chain();
    ask.steps[0].on_error = OnError::Ask;
    ask.grants = vec![grant, volume_grant(45)];
    let id = r.rig.tasks.create(ask).unwrap();
    let t = r.until_status(&id, TaskStatus::NeedsYou).await;
    let q = t.question.expect("a question");
    assert_eq!(q.choices, ["retry", "skip", "stop"]);
    assert!(
        r.rig.tasks.answer(&id, "maybe").is_err(),
        "only its choices"
    );
    r.rig.tasks.answer(&id, "skip").unwrap();
    r.until_status(&id, TaskStatus::Done).await;
    assert!((r.rig.control.volume.lock().unwrap().level - 0.45).abs() < 0.01);
    r.stop().await;
}

/// PLAN-03 and SEC-27: a timeout cancels the task; the emergency stop pauses tasks, which only
/// the user resumes; BRAIN-31: "done" only after the check passes.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn timeouts_pauses_and_checks() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start();
    let mut slow = spec("Slow", vec![step("a", StepAction::Delay { ms: 5_000 })]);
    slow.timeout_ms = Some(200);
    let id = r.rig.tasks.create(slow).unwrap();
    let t = r.until_status(&id, TaskStatus::Failed).await;
    assert_eq!(
        t.error.as_deref(),
        Some("It took too long, so KIVO stopped it.")
    );

    // The emergency stop pauses; resume carries on from the unfinished step.
    let mut two = spec(
        "Two",
        vec![
            step("a", volume(10)),
            step("b", StepAction::Delay { ms: 30_000 }),
            step("c", volume(60)),
        ],
    )
    .chain();
    two.grants = vec![volume_grant(10), volume_grant(60)];
    let id = r.rig.tasks.create(two).unwrap();
    until("the wait to start", Duration::from_secs(5), || {
        r.task(&id).steps[1].status == "running"
    })
    .await;
    r.rig.engine.stop_everything();
    let paused = r.until_status(&id, TaskStatus::Paused).await;
    assert_eq!(paused.steps[0].status, "done");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        r.task(&id).status,
        TaskStatus::Paused,
        "never resumes by itself"
    );
    // Resumed without the long wait this time: edit the stored spec? No — the user resumes and
    // then cancels the wait; what matters is that finished steps don't run again.
    r.rig.control.volume.lock().unwrap().level = 0.99;
    r.rig.tasks.resume(&id).unwrap();
    until("running again", Duration::from_secs(5), || {
        r.task(&id).status == TaskStatus::Running
    })
    .await;
    assert!(
        (r.rig.control.volume.lock().unwrap().level - 0.99).abs() < 0.01,
        "the done step didn't run twice"
    );
    r.rig
        .tasks
        .cancel(&id, kivo_core::event::CancelReason::UserButton)
        .unwrap();
    r.until_status(&id, TaskStatus::Cancelled).await;

    // Validation: the check fails, so it isn't "done"; then it passes.
    r.rig.core.update_config(|c| {
        c.capabilities.set(Capability::Shell, true);
        c.tools.shell_read_only = false;
    });
    let checked = || {
        let mut s = spec("Fix", vec![step("a", volume(50))]);
        s.success = vec![Criterion::CommandSucceeds {
            command: "cargo test".into(),
            cwd: None,
        }];
        s.grants = vec![
            volume_grant(50),
            TaskGrant {
                tool: "shell.run".into(),
                args: json!({ "command": "cargo test" }),
            },
        ];
        s
    };
    r.rig.commands.reply.lock().unwrap().exit_code = Some(101);
    r.rig.commands.reply.lock().unwrap().stderr = "test result: FAILED. 1 failed".into();
    let id = r.rig.tasks.create(checked()).unwrap();
    let t = r.until_status(&id, TaskStatus::Failed).await;
    let e = t.error.unwrap();
    assert!(
        e.starts_with("The work finished, but the check didn’t pass"),
        "{e}"
    );
    assert!(e.contains("1 failed"), "says what failed: {e}");
    r.rig.commands.reply.lock().unwrap().exit_code = Some(0);
    let id = r.rig.tasks.create(checked()).unwrap();
    r.until_status(&id, TaskStatus::Done).await;
    assert_eq!(r.rig.commands.ran.lock().unwrap().len(), 2);
    r.stop().await;
}

/// ARCH-27: after a crash, running tasks are reported as interrupted and never resumed on their
/// own; a watcher that was waiting is armed again.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_crash_leaves_interrupted_tasks_and_rearmed_watchers() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start();
    let db = r.rig.recorder.database();
    let running_spec =
        serde_json::to_string(&spec("Half done", vec![step("a", volume(70))])).unwrap();
    let mut watch = spec(
        "Wait for cargo",
        vec![step(
            "w",
            StepAction::Watch {
                watch: kivo_core::task::WatchSpec::ProcessExit {
                    pid: Some(77),
                    name: Some("cargo".into()),
                },
            },
        )],
    );
    watch.kind = TaskKind::Watch;
    let watch_spec = serde_json::to_string(&watch).unwrap();
    r.rig.processes.start(77, "cargo.exe");
    {
        let db = db.lock().unwrap();
        for (id, title, status, body) in [
            ("t-run", "Half done", "running", running_spec.as_str()),
            ("t-wait", "Wait for cargo", "waiting", watch_spec.as_str()),
        ] {
            db.insert_task(&kivo_store::tasks::NewTask {
                id,
                title,
                kind: "plan",
                owner: "you",
                status,
                spec: body,
                routine_id: None,
                turn_id: None,
                steps: &[("a".into(), "a".into()), ("w".into(), "w".into())],
            })
            .unwrap();
        }
    }
    let interrupted = r.rig.tasks.startup().await;
    assert_eq!(interrupted.len(), 1);
    assert_eq!(r.task("t-run").status, TaskStatus::Interrupted);
    assert!(
        (r.rig.control.volume.lock().unwrap().level - 0.7).abs() > 0.01,
        "an interrupted task's step is not run again on its own"
    );
    r.until_status("t-wait", TaskStatus::Waiting).await;
    r.rig.processes.exit(77, 0);
    r.until_status("t-wait", TaskStatus::Done).await;
    r.stop().await;
}

/// UX-40: in a call nothing is spoken; it waits for "what did I miss?".
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn in_a_call_news_waits_for_what_did_i_miss() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start();
    r.rig.system.presence.lock().unwrap().mic_in_use_elsewhere = true;
    let mut watch = spec(
        "The build",
        vec![step(
            "w",
            StepAction::Watch {
                watch: kivo_core::task::WatchSpec::ProcessExit {
                    pid: Some(9),
                    name: Some("msbuild".into()),
                },
            },
        )],
    );
    watch.notify = Notify::Speak;
    r.rig.processes.start(9, "msbuild.exe");
    let id = r.rig.tasks.create(watch).unwrap();
    r.until_status(&id, TaskStatus::Waiting).await;
    r.rig.processes.exit(9, 1);
    r.until_status(&id, TaskStatus::Done).await;
    until("a quiet toast", Duration::from_secs(5), || {
        !r.rig.notifications.shown.lock().unwrap().is_empty()
    })
    .await;
    assert!(
        r.rig.core.turn_view().is_none(),
        "nothing shown over the call"
    );
    assert_eq!(r.rig.tasks.notifier().missed_count(), 1);
    r.rig.system.presence.lock().unwrap().mic_in_use_elsewhere = false;
    r.rig.engine.say("what did I miss").await.unwrap();
    let view = answered(&r.rig).await;
    assert_eq!(
        view.answer.as_deref(),
        Some("While you were away: msbuild finished with errors (exit code 1).")
    );
    assert_eq!(r.rig.tasks.notifier().missed_count(), 0);
    r.stop().await;
}
