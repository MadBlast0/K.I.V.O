//! Event tasks as tools (BRAIN-07): "remind me in 20 minutes to stretch", "tell me when the build
//! finishes", "tell me when Teams closes". The grammar maps the usual phrasings straight to these
//! tools; a brain gets the same tools for the rest. Each creates a task with a watcher and returns
//! at once: the waiting costs nothing, and the user is told when the event happens (TOOL-29).

use crate::tasks::Tasks;
use crate::watchers::program_name;
use kivo_core::Capability;
use kivo_core::task::{
    Notify, OnError, StepAction, TaskKind, TaskSpec, TaskStep, WatchSpec, WindowEvent,
};
use kivo_core::text;
use kivo_core::tool::{
    CapabilityTier, Platform, Reversibility, Risk, SideEffect, ToolError, ToolErrorCode, ToolSpec,
};
use kivo_tools::{Output, Tool};
use serde_json::{Value, json};
use std::sync::{Arc, OnceLock, Weak};

/// The task manager, set once it exists (it needs the registry these tools live in).
pub type TaskHandle = Arc<OnceLock<Weak<Tasks>>>;

fn spec(id: &str, title: &str, description: &str, params: Value) -> ToolSpec {
    ToolSpec {
        id: id.into(),
        description: description.into(),
        title: title.into(),
        params,
        result: json!({ "type": "object" }),
        risk: Risk::Safe,
        side_effects: vec![SideEffect::None],
        data_egress: false,
        timeout_ms: 5_000,
        cancellable: false,
        tier: CapabilityTier::Native,
        platforms: vec![Platform::Windows, Platform::MacOs, Platform::Linux],
        reversibility: Reversibility::NotApplicable,
        capability: Capability::BackgroundTasks,
    }
}

fn failed(message: String) -> ToolError {
    ToolError::new(ToolErrorCode::Failed, message)
}

fn tasks(handle: &TaskHandle) -> Result<Arc<Tasks>, ToolError> {
    handle
        .get()
        .and_then(Weak::upgrade)
        .ok_or_else(|| failed(text::t("task.notReady")))
}

fn watch_task(title: String, kind: TaskKind, watch: WatchSpec, say: String) -> TaskSpec {
    TaskSpec {
        title,
        kind,
        owner: text::t("task.ownerYou"),
        steps: vec![
            TaskStep {
                id: "watch".into(),
                title: String::new(),
                action: StepAction::Watch { watch },
                depends_on: Vec::new(),
                on_error: OnError::Stop,
                delay_ms: None,
                confirm: false,
            },
            TaskStep {
                id: "tell".into(),
                title: String::new(),
                action: StepAction::Say { text: say },
                depends_on: vec!["watch".into()],
                on_error: OnError::Stop,
                delay_ms: None,
                confirm: false,
            },
        ],
        success: Vec::new(),
        grants: Vec::new(),
        timeout_ms: None,
        notify: Notify::Speak,
        routine_id: None,
        turn_id: None,
        cwd: None,
    }
}

/// `tasks.remind`: a reminder or timer.
pub struct Remind {
    spec: ToolSpec,
    handle: TaskHandle,
}

impl Remind {
    pub fn new(handle: TaskHandle) -> Self {
        Self {
            spec: spec(
                "tasks.remind",
                &text::t("tool.tasks.remind"),
                "Remind the user later: after a number of minutes or hours, or at a clock time (HH:MM, today or tomorrow). Also a plain timer.",
                json!({
                    "type": "object",
                    "properties": {
                        "number": { "type": "number", "description": "How many minutes or hours from now" },
                        "unit": { "type": "string", "enum": ["minutes", "hours"] },
                        "at": { "type": "string", "description": "A clock time, HH:MM, instead of number and unit" },
                        "text": { "type": "string", "description": "What to remind them of" }
                    }
                }),
            ),
            handle,
        }
    }
}

/// When `at` ("15:30") next comes, in epoch ms, given the local offset.
pub fn next_clock_time(at: &str, now_ms: i64, utc_offset_minutes: i32) -> Option<i64> {
    let (h, m) = at.trim().split_once(':')?;
    let (h, m): (i64, i64) = (h.trim().parse().ok()?, m.trim().parse().ok()?);
    if !(0..24).contains(&h) || !(0..60).contains(&m) {
        return None;
    }
    let offset = i64::from(utc_offset_minutes) * 60_000;
    let local_now = now_ms + offset;
    let day_start = local_now - local_now.rem_euclid(86_400_000);
    let mut local = day_start + (h * 60 + m) * 60_000;
    if local <= local_now {
        local += 86_400_000;
    }
    Some(local - offset)
}

