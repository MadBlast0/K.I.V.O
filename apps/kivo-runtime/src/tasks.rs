//! Background tasks (ARCHITECTURE §4.2, BRAINS §7, TOOLS_AND_CONTROL §6, SECURITY §2).
//!
//! A task is a graph of steps with its own cancellation that outlives turns: a plan a brain
//! proposed, a watcher, a reminder, a routine's run, work handed to a CLI agent. Steps whose
//! dependencies are done run concurrently. Every tool step goes through the permission engine as
//! `Initiator::Task` with the grants the user approved when the task was created, and nothing
//! beyond them (SEC-09). A task is persisted as it runs: after a crash it is reported as
//! interrupted, never resumed on its own; waiting watchers are armed again, since waiting has no
//! side effect (ARCH-27). The emergency stop pauses tasks, and only the user resumes them
//! (SECURITY §8). A timeout, a provider failure under the "stop" policy and quitting all cancel
//! every running step (PLAN-03). A task that declares success criteria reports "done" only after
//! they pass (BRAIN-31).

use crate::activity::{Recorder, task_key};
use crate::agents::Agents;
use crate::brains::{Brains, Meter};
use crate::core::Core;
use crate::notifier::{Announcement, Notifier};
use crate::watchers::Watchers;
use kivo_brain::acp::{AgentEvent, PermissionAnswer, PermissionAsk, PermissionHandler, pick};
use kivo_brain::routing::Route;
use kivo_brain::{ChatRequest, Message, PrivacyClass, ProviderKind, SystemBlock};
use kivo_core::event::{CancelReason, EventKind, TaskEvent};
use kivo_core::task::{
    Criterion, Notify, OnError, StepAction, TaskKind, TaskSpec, TaskStatus, TaskStep,
};
use kivo_core::text;
use kivo_core::tool::{Initiator, ToolCall};
use kivo_core::{Event, TaskId};
use kivo_ipc::protocol::{LiveActivity, TaskQuestion, TaskStepView, TaskView};
use kivo_security::{Context, Decision, Grant, HardLimits, SessionKind, Taint};
use kivo_store::Database;
use kivo_store::tasks::{NewTask, StepUpdate, StoredTask};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

/// Finished tasks keep their steps this long, then only a summary (MEM-02).
const DETAIL_KEPT_MS: i64 = kivo_store::tasks::TASK_DETAIL_KEPT_MS;
/// The most words of a step's result kept for the Tasks page.
const RESULT_CHARS: usize = 600;

/// How a task stopped short.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ending {
    Cancelled(CancelReason),
    /// The emergency stop: paused, resumed only by the user.
    Paused,
    /// KIVO is quitting: running steps are interrupted, watchers keep waiting for next time.
    Shutdown,
    TimedOut,
}

/// A running task.
struct Live {
    cancel: CancellationToken,
    ending: Arc<Mutex<Option<Ending>>>,
    /// A question waiting for the user, and where the answer goes.
    question: Option<(TaskQuestion, oneshot::Sender<String>)>,
    title: String,
}

/// What a step produced.
#[derive(Default)]
struct StepDone {
    detail: String,
    /// Text a later step or the final report uses (an answer, a decision).
    result: Option<String>,
    /// The user said no to it (a step that asks first).
    skipped: bool,
}

pub struct TaskParts {
    pub core: Arc<Core>,
    pub recorder: Recorder,
    pub registry: Arc<kivo_tools::Registry>,
    pub watchers: Arc<Watchers>,
    pub notifier: Arc<Notifier>,
    pub brains: Arc<Brains>,
    pub agents: Arc<Agents>,
    pub windows: Arc<dyn kivo_platform::Windows>,
}

pub struct Tasks {
    core: Arc<Core>,
    recorder: Recorder,
    db: Arc<Mutex<Database>>,
    registry: Arc<kivo_tools::Registry>,
    watchers: Arc<Watchers>,
    notifier: Arc<Notifier>,
    brains: Arc<Brains>,
    agents: Arc<Agents>,
    windows: Arc<dyn kivo_platform::Windows>,
    live: Mutex<HashMap<String, Live>>,
    me: Mutex<Weak<Tasks>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn clip(s: &str) -> String {
    let mut out: String = s.chars().take(RESULT_CHARS).collect();
    if out.len() < s.len() {
        out.push('…');
    }
    out
}

fn kind_str(kind: TaskKind) -> &'static str {
    match kind {
        TaskKind::Plan => "plan",
        TaskKind::Watch => "watch",
        TaskKind::Reminder => "reminder",
        TaskKind::Coding => "coding",
        TaskKind::Routine => "routine",
    }
}

fn kind_of(s: &str) -> TaskKind {
    match s {
        "watch" => TaskKind::Watch,
        "reminder" => TaskKind::Reminder,
        "coding" => TaskKind::Coding,
        "routine" => TaskKind::Routine,
        _ => TaskKind::Plan,
    }
}

/// Which notification source a kind of task speaks as (UX-40 per-source settings).
fn source_of(kind: TaskKind) -> &'static str {
    match kind {
        TaskKind::Watch => "watchers",
        TaskKind::Reminder => "reminders",
        TaskKind::Routine => "routines",
        TaskKind::Coding => "agents",
        TaskKind::Plan => "tasks",
    }
}

impl Tasks {
    pub fn new(parts: TaskParts) -> Arc<Self> {
        let db = parts.recorder.database();
        let tasks = Arc::new(Self {
            core: parts.core,
            recorder: parts.recorder,
            db,
            registry: parts.registry,
            watchers: parts.watchers,
            notifier: parts.notifier,
            brains: parts.brains,
            agents: parts.agents,
            windows: parts.windows,
            live: Mutex::default(),
            me: Mutex::new(Weak::new()),
        });
        *lock(&tasks.me) = Arc::downgrade(&tasks);
        tasks
    }

    fn me(&self) -> Option<Arc<Self>> {
        lock(&self.me).upgrade()
    }

    pub fn registry(&self) -> &Arc<kivo_tools::Registry> {
        &self.registry
    }

    pub fn watchers(&self) -> &Arc<Watchers> {
        &self.watchers
    }

    pub fn notifier(&self) -> &Arc<Notifier> {
        &self.notifier
    }

    fn db<T>(&self, f: impl FnOnce(&Database) -> Result<T, kivo_store::db::DbError>) -> Option<T> {
        match f(&lock(&self.db)) {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!(%e, "task store");
                None
            }
        }
    }

    fn publish(&self, id: &str, event: TaskEvent) {
        let task = id.parse::<TaskId>().ok();
        let mut e = Event::new(EventKind::Task(event));
        if let Some(t) = task {
            e = e.in_task(t);
        }
        self.core.bus.publish(e);
    }

    fn refresh_count(&self) {
        let count = self
            .db(|db| db.tasks(false, 500))
            .unwrap_or_default()
            .iter()
            .filter(|t| TaskStatus::parse(&t.status).is_some_and(TaskStatus::is_active))
            .count();
        self.core
            .set_tasks_active(u32::try_from(count).unwrap_or(u32::MAX));
    }

    fn set_status(&self, id: &str, status: TaskStatus, result: Option<&str>, error: Option<&str>) {
        self.db(|db| db.set_task_status(id, status.as_str(), result, error));
        self.refresh_count();
    }

    fn set_step(&self, task: &str, step: &str, u: &StepUpdate<'_>) {
        self.db(|db| db.set_step(task, step, u));
        let status = match u.status {
            "running" | "waiting" => kivo_core::event::StepStatus::Running,
            "done" => kivo_core::event::StepStatus::Done,
            "failed" => kivo_core::event::StepStatus::Failed,
            "skipped" => kivo_core::event::StepStatus::Skipped,
            _ => kivo_core::event::StepStatus::Pending,
        };
        self.publish(
            task,
            TaskEvent::StepChanged {
                step: step.to_owned(),
                status,
            },
        );
    }

    /// Creates a task and starts it (SEC-09: `spec.grants` are what the user approved now).
    pub fn create(&self, spec: TaskSpec) -> Result<String, String> {
        spec.validate().map_err(|e| e.to_string())?;
        let id = TaskId::new().to_string();
        let steps: Vec<(String, String)> = spec
            .steps
            .iter()
            .map(|s| (s.id.clone(), self.step_title(s)))
            .collect();
        let body = serde_json::to_string(&spec).map_err(|e| e.to_string())?;
        self.db(|db| {
            db.insert_task(&NewTask {
                id: &id,
                title: &spec.title,
                kind: kind_str(spec.kind),
                owner: &spec.owner,
                status: TaskStatus::Pending.as_str(),
                spec: &body,
                routine_id: spec.routine_id.as_deref(),
                turn_id: spec.turn_id.as_deref(),
                steps: &steps,
            })
        })
        .ok_or_else(|| text::t("task.couldntSave"))?;
        self.recorder
            .task_activity(&id, &spec.title, "started", None);
        self.publish(
            &id,
            TaskEvent::Created {
                name: spec.title.clone(),
            },
        );
        self.start(id.clone(), spec, HashSet::new());
        Ok(id)
    }

    /// A step's title in the user's words: the tool's own title with its arguments.
    fn step_title(&self, step: &TaskStep) -> String {
        match &step.action {
            StepAction::Tool { tool, args } => self.registry.known(tool).map_or_else(
                || step.title.clone(),
                |spec| kivo_security::render_title(&spec.title, args),
            ),
            StepAction::Watch { watch } => watch_title(watch),
            StepAction::Verify { criterion } => criterion_title(criterion),
            StepAction::Say { text: t } => text::tf("task.step.say", &[("text", t)]),
            StepAction::Delay { ms } => {
                text::tf("task.step.delay", &[("seconds", &(ms / 1000).to_string())])
            }
            _ => step.title.clone(),
        }
    }

    fn start(&self, id: String, spec: TaskSpec, finished: HashSet<String>) {
        let Some(me) = self.me() else { return };
        let cancel = self.core.shutdown().child_token();
        let ending = Arc::new(Mutex::new(None));
        lock(&self.live).insert(
            id.clone(),
            Live {
                cancel: cancel.clone(),
                ending: Arc::clone(&ending),
                question: None,
                title: spec.title.clone(),
            },
        );
        tokio::spawn(async move {
            me.run(id, spec, finished, cancel, ending).await;
        });
    }

    async fn run(
        self: Arc<Self>,
        id: String,
        spec: TaskSpec,
        mut finished: HashSet<String>,
        cancel: CancellationToken,
        ending: Arc<Mutex<Option<Ending>>>,
    ) {
        let outcome = match spec.timeout_ms {
            Some(ms) => {
                let graph = self.run_graph(&id, &spec, &mut finished, &cancel);
                tokio::select! {
                    r = graph => r,
                    () = tokio::time::sleep(Duration::from_millis(ms)) => {
                        *lock(&ending) = Some(Ending::TimedOut);
                        cancel.cancel();
                        Err(text::t("task.timedOut"))
                    }
                }
            }
            None => self.run_graph(&id, &spec, &mut finished, &cancel).await,
        };
        // Quitting cancels every task through the shutdown token, before `shutdown` may run.
        let end = lock(&ending).or_else(|| {
            self.core
                .shutdown()
                .is_cancelled()
                .then_some(Ending::Shutdown)
        });
        lock(&self.live).remove(&id);
        self.core.remove_activity(&id);
        match (end, outcome.clone()) {
            (Some(Ending::Paused), _) => {
                self.set_status(&id, TaskStatus::Paused, None, None);
                self.recorder
                    .task_activity(&id, &spec.title, "paused", None);
            }
            (Some(Ending::Shutdown), _) => {
                // Watchers wait again next time; anything else was interrupted.
                let waiting = self
                    .db(|db| db.task(&id))
                    .flatten()
                    .is_some_and(|t| t.status == "waiting");
                if !waiting {
                    self.set_status(&id, TaskStatus::Interrupted, None, None);
                }
            }
            (Some(Ending::Cancelled(reason)), _) => {
                self.set_status(&id, TaskStatus::Cancelled, None, None);
                self.recorder
                    .task_activity(&id, &spec.title, "cancelled", None);
                self.publish(&id, TaskEvent::Cancelled { reason });
            }
            (Some(Ending::TimedOut), _) | (None, Err(_)) => {
                let message = match (end, outcome) {
                    (Some(Ending::TimedOut), _) => text::t("task.timedOut"),
                    (_, Err(e)) if !e.is_empty() => e,
                    _ => self
                        .last_error(&id)
                        .unwrap_or_else(|| text::t("task.failed")),
                };
                self.set_status(&id, TaskStatus::Failed, None, Some(&message));
                self.recorder
                    .task_activity(&id, &spec.title, "failed", Some(&message));
                self.publish(
                    &id,
                    TaskEvent::Failed {
                        message: message.clone(),
                    },
                );
                if spec.notify != Notify::Silent || spec.kind == TaskKind::Routine {
                    let said = text::tf(
                        "task.failedSay",
                        &[("title", &spec.title), ("reason", &message)],
                    );
                    self.notifier
                        .announce(Announcement {
                            source: source_of(spec.kind).to_owned(),
                            title: spec.title.clone(),
                            text: said,
                            urgent: false,
                            tell_me: spec.notify == Notify::Speak,
                            task: Some(id.clone()),
                        })
                        .await;
                }
            }
            (None, Ok(result)) => {
                self.set_status(&id, TaskStatus::Done, Some(&result), None);
                self.recorder
                    .task_activity(&id, &spec.title, "done", Some(&result));
                self.publish(&id, TaskEvent::Completed);
                if let Some(r) = &spec.routine_id {
                    self.db(|db| db.routine_ran(r));
                }
                if spec.notify != Notify::Silent {
                    self.notifier
                        .announce(Announcement {
                            source: source_of(spec.kind).to_owned(),
                            title: spec.title.clone(),
                            text: result,
                            urgent: spec.kind == TaskKind::Reminder,
                            tell_me: spec.notify == Notify::Speak,
                            task: Some(id.clone()),
                        })
                        .await;
                }
            }
        }
    }

    fn last_error(&self, id: &str) -> Option<String> {
        self.db(|db| db.task_steps(id))
            .unwrap_or_default()
            .into_iter()
            .rev()
            .find(|s| s.status == "failed")
            .and_then(|s| s.detail)
    }

    /// Runs the graph to its end; `Ok` with what to tell the user.
    async fn run_graph(
        self: &Arc<Self>,
        id: &str,
        spec: &TaskSpec,
        finished: &mut HashSet<String>,
        cancel: &CancellationToken,
    ) -> Result<String, String> {
        let grants: Arc<Vec<Grant>> = Arc::new(
            spec.grants
                .iter()
                .map(|g| Grant::exact(g.tool.clone(), g.args.clone()))
                .collect(),
        );
        let mut started: HashSet<String> = HashSet::new();
        let mut attempts: HashMap<String, u8> = HashMap::new();
        let mut results: Vec<String> = Vec::new();
        let mut running: JoinSet<(String, Result<StepDone, String>)> = JoinSet::new();
        let mut in_flight: HashMap<String, bool> = HashMap::new();
        loop {
            for step in spec.ready(finished, &started) {
                started.insert(step.id.clone());
                let attempt = attempts.entry(step.id.clone()).or_insert(0);
                *attempt += 1;
                let is_watch = matches!(step.action, StepAction::Watch { .. });
                in_flight.insert(step.id.clone(), is_watch);
                self.set_step(
                    id,
                    &step.id,
                    &StepUpdate {
                        status: if is_watch { "waiting" } else { "running" },
                        attempts: Some(i64::from(*attempt)),
                        started: true,
                        ..Default::default()
                    },
                );
                let (me, step, task, spec, grants, cancel) = (
                    Arc::clone(self),
                    step.clone(),
                    id.to_owned(),
                    spec.clone(),
                    Arc::clone(&grants),
                    cancel.child_token(),
                );
                running.spawn(async move {
                    if let Some(ms) = step.delay_ms {
                        tokio::select! {
                            () = tokio::time::sleep(Duration::from_millis(ms)) => {}
                            () = cancel.cancelled() => return (step.id.clone(), Err(text::t("reply.cancelled"))),
                        }
                    }
                    if step.confirm {
                        let question = text::tf("task.confirmStep", &[("step", &me.step_title(&step))]);
                        let answer = me
                            .ask_user(&task, &step.id, &question, &["allow", "deny"], &cancel)
                            .await;
                        if answer.as_deref() != Some("allow") {
                            let done = StepDone {
                                detail: text::t("task.skippedByYou"),
                                skipped: true,
                                ..StepDone::default()
                            };
                            return (step.id, Ok(done));
                        }
                    }
                    let r = me.run_step(&task, &spec, &step, &grants, &cancel).await;
                    (step.id, r)
                });
            }
            if in_flight.is_empty() {
                break;
            }
            // Waiting when every running step is a watcher: no brain, nothing to do but wait.
            let status = if in_flight.values().all(|w| *w) {
                TaskStatus::Waiting
            } else {
                TaskStatus::Running
            };
            self.set_status(id, status, None, None);
            if status == TaskStatus::Waiting {
                self.publish(
                    id,
                    TaskEvent::Waiting {
                        reason: spec.title.clone(),
                    },
                );
            }
            let joined = tokio::select! {
                j = running.join_next() => j,
                () = cancel.cancelled() => {
                    running.abort_all();
                    while running.join_next().await.is_some() {}
                    for step in in_flight.keys() {
                        self.set_step(id, step, &StepUpdate { status: "failed", detail: Some(&text::t("reply.cancelled")), finished: true, ..Default::default() });
                    }
                    return Err(text::t("reply.cancelled"));
                }
            };
            let Some(Ok((step_id, result))) = joined else {
                continue;
            };
            in_flight.remove(&step_id);
            let step = spec
                .steps
                .iter()
                .find(|s| s.id == step_id)
                .expect("a step of this task");
            match result {
                Ok(done) => {
                    self.set_step(
                        id,
                        &step_id,
                        &StepUpdate {
                            status: if done.skipped { "skipped" } else { "done" },
                            detail: Some(&clip(&done.detail)),
                            result: done.result.as_deref(),
                            finished: true,
                            ..Default::default()
                        },
                    );
                    if let Some(r) = done.result {
                        results.push(r);
                    } else if !done.detail.is_empty() {
                        results.push(done.detail);
                    }
                    finished.insert(step_id);
                }
                Err(message) => {
                    let tries = attempts.get(&step_id).copied().unwrap_or(1);
                    let decision = match step.on_error {
                        OnError::Retry { times } if tries <= times => "retry",
                        OnError::Continue => "skip",
                        OnError::Ask if !cancel.is_cancelled() => {
                            self.set_step(
                                id,
                                &step_id,
                                &StepUpdate {
                                    status: "needsYou",
                                    detail: Some(&message),
                                    ..Default::default()
                                },
                            );
                            let question = text::tf(
                                "task.askFailed",
                                &[("step", &self.step_title(step)), ("reason", &message)],
                            );
                            let answer = self
                                .ask_user(
                                    id,
                                    &step_id,
                                    &question,
                                    &["retry", "skip", "stop"],
                                    cancel,
                                )
                                .await;
                            match answer.as_deref() {
                                Some("retry") => "retry",
                                Some("skip") => "skip",
                                _ => "stop",
                            }
                        }
                        _ => "stop",
                    };
                    match decision {
                        "retry" => {
                            self.set_step(
                                id,
                                &step_id,
                                &StepUpdate {
                                    status: "pending",
                                    detail: Some(&message),
                                    ..Default::default()
                                },
                            );
                            started.remove(&step_id);
                        }
                        "skip" => {
                            self.set_step(
                                id,
                                &step_id,
                                &StepUpdate {
                                    status: "failed",
                                    detail: Some(&message),
                                    finished: true,
                                    ..Default::default()
                                },
                            );
                            finished.insert(step_id);
                        }
                        _ => {
                            self.set_step(
                                id,
                                &step_id,
                                &StepUpdate {
                                    status: "failed",
                                    detail: Some(&message),
                                    finished: true,
                                    ..Default::default()
                                },
                            );
                            // The rest stops with it (PLAN-03).
                            running.abort_all();
                            while running.join_next().await.is_some() {}
                            for other in in_flight.keys() {
                                self.set_step(
                                    id,
                                    other,
                                    &StepUpdate {
                                        status: "failed",
                                        detail: Some(&text::t("reply.cancelled")),
                                        finished: true,
                                        ..Default::default()
                                    },
                                );
                            }
                            return Err(message);
                        }
                    }
                }
            }
        }
        // BRAIN-31: "done" only once every success criterion passes.
        for criterion in &spec.success {
            if let Err(e) = self.verify(id, spec, criterion, &grants, cancel).await {
                return Err(text::tf("task.notVerified", &[("check", &e)]));
            }
        }
        let summary = results
            .last()
            .cloned()
            .unwrap_or_else(|| text::tf("task.done", &[("title", &spec.title)]));
        Ok(summary)
    }

    async fn run_step(
        self: &Arc<Self>,
        task: &str,
        spec: &TaskSpec,
        step: &TaskStep,
        grants: &[Grant],
        cancel: &CancellationToken,
    ) -> Result<StepDone, String> {
        match &step.action {
            StepAction::Tool { tool, args } => {
                let say = self
                    .run_tool(task, &step.id, tool, args.clone(), grants, cancel)
                    .await?;
                Ok(StepDone {
                    detail: say,
                    result: None,
                    skipped: false,
                })
            }
            StepAction::Watch { watch } => {
                self.show_watch(task, spec, watch);
                let fired = self.watchers.wait(watch, cancel).await;
                self.core.remove_activity(task);
                let fired = fired?;
                let detail = if fired.late {
                    text::tf("task.missedWhileOff", &[("what", &fired.what)])
                } else {
                    fired.what
                };
                Ok(StepDone {
                    detail: detail.clone(),
                    result: Some(detail),
                    skipped: false,
                })
            }
            StepAction::Say { text: t } => {
                // A quiet task (a routine) says it now; the others say their result at the end.
                if spec.notify == Notify::Silent {
                    self.notifier
                        .announce(Announcement {
                            source: source_of(spec.kind).to_owned(),
                            title: spec.title.clone(),
                            text: t.clone(),
                            urgent: false,
                            tell_me: true,
                            task: Some(task.to_owned()),
                        })
                        .await;
                }
                Ok(StepDone {
                    detail: t.clone(),
                    result: Some(t.clone()),
                    skipped: false,
                })
            }
            StepAction::Delay { ms } => {
                tokio::select! {
                    () = tokio::time::sleep(Duration::from_millis(*ms)) => Ok(StepDone::default()),
                    () = cancel.cancelled() => Err(text::t("reply.cancelled")),
                }
            }
            StepAction::Ask { prompt, profile } => {
                let answer = self
                    .ask_brain(task, spec, prompt, profile.as_deref(), cancel)
                    .await?;
                Ok(StepDone {
                    detail: answer.clone(),
                    result: Some(answer),
                    skipped: false,
                })
            }
            StepAction::Decide {
                question,
                options,
                profile,
            } => {
                let prompt = text::tf(
                    "task.decidePrompt",
                    &[("question", question), ("options", &options.join(" | "))],
                );
                let answer = self
                    .ask_brain(task, spec, &prompt, profile.as_deref(), cancel)
                    .await?;
                let lower = answer.to_lowercase();
                let chosen = options
                    .iter()
                    .find(|o| lower.trim() == o.to_lowercase())
                    .or_else(|| options.iter().find(|o| lower.contains(&o.to_lowercase())))
                    .ok_or_else(|| text::tf("task.noDecision", &[("answer", &clip(&answer))]))?;
                Ok(StepDone {
                    detail: chosen.clone(),
                    result: Some(chosen.clone()),
                    skipped: false,
                })
            }
            StepAction::Agent { prompt, agent, cwd } => {
                let cwd = cwd.clone().or_else(|| spec.cwd.clone());
                self.run_agent(task, prompt, agent.as_deref(), cwd.as_deref(), cancel)
                    .await
                    .map(|summary| StepDone {
                        detail: summary.clone(),
                        result: Some(summary),
                        skipped: false,
                    })
            }
            StepAction::Verify { criterion } => {
                let detail = self.verify(task, spec, criterion, grants, cancel).await?;
                Ok(StepDone {
                    detail,
                    ..StepDone::default()
                })
            }
        }
    }

    /// A watcher's live activity in the Island (UX-15).
    fn show_watch(&self, task: &str, spec: &TaskSpec, watch: &kivo_core::task::WatchSpec) {
        let config = self.core.config().automation.live_activities;
        let (kind, until) = match watch {
            kivo_core::task::WatchSpec::Time { at_ms } => ("timer", u64::try_from(*at_ms).ok()),
            kivo_core::task::WatchSpec::Download { .. } => ("download", None),
            _ => ("task", None),
        };
        let shown = match kind {
            "timer" => config.timer,
            "download" => config.download,
            _ => true,
        };
        if shown {
            self.core.set_activity(LiveActivity {
                id: task.to_owned(),
                kind: kind.to_owned(),
                title: spec.title.clone(),
                detail: Some(watch_title(watch)),
                progress: None,
                until,
                task_id: Some(task.to_owned()),
            });
        }
    }

    /// One tool call with the task's grants (SEC-09).
    async fn run_tool(
        &self,
        task: &str,
        step: &str,
        tool_id: &str,
        args: serde_json::Value,
        grants: &[Grant],
        cancel: &CancellationToken,
    ) -> Result<String, String> {
        let config = self.core.config();
        let key = task_key(task);
        let mut call = ToolCall {
            id: format!("{task}-{step}"),
            tool: tool_id.to_owned(),
            args,
            initiated_by: Initiator::Task,
            targets: Vec::new(),
        };
        let Some(tool) = self.registry.get(tool_id, &config.capabilities) else {
            let message = self.registry.known(tool_id).map_or_else(
                || text::tf("task.unknownTool", &[("tool", &tool_id)]),
                |spec| {
                    text::tf(
                        "policy.capabilityOff",
                        &[("capability", &spec.capability.label())],
                    )
                },
            );
            self.recorder.tool_denied(&key, &call, &message);
            return Err(message);
        };
        for t in tool
            .targets(&call.args)
            .into_iter()
            .chain(kivo_tools::targets(&call.tool, &call.args))
        {
            if !call.targets.contains(&t) {
                call.targets.push(t);
            }
        }
        if let Err(e) = tool.hard_limit(&call.args) {
            self.recorder.tool_denied(&key, &call, &e.message);
            return Err(e.message);
        }
        let spec = tool.spec().clone();
        let assessed = tool.assess(&call.args, call.initiated_by);
        let limits = HardLimits {
            stopped: cancel.is_cancelled(),
            blocked_apps: config.permissions.blocked_apps.clone(),
        };
        let decision = kivo_security::authorize(
            &spec,
            &call,
            &Context {
                mode: self.core.state().borrow().mode,
                session: SessionKind::Owner,
                taint: Taint::Clean,
                capabilities: &config.capabilities,
                limits: &limits,
                grants: &[],
                tools: &config.tools,
                privacy: config.privacy.mode,
                assessed: Some(assessed),
                task_grants: Some(grants),
            },
        );
        self.recorder.tool_decision(&key, &call, &spec, &decision);
        let permit = match decision {
            Decision::Allow(p) => p,
            Decision::Deny(d) => return Err(d.message),
            // Task calls are never asked mid-run: the engine only allows or refuses them.
            Decision::Confirm(_) => return Err(text::t("policy.taskNotGranted")),
        };
        let (result, output) = kivo_tools::execute(tool, &call, permit, cancel).await;
        self.recorder.tool_result(&key, &call, &result);
        match (result.status, output) {
            (Ok(_), Some(out)) => Ok(out.say),
            (Ok(_), None) => Ok(String::new()),
            (Err(e), _) => Err(e.message),
        }
    }

    /// Runs a success criterion; `Ok` with what was checked, `Err` with why it didn't pass.
    async fn verify(
        &self,
        task: &str,
        spec: &TaskSpec,
        criterion: &Criterion,
        grants: &[Grant],
        cancel: &CancellationToken,
    ) -> Result<String, String> {
        let title = criterion_title(criterion);
        match criterion {
            Criterion::CommandSucceeds { command, cwd } => {
                let args = check_args(command, cwd.as_deref().or(spec.cwd.as_deref()));
                let config = self.core.config();
                let Some(tool) = self.registry.get("shell.run", &config.capabilities) else {
                    return Err(text::tf(
                        "policy.capabilityOff",
                        &[("capability", &kivo_core::Capability::Shell.label())],
                    ));
                };
                let mut call = ToolCall {
                    id: format!("{task}-verify"),
                    tool: "shell.run".into(),
                    args,
                    initiated_by: Initiator::Task,
                    targets: Vec::new(),
                };
                call.targets = tool.targets(&call.args);
                let limits = HardLimits {
                    stopped: cancel.is_cancelled(),
                    blocked_apps: config.permissions.blocked_apps.clone(),
                };
                let spec_tool = tool.spec().clone();
                let decision = kivo_security::authorize(
                    &spec_tool,
                    &call,
                    &Context {
                        mode: self.core.state().borrow().mode,
                        session: SessionKind::Owner,
                        taint: Taint::Clean,
                        capabilities: &config.capabilities,
                        limits: &limits,
                        grants: &[],
                        tools: &config.tools,
                        privacy: config.privacy.mode,
                        assessed: Some(tool.assess(&call.args, Initiator::Task)),
                        task_grants: Some(grants),
                    },
                );
                let key = task_key(task);
                self.recorder
                    .tool_decision(&key, &call, &spec_tool, &decision);
                let Decision::Allow(permit) = decision else {
                    return Err(text::tf("task.checkNotAllowed", &[("check", &title)]));
                };
                let (result, output) = kivo_tools::execute(tool, &call, permit, cancel).await;
                self.recorder.tool_result(&key, &call, &result);
                let exit = output
                    .as_ref()
                    .and_then(|o| o.data["exitCode"].as_i64())
                    .or_else(|| {
                        result
                            .status
                            .as_ref()
                            .ok()
                            .and_then(|v| v["exitCode"].as_i64())
                    });
                match exit {
                    Some(0) => Ok(text::tf("task.checkPassed", &[("check", &title)])),
                    _ => {
                        let tail = output
                            .as_ref()
                            .map(|o| {
                                let out = o.data["stderr"].as_str().unwrap_or_default().to_owned()
                                    + o.data["stdout"].as_str().unwrap_or_default();
                                let lines: Vec<&str> = out.lines().rev().take(3).collect();
                                lines.into_iter().rev().collect::<Vec<_>>().join(" ")
                            })
                            .unwrap_or_default();
                        Err(format!("{title}: {}", clip(tail.trim()))
                            .trim_end_matches(": ")
                            .to_owned())
                    }
                }
            }
            Criterion::FileExists { path } => {
                if std::path::Path::new(path).exists() {
                    Ok(text::tf("task.checkPassed", &[("check", &title)]))
                } else {
                    Err(title)
                }
            }
            Criterion::WindowExists { title: wanted } => {
                let open = self
                    .windows
                    .list()
                    .unwrap_or_default()
                    .iter()
                    .any(|w| w.title.to_lowercase().contains(&wanted.to_lowercase()));
                if open {
                    Ok(text::tf("task.checkPassed", &[("check", &title)]))
                } else {
                    Err(title)
                }
            }
        }
    }

    /// An AI step (ROUT-08): a chat brain with the chosen profile, metered to the task and the
    /// routine, refused when a spending limit is reached (no one is there to be asked).
    async fn ask_brain(
        &self,
        task: &str,
        spec: &TaskSpec,
        prompt: &str,
        profile: Option<&str>,
        cancel: &CancellationToken,
    ) -> Result<String, String> {
        let config = self.core.config();
        let route: Route = self
            .brains
            .route(&config, prompt, profile, false)
            .map_err(|e| e.to_string())?;
        if route.kind == ProviderKind::Cli {
            return Err(text::t("task.needsChatBrain"));
        }
        if route.privacy != PrivacyClass::Local
            && self
                .brains
                .limit_states(&route.target.provider, &route.profile, "brain")
                .iter()
                .any(|(_, s)| s.reached)
        {
            return Err(text::t("task.limitReached"));
        }
        let provider = self
            .brains
            .provider(&route.target.provider)
            .ok_or_else(|| text::t("task.needsChatBrain"))?;
        let request = ChatRequest {
            model: route.target.model.clone(),
            system: vec![SystemBlock {
                text: text::t("task.brainSystem"),
                cacheable: true,
            }],
            messages: vec![Message::user(prompt.to_owned())],
            tools: Vec::new(),
            max_tokens: 700,
            temperature: Some(0.3),
            reasoning: route.target.reasoning,
        };
        let collected = kivo_brain::collect(provider.chat(request, cancel.child_token())).await;
        self.brains.meter(&Meter {
            kind: "brain",
            provider: &route.target.provider,
            model: &route.target.model,
            profile: Some(&route.profile),
            usage: collected.usage,
            turn: None,
            task: Some(task),
            routine: spec.routine_id.as_deref(),
        });
        if let Some(e) = collected.error {
            self.brains.failed(&route.target.provider, &e);
            return Err(crate::engine::failure_text(&route.target_name, &e));
        }
        let answer = collected.text.trim().to_owned();
        if answer.is_empty() {
            return Err(text::t("task.emptyAnswer"));
        }
        Ok(answer)
    }

    /// Work handed to a CLI agent (BRAIN-32): its permission requests become the task's
    /// questions; its last message is the step's result.
    async fn run_agent(
        self: &Arc<Self>,
        task: &str,
        prompt: &str,
        agent: Option<&str>,
        cwd: Option<&str>,
        cancel: &CancellationToken,
    ) -> Result<String, String> {
        let config = self.core.config();
        let folder = cwd.map_or_else(|| self.agents.workspace(), std::path::PathBuf::from);
        let id = match agent {
            Some(a) => a.to_owned(),
            None => {
                // The agent chosen for this folder's workspace goes first (CONVERSATION §4).
                let preferred = lock(&self.db)
                    .workspace_by_path(
                        &crate::workspaces::project_root(&folder)
                            .display()
                            .to_string(),
                    )
                    .ok()
                    .flatten()
                    .and_then(|w| w.preferred_agent);
                let route = self
                    .brains
                    .route_in(&config, prompt, Some("coding"), false, preferred.as_deref())
                    .map_err(|e| e.to_string())?;
                if route.kind != ProviderKind::Cli {
                    return Err(text::t("task.noAgent"));
                }
                route.target.provider
            }
        };
        let found = self.brains.agent(&id);
        let session = self
            .agents
            .session(&id, found.as_ref().map(|f| f.program.as_path()), &folder)
            .await
            .map_err(|e| crate::engine::failure_text(&id, &e))?;
        let handler: Arc<dyn PermissionHandler> = Arc::new(TaskPermissions {
            tasks: Arc::downgrade(self),
            task: task.to_owned(),
            always: Mutex::default(),
        });
        self.agents.route_session(session.id(), handler);
        if config.automation.live_activities.agent {
            self.core.set_activity(LiveActivity {
                id: task.to_owned(),
                kind: "agent".into(),
                title: kivo_brain::catalog::entry(&id).map_or(id.clone(), |e| e.name.to_owned()),
                detail: None,
                progress: None,
                until: None,
                task_id: Some(task.to_owned()),
            });
        }
        let mut events = session.prompt(prompt, cancel.child_token());
        let mut message = String::new();
        let mut failed = None;
        while let Some(event) = events.recv().await {
            match event {
                AgentEvent::Message(delta) => message.push_str(&delta),
                AgentEvent::Tool { title, .. } if !title.is_empty() => {
                    self.core.set_activity(LiveActivity {
                        id: task.to_owned(),
                        kind: "agent".into(),
                        title: kivo_brain::catalog::entry(&id)
                            .map_or(id.clone(), |e| e.name.to_owned()),
                        detail: Some(title),
                        progress: None,
                        until: None,
                        task_id: Some(task.to_owned()),
                    });
                }
                AgentEvent::Done(_) => break,
                AgentEvent::Error(e) => {
                    failed = Some(e);
                    break;
                }
                _ => {}
            }
        }
        self.agents.unroute_session(session.id());
        self.core.remove_activity(task);
        self.brains.meter(&Meter {
            kind: "agent",
            provider: &id,
            model: "",
            profile: Some("coding"),
            usage: kivo_brain::Usage::default(),
            turn: None,
            task: Some(task),
            routine: None,
        });
        if cancel.is_cancelled() {
            return Err(text::t("reply.cancelled"));
        }
        if let Some(e) = failed {
            if matches!(e, kivo_brain::NormalizedError::ProviderDown(_)) {
                self.agents.forget(&id, &folder).await;
            }
            return Err(crate::engine::failure_text(&id, &e));
        }
        let summary = message.trim().to_owned();
        Ok(if summary.is_empty() {
            text::t("task.agentDone")
        } else {
            summary
        })
    }

    /// Puts a question to the user and waits for the answer (or the task's end).
    async fn ask_user(
        &self,
        task: &str,
        step: &str,
        question: &str,
        choices: &[&str],
        cancel: &CancellationToken,
    ) -> Option<String> {
        let (tx, rx) = oneshot::channel();
        let title = {
            let mut live = lock(&self.live);
            let l = live.get_mut(task)?;
            l.question = Some((
                TaskQuestion {
                    step: step.to_owned(),
                    text: question.to_owned(),
                    choices: choices.iter().map(|c| (*c).to_owned()).collect(),
                },
                tx,
            ));
            l.title.clone()
        };
        self.set_status(task, TaskStatus::NeedsYou, None, None);
        self.publish(
            task,
            TaskEvent::Waiting {
                reason: question.to_owned(),
            },
        );
        self.notifier
            .announce(Announcement {
                source: "tasks".into(),
                title,
                text: question.to_owned(),
                urgent: true,
                tell_me: true,
                task: Some(task.to_owned()),
            })
            .await;
        let answer = tokio::select! {
            a = rx => a.ok(),
            () = cancel.cancelled() => None,
        };
        if let Some(l) = lock(&self.live).get_mut(task) {
            l.question = None;
        }
        self.set_status(task, TaskStatus::Running, None, None);
        answer
    }

    // ---- Tasks run elsewhere ----------------------------------------------------------------

    /// A task whose steps run in a turn (a coding request handed to an agent, BRAIN-32): recorded
    /// so Tasks and Home show it, its steps and its result.
    pub fn record(&self, spec: &TaskSpec) -> Option<String> {
        let id = TaskId::new().to_string();
        let steps: Vec<(String, String)> = spec
            .steps
            .iter()
            .map(|s| (s.id.clone(), self.step_title(s)))
            .collect();
        let body = serde_json::to_string(spec).ok()?;
        self.db(|db| {
            db.insert_task(&NewTask {
                id: &id,
                title: &spec.title,
                kind: kind_str(spec.kind),
                owner: &spec.owner,
                status: TaskStatus::Running.as_str(),
                spec: &body,
                routine_id: spec.routine_id.as_deref(),
                turn_id: spec.turn_id.as_deref(),
                steps: &steps,
            })
        })?;
        self.recorder
            .task_activity(&id, &spec.title, "started", None);
        self.publish(
            &id,
            TaskEvent::Created {
                name: spec.title.clone(),
            },
        );
        self.refresh_count();
        Some(id)
    }

    /// A recorded task's step changed: `running`, `done`, `failed`.
    pub fn record_step(&self, id: &str, step: &str, status: &str, detail: Option<&str>) {
        let finished = matches!(status, "done" | "failed" | "skipped");
        self.set_step(
            id,
            step,
            &StepUpdate {
                status,
                detail,
                started: status == "running",
                finished,
                ..Default::default()
            },
        );
    }

    /// A recorded task ended.
    pub fn record_end(&self, id: &str, done: bool, text: &str) {
        let title = self
            .db(|db| db.task(id))
            .flatten()
            .map(|t| t.title)
            .unwrap_or_default();
        if done {
            self.set_status(id, TaskStatus::Done, Some(text), None);
            self.recorder.task_activity(id, &title, "done", Some(text));
            self.publish(id, TaskEvent::Completed);
        } else {
            self.set_status(id, TaskStatus::Failed, None, Some(text));
            self.recorder
                .task_activity(id, &title, "failed", Some(text));
            self.publish(
                id,
                TaskEvent::Failed {
                    message: text.to_owned(),
                },
            );
        }
    }

    // ---- The user's controls ----------------------------------------------------------------

    /// The user answered a task's question.
    pub fn answer(&self, id: &str, choice: &str) -> Result<(), String> {
        let sender = lock(&self.live)
            .get_mut(id)
            .and_then(|l| {
                let ok = l
                    .question
                    .as_ref()
                    .is_some_and(|(q, _)| q.choices.iter().any(|c| c == choice));
                ok.then(|| l.question.take()).flatten()
            })
            .ok_or_else(|| text::t("turn.nothingWaiting"))?;
        let _ = sender.1.send(choice.to_owned());
        Ok(())
    }

    pub fn cancel(&self, id: &str, reason: CancelReason) -> Result<(), String> {
        let live = lock(&self.live);
        let l = live.get(id).ok_or_else(|| text::t("task.notRunning"))?;
        *lock(&l.ending) = Some(Ending::Cancelled(reason));
        l.cancel.cancel();
        Ok(())
    }

    /// Pauses one task (the user) or every task (the emergency stop, SECURITY §8).
    pub fn pause(&self, id: &str) -> Result<(), String> {
        let live = lock(&self.live);
        let l = live.get(id).ok_or_else(|| text::t("task.notRunning"))?;
        *lock(&l.ending) = Some(Ending::Paused);
        l.cancel.cancel();
        Ok(())
    }

    pub fn pause_all(&self) -> usize {
        let live = lock(&self.live);
        for l in live.values() {
            *lock(&l.ending) = Some(Ending::Paused);
            l.cancel.cancel();
        }
        live.len()
    }

    /// Carries on with a paused (or interrupted) task from its unfinished steps; a step that was
    /// running when it stopped runs again, so the user decides (never automatic).
    pub fn resume(&self, id: &str) -> Result<(), String> {
        if lock(&self.live).contains_key(id) {
            return Err(text::t("task.alreadyRunning"));
        }
        let stored = self
            .db(|db| db.task(id))
            .flatten()
            .ok_or_else(|| text::t("task.notFound"))?;
        if !matches!(stored.status.as_str(), "paused" | "interrupted" | "waiting") {
            return Err(text::t("task.cantResume"));
        }
        let spec: TaskSpec =
            serde_json::from_str(&stored.spec).map_err(|_| text::t("task.cantResume"))?;
        let finished: HashSet<String> = self
            .db(|db| db.task_steps(id))
            .unwrap_or_default()
            .into_iter()
            .filter(|s| s.status == "done" || s.status == "skipped")
            .map(|s| s.step_id)
            .collect();
        self.set_status(id, TaskStatus::Pending, None, None);
        self.recorder
            .task_activity(id, &stored.title, "resumed", None);
        self.start(id.to_owned(), spec, finished);
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        if lock(&self.live).contains_key(id) {
            return Err(text::t("task.stillRunning"));
        }
        self.db(|db| db.delete_task(id));
        self.refresh_count();
        Ok(())
    }

    pub fn clear_finished(&self) -> usize {
        let n = self.db(Database::clear_finished_tasks).unwrap_or(0);
        self.refresh_count();
        n
    }

    /// Quitting: running steps are cancelled; watchers stay "waiting" for the next start.
    pub fn shutdown(&self) {
        let live = lock(&self.live);
        for l in live.values() {
            *lock(&l.ending) = Some(Ending::Shutdown);
            l.cancel.cancel();
        }
    }

    /// At start: tasks that were running when KIVO last stopped are reported as interrupted
    /// (ARCH-27), waiting watchers are armed again (a reminder that came due while KIVO was off
    /// says so), and old finished tasks are summarized (MEM-02).
    pub async fn startup(&self) -> Vec<StoredTask> {
        let interrupted = self.db(Database::interrupt_unfinished).unwrap_or_default();
        for t in &interrupted {
            self.recorder
                .task_activity(&t.id, &t.title, "interrupted", None);
        }
        let waiting = self.db(Database::waiting_tasks).unwrap_or_default();
        for t in waiting {
            let _ = self.resume(&t.id);
        }
        let cutoff = kivo_store::brains::now_ms() - DETAIL_KEPT_MS;
        self.db(|db| db.summarize_old_tasks(cutoff));
        self.refresh_count();
        if !interrupted.is_empty() {
            let names: Vec<&str> = interrupted.iter().map(|t| t.title.as_str()).collect();
            let said = text::tf("task.interrupted", &[("names", &names.join(", "))]);
            self.notifier
                .announce(Announcement {
                    source: "tasks".into(),
                    title: text::t("task.interruptedTitle"),
                    text: said,
                    urgent: false,
                    tell_me: false,
                    task: None,
                })
                .await;
        }
        interrupted
    }

    // ---- Views -----------------------------------------------------------------------------

    pub fn view(&self, id: &str) -> Option<TaskView> {
        let stored = self.db(|db| db.task(id)).flatten()?;
        Some(self.to_view(stored))
    }

    pub fn list(&self, finished: bool, limit: u32) -> Vec<TaskView> {
        self.db(|db| db.tasks(finished, limit))
            .unwrap_or_default()
            .into_iter()
            .map(|t| self.to_view(t))
            .collect()
    }

    fn to_view(&self, t: StoredTask) -> TaskView {
        let steps = self.db(|db| db.task_steps(&t.id)).unwrap_or_default();
        let spec: Option<TaskSpec> = serde_json::from_str(&t.spec).ok();
        let question = lock(&self.live)
            .get(&t.id)
            .and_then(|l| l.question.as_ref().map(|(q, _)| q.clone()));
        TaskView {
            id: t.id,
            title: t.title,
            kind: kind_of(&t.kind),
            owner: t.owner,
            status: TaskStatus::parse(&t.status).unwrap_or(TaskStatus::Failed),
            steps: steps
                .into_iter()
                .map(|s| TaskStepView {
                    id: s.step_id,
                    title: s.title,
                    status: s.status,
                    detail: s.detail,
                    started_at: s.started_at,
                    finished_at: s.finished_at,
                    attempts: u32::try_from(s.attempts).unwrap_or(0),
                })
                .collect(),
            created_at: t.created_at,
            updated_at: t.updated_at,
            finished_at: t.finished_at,
            result: t.result,
            error: t.error,
            summary: t.summary,
            uses_ai: spec.as_ref().is_some_and(TaskSpec::uses_ai),
            question,
            routine_id: t.routine_id,
        }
    }

    /// Tasks running now.
    pub fn running(&self) -> usize {
        lock(&self.live).len()
    }
}