impl Tool for Remind {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn run(&self, args: &Value) -> Result<Output, ToolError> {
        let tasks = tasks(&self.handle)?;
        let now = kivo_store::brains::now_ms();
        let what = args["text"]
            .as_str()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_owned);
        let at_ms = if let Some(at) = args["at"].as_str() {
            next_clock_time(at, now, crate::tasks::utc_offset())
                .ok_or_else(|| failed(text::t("task.badTime")))?
        } else {
            let n = args["number"]
                .as_f64()
                .filter(|n| *n > 0.0 && *n <= 24.0 * 60.0)
                .ok_or_else(|| failed(text::t("task.badTime")))?;
            let unit_ms = if args["unit"].as_str() == Some("hours") {
                3_600_000.0
            } else {
                60_000.0
            };
            #[allow(clippy::cast_possible_truncation, reason = "at most a day in ms")]
            let delay = (n * unit_ms) as i64;
            now + delay
        };
        let say = what.as_ref().map_or_else(
            || text::t("task.reminderPlain"),
            |w| text::tf("task.reminderSay", &[("text", w)]),
        );
        let title = what
            .clone()
            .unwrap_or_else(|| text::t("task.reminderTitle"));
        let clock = crate::tasks::local_clock(at_ms);
        let id = tasks
            .create(watch_task(
                title,
                TaskKind::Reminder,
                WatchSpec::Time { at_ms },
                say,
            ))
            .map_err(failed)?;
        Ok(Output::new(
            text::tf("task.reminderSet", &[("when", &clock)]),
            json!({ "task": id, "at": at_ms }),
        ))
    }
}

/// `tasks.watch`: tell the user when something happens.
pub struct Watch {
    spec: ToolSpec,
    handle: TaskHandle,
}

impl Watch {
    pub fn new(handle: TaskHandle) -> Self {
        Self {
            spec: spec(
                "tasks.watch",
                &text::t("tool.tasks.watch"),
                "Tell the user when something happens, without using AI while waiting: a build or process finishes (kind build or process with a program name), a download finishes, a file changes in a folder, or an app's window opens or closes.",
                json!({
                    "type": "object",
                    "properties": {
                        "kind": { "type": "string", "enum": ["build", "process", "download", "folder", "window"] },
                        "name": { "type": "string", "description": "The program (process) or app (window)" },
                        "app": { "type": "object", "description": "An app from the fast path: {id, name}" },
                        "path": { "type": "string", "description": "The folder to watch" },
                        "pattern": { "type": "string", "description": "A file-name filter such as *.pdf" },
                        "event": { "type": "string", "enum": ["opened", "closed"] }
                    },
                    "required": ["kind"]
                }),
            ),
            handle,
        }
    }
}