/// An agent working for a task asks permission: reads are fine, anything else is the task's
/// question to the user ("Allow", "Always for this task", "Deny").
struct TaskPermissions {
    tasks: Weak<Tasks>,
    task: String,
    always: Mutex<HashSet<String>>,
}

#[async_trait::async_trait]
impl PermissionHandler for TaskPermissions {
    async fn ask(&self, ask: PermissionAsk) -> PermissionAnswer {
        let Some(tasks) = self.tasks.upgrade() else {
            return pick(&ask, false, false);
        };
        let spec = crate::engine::agent_spec(&ask.kind);
        if spec.risk <= kivo_core::tool::Risk::Low && !spec.data_egress {
            return pick(&ask, true, false);
        }
        if lock(&self.always).contains(&ask.kind) {
            return pick(&ask, true, false);
        }
        let cancel = lock(&tasks.live)
            .get(&self.task)
            .map(|l| l.cancel.clone())?;
        let question = text::tf("task.agentAsks", &[("title", &ask.title)]);
        let answer = tasks
            .ask_user(
                &self.task,
                "agent",
                &question,
                &["allow", "always", "deny"],
                &cancel,
            )
            .await;
        match answer.as_deref() {
            Some("allow") => pick(&ask, true, false),
            Some("always") => {
                lock(&self.always).insert(ask.kind.clone());
                pick(&ask, true, false)
            }
            _ => pick(&ask, false, false),
        }
    }
}

/// The `shell.run` arguments a command check runs with, and so the grant it needs.
pub fn check_args(command: &str, cwd: Option<&str>) -> serde_json::Value {
    let mut args = json!({ "command": command });
    if let Some(c) = cwd {
        args["cwd"] = json!(c);
    }
    args
}

/// The grants a task's success checks need (a command check runs through `shell.run`).
pub fn check_grants(spec: &TaskSpec) -> Vec<kivo_core::task::TaskGrant> {
    spec.success
        .iter()
        .filter_map(|c| match c {
            Criterion::CommandSucceeds { command, cwd } => Some(kivo_core::task::TaskGrant {
                tool: "shell.run".into(),
                args: check_args(command, cwd.as_deref().or(spec.cwd.as_deref())),
            }),
            _ => None,
        })
        .collect()
}

pub fn watch_title(watch: &kivo_core::task::WatchSpec) -> String {
    use kivo_core::task::WatchSpec as W;
    match watch {
        W::Time { at_ms } => text::tf("task.step.time", &[("time", &local_clock(*at_ms))]),
        W::ProcessExit { name, pid } => text::tf(
            "task.step.process",
            &[(
                "name",
                &name
                    .clone()
                    .or_else(|| pid.map(|p| p.to_string()))
                    .unwrap_or_default(),
            )],
        ),
        W::Folder { path, .. } => text::tf("task.step.folder", &[("path", path)]),
        W::Download { .. } => text::t("task.step.download"),
        W::Window { app, title, event } => text::tf(
            match event {
                kivo_core::task::WindowEvent::Opened => "task.step.windowOpen",
                kivo_core::task::WindowEvent::Closed => "task.step.windowClose",
            },
            &[(
                "name",
                &title.clone().or_else(|| app.clone()).unwrap_or_default(),
            )],
        ),
    }
}

pub fn criterion_title(c: &Criterion) -> String {
    match c {
        Criterion::CommandSucceeds { command, .. } => {
            text::tf("task.check.command", &[("command", command)])
        }
        Criterion::FileExists { path } => text::tf("task.check.file", &[("path", path)]),
        Criterion::WindowExists { title } => text::tf("task.check.window", &[("title", title)]),
    }
}

/// "15:30" in local time.
pub fn local_clock(at_ms: i64) -> String {
    let offset = i64::from(crate::tasks::utc_offset());
    let secs = at_ms / 1000 + offset * 60;
    let of_day = secs.rem_euclid(86_400);
    format!("{:02}:{:02}", of_day / 3600, of_day % 3600 / 60)
}

static UTC_OFFSET: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

/// The local time offset, set once at start (for clock times in titles).
pub fn set_utc_offset(minutes: i32) {
    UTC_OFFSET.store(minutes, std::sync::atomic::Ordering::Relaxed);
}

pub fn utc_offset() -> i32 {
    UTC_OFFSET.load(std::sync::atomic::Ordering::Relaxed)
}