impl Tool for Watch {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn run(&self, args: &Value) -> Result<Output, ToolError> {
        let tasks = tasks(&self.handle)?;
        let app_name = args["app"]["name"]
            .as_str()
            .or_else(|| args["name"].as_str())
            .map(str::to_owned);
        let app_id = args["app"]["id"].as_str().map(str::to_owned);
        let (title, watch, say) = match args["kind"].as_str() {
            Some("build") => {
                let (pid, name) = tasks
                    .watchers()
                    .running_build()
                    .ok_or_else(|| failed(text::t("task.noBuild")))?;
                (
                    text::tf("task.watchBuild", &[("name", &name)]),
                    WatchSpec::ProcessExit {
                        pid: Some(pid),
                        name: Some(name.clone()),
                    },
                    String::new(),
                )
            }
            Some("process") => {
                let name = app_id
                    .as_deref()
                    .or(app_name.as_deref())
                    .map(program_name)
                    .ok_or_else(|| failed(text::t("watch.nothingToWatch")))?;
                if tasks.watchers().find(&name).is_empty() {
                    return Err(failed(text::tf("watch.notRunning", &[("name", &name)])));
                }
                let label = app_name.clone().unwrap_or_else(|| name.clone());
                (
                    text::tf("task.watchProcess", &[("name", &label)]),
                    WatchSpec::ProcessExit {
                        pid: None,
                        name: Some(name),
                    },
                    String::new(),
                )
            }
            Some("download") => (
                text::t("task.watchDownload"),
                WatchSpec::Download { folder: None },
                String::new(),
            ),
            Some("folder") => {
                let path = args["path"]
                    .as_str()
                    .ok_or_else(|| failed(text::t("watch.nothingToWatch")))?;
                let safe = kivo_tools::files::safe_path(path).map_err(|e| failed(e.message))?;
                let path = safe.display().to_string();
                (
                    text::tf("task.watchFolder", &[("path", &path)]),
                    WatchSpec::Folder {
                        path,
                        pattern: args["pattern"].as_str().map(str::to_owned),
                    },
                    String::new(),
                )
            }
            Some("window") => {
                let label = app_name
                    .clone()
                    .ok_or_else(|| failed(text::t("watch.nothingToWatch")))?;
                let event = if args["event"].as_str() == Some("opened") {
                    WindowEvent::Opened
                } else {
                    WindowEvent::Closed
                };
                let key = match event {
                    WindowEvent::Opened => "watch.windowOpened",
                    WindowEvent::Closed => "watch.windowClosed",
                };
                (
                    crate::tasks::watch_title(&WatchSpec::Window {
                        app: Some(label.clone()),
                        title: None,
                        event,
                    }),
                    WatchSpec::Window {
                        app: Some(app_id.map_or(label.clone(), |id| program_name(&id))),
                        title: None,
                        event,
                    },
                    text::tf(key, &[("name", &label)]),
                )
            }
            _ => return Err(failed(text::t("watch.nothingToWatch"))),
        };
        // An empty "say": the watcher's own words tell it ("cargo finished with errors").
        let mut task = watch_task(title.clone(), TaskKind::Watch, watch, say.clone());
        if say.is_empty() {
            task.steps.truncate(1);
        }
        let id = tasks.create(task).map_err(failed)?;
        Ok(Output::new(
            text::tf("task.watching", &[("title", &title)]),
            json!({ "task": id }),
        ))
    }
}

/// `tasks.propose_plan` (BRAIN-30): a multi-step request as a plan the user approves in one go.
/// Approving it grants exactly its steps (SEC-09); independent steps run side by side; checks the
/// plan declares must pass before it is called done (BRAIN-31).
pub struct ProposePlan {
    spec: ToolSpec,
    handle: TaskHandle,
}

impl ProposePlan {
    pub fn new(handle: TaskHandle) -> Self {
        let mut spec = spec(
            "tasks.propose_plan",
            &text::t("tool.tasks.proposePlan"),
            "For requests with several steps: propose them as one plan the user approves. Each step is a tool call with its arguments; list the step ids it waits for in dependsOn (steps that don't wait for each other run at the same time). Add success checks (commandSucceeds, fileExists, windowExists) that must pass before the plan counts as done. The plan runs in the background and the user is told when it finishes.",
            json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "steps": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string" },
                                "title": { "type": "string" },
                                "tool": { "type": "string", "description": "A tool id, such as apps.launch" },
                                "args": { "type": "object" },
                                "dependsOn": { "type": "array", "items": { "type": "string" } }
                            },
                            "required": ["id", "tool", "args"]
                        }
                    },
                    "success": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "type": { "type": "string", "enum": ["commandSucceeds", "fileExists", "windowExists"] },
                                "command": { "type": "string" },
                                "cwd": { "type": ["string", "null"] },
                                "path": { "type": "string" },
                                "title": { "type": "string" }
                            },
                            "required": ["type"]
                        }
                    }
                },
                "required": ["title", "steps"]
            }),
        );
        spec.risk = Risk::Medium;
        // It hands out permissions: never offered once a turn is tainted, always asked.
        spec.side_effects = vec![SideEffect::LocalWrite, SideEffect::SecuritySensitive];
        spec.reversibility = Reversibility::Irreversible;
        spec.timeout_ms = 10_000;
        Self { spec, handle }
    }
}

/// The plan in the tool's arguments, as a task.
pub fn plan_task(args: &Value, registry: &kivo_tools::Registry) -> Result<TaskSpec, ToolError> {
    let title = args["title"]
        .as_str()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| failed(text::t("task.planNoTitle")))?;
    let raw = args["steps"]
        .as_array()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| failed(text::t("task.planNoSteps")))?;
    let mut steps = Vec::new();
    for s in raw {
        let id = s["id"].as_str().unwrap_or_default().to_owned();
        let tool = s["tool"].as_str().unwrap_or_default().to_owned();
        if registry.known(&tool).is_none() || tool == "tasks.propose_plan" {
            return Err(failed(text::tf("task.unknownTool", &[("tool", &tool)])));
        }
        let args = s["args"].clone();
        let args = if args.is_object() { args } else { json!({}) };
        steps.push(TaskStep {
            title: s["title"].as_str().unwrap_or(&id).to_owned(),
            id,
            action: StepAction::Tool { tool, args },
            depends_on: s["dependsOn"]
                .as_array()
                .map(|d| {
                    d.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
            on_error: OnError::Stop,
            delay_ms: None,
            confirm: false,
        });
    }
    let success: Vec<kivo_core::task::Criterion> = args["success"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|c| serde_json::from_value(c.clone()).ok())
                .collect()
        })
        .unwrap_or_default();
    let mut task = TaskSpec {
        title: title.to_owned(),
        kind: TaskKind::Plan,
        owner: text::t("task.ownerYou"),
        steps,
        success,
        grants: Vec::new(),
        timeout_ms: None,
        notify: Notify::Speak,
        routine_id: None,
        turn_id: None,
        cwd: None,
    };
    task.validate()
        .map_err(|e| failed(text::tf("task.planInvalid", &[("reason", &e.to_string())])))?;
    // Approving the plan grants exactly its steps and its checks (SEC-09).
    task.grants = task
        .steps
        .iter()
        .filter_map(|s| match &s.action {
            StepAction::Tool { tool, args } => Some(kivo_core::task::TaskGrant {
                tool: tool.clone(),
                args: args.clone(),
            }),
            _ => None,
        })
        .chain(crate::tasks::check_grants(&task))
        .collect();
    Ok(task)
}

impl Tool for ProposePlan {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    /// The plan is as risky as its riskiest step.
    fn assess(&self, args: &Value, initiator: kivo_core::tool::Initiator) -> Risk {
        let Ok(tasks) = tasks(&self.handle) else {
            return Risk::High;
        };
        let registry = tasks.registry();
        let steps = args["steps"].as_array().cloned().unwrap_or_default();
        steps
            .iter()
            .map(|s| {
                let tool = s["tool"].as_str().unwrap_or_default();
                registry
                    .all_tools()
                    .find(|t| t.spec().id == tool)
                    .map_or(Risk::High, |t| t.assess(&s["args"], initiator))
            })
            .max()
            .unwrap_or(Risk::Medium)
            .max(Risk::Medium)
    }

    fn run(&self, args: &Value) -> Result<Output, ToolError> {
        let tasks = tasks(&self.handle)?;
        let task = plan_task(args, tasks.registry())?;
        let title = task.title.clone();
        let steps = task.steps.len();
        let id = tasks.create(task).map_err(failed)?;
        Ok(Output::new(
            text::tf(
                "task.planStarted",
                &[("title", &title), ("steps", &steps.to_string())],
            ),
            json!({ "task": id }),
        ))
    }
}

/// The event-task tools, sharing the handle the runtime fills once the task manager exists.
pub fn tools(handle: &TaskHandle) -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(Remind::new(Arc::clone(handle))),
        Arc::new(Watch::new(Arc::clone(handle))),
        Arc::new(ProposePlan::new(Arc::clone(handle))),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_times_are_the_next_one() {
        // 10:00 UTC on some day, offset +60 (11:00 local).
        let day = 20_000 * 86_400_000_i64;
        let now = day + 10 * 3_600_000;
        let at = next_clock_time("15:30", now, 60).unwrap();
        assert_eq!(
            at,
            day + 14 * 3_600_000 + 30 * 60_000,
            "15:30 local is 14:30 UTC"
        );
        let tomorrow = next_clock_time("09:00", now, 60).unwrap();
        assert_eq!(tomorrow, day + 86_400_000 + 8 * 3_600_000);
        assert!(next_clock_time("25:00", now, 0).is_none());
    }
}
