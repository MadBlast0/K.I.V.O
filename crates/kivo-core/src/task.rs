//! Tasks (ARCHITECTURE §4.2, BRAINS §7, TOOLS_AND_CONTROL §6): long-running work with a graph of
//! steps, its own cancellation, the grants it was given when it was created, and a history. Tasks
//! outlive turns and are persisted; after a crash they are reported as interrupted and never
//! resume a side effect on their own.
//!
//! A plan from a brain, a watcher ("tell me when the build finishes"), a reminder, a coding job
//! run by a CLI agent and a routine all become the same `TaskSpec`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What made the task.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskKind {
    /// A multi-step plan (BRAIN-30).
    Plan,
    /// "Tell me when …", "watch …" (BRAIN-07, TOOL-28).
    Watch,
    /// "Remind me …" (a time watcher).
    Reminder,
    /// Coding work delegated to a CLI agent (BRAIN-32).
    Coding,
    /// A routine's run (ROUTINES §3).
    Routine,
}

/// Where a task is in its life. `Waiting` is a watcher waiting for its event, with no brain call
/// and no polling.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskStatus {
    Pending,
    Running,
    Waiting,
    /// A step needs the user's answer (an error policy of `ask`, a validation that failed).
    NeedsYou,
    /// Stopped by the emergency stop; resumes only when the user says so (SECURITY §8).
    Paused,
    Done,
    Failed,
    Cancelled,
    /// It was running when KIVO stopped unexpectedly (ARCH-27).
    Interrupted,
}

impl TaskStatus {
    /// Finished for good.
    pub fn is_final(self) -> bool {
        matches!(
            self,
            Self::Done | Self::Failed | Self::Cancelled | Self::Interrupted
        )
    }

    /// Counts as running for the tray and Home (UX-56, UX-19).
    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Pending | Self::Running | Self::Waiting | Self::NeedsYou
        )
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Waiting => "waiting",
            Self::NeedsYou => "needsYou",
            Self::Paused => "paused",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        serde_json::from_value(Value::String(s.to_owned())).ok()
    }
}

/// What happens when a step fails (ROUT-04).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "policy")]
pub enum OnError {
    /// The task stops here (the default).
    #[default]
    Stop,
    /// The failure is noted and the task goes on.
    Continue,
    /// Tries again, up to `times` more times.
    Retry { times: u8 },
    /// Asks the user: retry, skip or stop.
    Ask,
}

/// Which window event a window watcher waits for.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WindowEvent {
    Opened,
    Closed,
}

/// What a watcher waits for (TOOL-28). Every one of them is an event from the system; none
/// polls a brain.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum WatchSpec {
    /// A moment (Unix milliseconds). Persisted; a moment missed while KIVO was off is reported
    /// on the next start.
    Time {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at_ms: i64,
    },
    /// A process ends: by id, or every process with this name (a build: `cargo`, `msbuild`).
    ProcessExit {
        pid: Option<u32>,
        name: Option<String>,
    },
    /// Something in a folder is created, changed or removed; `pattern` is a `*` name filter.
    Folder {
        path: String,
        pattern: Option<String>,
    },
    /// A download finishes: a new file in the download folder that isn't a partial download and
    /// has stopped growing.
    Download { folder: Option<String> },
    /// A window of an app (or with a title) opens or closes.
    Window {
        app: Option<String>,
        title: Option<String>,
        event: WindowEvent,
    },
}

/// A check that must pass before a task says "done" (BRAIN-31).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum Criterion {
    /// The command exits with 0 (the tests pass, the build builds).
    CommandSucceeds {
        command: String,
        cwd: Option<String>,
    },
    FileExists {
        path: String,
    },
    /// A window whose title contains this is open.
    WindowExists {
        title: String,
    },
}

/// What one step does.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum StepAction {
    /// A tool call, through the permission engine with the task's grants.
    Tool {
        tool: String,
        #[cfg_attr(feature = "ts", ts(type = "unknown"))]
        args: Value,
    },
    /// Waits for an event.
    Watch { watch: WatchSpec },
    /// Tells the user, following the proactive-speech rules (UX §7).
    Say { text: String },
    /// Asks a brain, with a chosen profile (ROUT-08); counted toward cost limits.
    Ask {
        prompt: String,
        profile: Option<String>,
    },
    /// Asks a brain to pick one of `options`; the answer is the step's result.
    Decide {
        question: String,
        options: Vec<String>,
        profile: Option<String>,
    },
    /// Hands work to a CLI agent over ACP (BRAIN-32).
    Agent {
        prompt: String,
        agent: Option<String>,
        cwd: Option<String>,
    },
    /// Checks a criterion (BRAIN-31).
    Verify { criterion: Criterion },
    /// Waits this long.
    Delay {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        ms: u64,
    },
}

impl StepAction {
    /// Whether this step calls a brain (routines with one are badged, ROUT-08).
    pub fn uses_ai(&self) -> bool {
        matches!(
            self,
            Self::Ask { .. } | Self::Decide { .. } | Self::Agent { .. }
        )
    }
}

/// One step of a task graph.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStep {
    pub id: String,
    pub title: String,
    pub action: StepAction,
    /// Steps that must finish first; steps with none waiting on each other run concurrently.
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub on_error: OnError,
    /// Waits this long before starting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub delay_ms: Option<u64>,
    /// Asks the user first ("Lock the PC?"); a no skips the step.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirm: bool,
}

/// How the user hears about the task's end (UX §7).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Notify {
    /// Spoken when the situation allows ("tell me when …").
    #[default]
    Speak,
    Toast,
    Silent,
}

/// A grant a task was given when it was created (SEC-09): a tool with its exact arguments
/// (`${name}` for a variable).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskGrant {
    pub tool: String,
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub args: Value,
}

/// Everything a task is made of.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSpec {
    pub title: String,
    pub kind: TaskKind,
    /// Who it's for and who made it: "you", a routine's name, a brain.
    pub owner: String,
    pub steps: Vec<TaskStep>,
    /// Must all pass before the task reports success (BRAIN-31).
    #[serde(default)]
    pub success: Vec<Criterion>,
    #[serde(default)]
    pub grants: Vec<TaskGrant>,
    /// The whole task is cancelled after this long (PLAN-03).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub notify: Notify,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routine_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// The workspace folder it works in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

/// Why a graph can't run.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GraphError {
    #[error("step {0} is listed twice")]
    Duplicate(String),
    #[error("step {step} waits for {missing}, which isn't in the task")]
    Missing { step: String, missing: String },
    #[error("the steps wait for each other in a circle")]
    Cycle,
    #[error("the task has no steps")]
    Empty,
}

impl TaskSpec {
    /// Checks the graph: unique ids, known dependencies, no cycles.
    pub fn validate(&self) -> Result<(), GraphError> {
        if self.steps.is_empty() {
            return Err(GraphError::Empty);
        }
        let mut seen = std::collections::HashSet::new();
        for s in &self.steps {
            if !seen.insert(s.id.as_str()) {
                return Err(GraphError::Duplicate(s.id.clone()));
            }
        }
        for s in &self.steps {
            if let Some(missing) = s.depends_on.iter().find(|d| !seen.contains(d.as_str())) {
                return Err(GraphError::Missing {
                    step: s.id.clone(),
                    missing: missing.clone(),
                });
            }
        }
        // Kahn's algorithm: every step must become ready.
        let mut done = std::collections::HashSet::new();
        loop {
            let ready: Vec<&str> = self
                .steps
                .iter()
                .filter(|s| !done.contains(s.id.as_str()))
                .filter(|s| s.depends_on.iter().all(|d| done.contains(d.as_str())))
                .map(|s| s.id.as_str())
                .collect();
            if ready.is_empty() {
                break;
            }
            done.extend(ready);
        }
        if done.len() == self.steps.len() {
            Ok(())
        } else {
            Err(GraphError::Cycle)
        }
    }

    /// The steps that can start now, given the ones finished (in the order written).
    pub fn ready<'a>(
        &'a self,
        finished: &std::collections::HashSet<String>,
        started: &std::collections::HashSet<String>,
    ) -> Vec<&'a TaskStep> {
        self.steps
            .iter()
            .filter(|s| !finished.contains(&s.id) && !started.contains(&s.id))
            .filter(|s| s.depends_on.iter().all(|d| finished.contains(d)))
            .collect()
    }

    /// Whether any step calls a brain.
    pub fn uses_ai(&self) -> bool {
        self.steps.iter().any(|s| s.action.uses_ai())
    }

    /// One step after another, each waiting for the one before (a routine without parallel
    /// groups, a plan written as a list).
    pub fn chain(mut self) -> Self {
        let mut previous: Option<String> = None;
        for s in &mut self.steps {
            if s.depends_on.is_empty()
                && let Some(p) = &previous
            {
                s.depends_on.push(p.clone());
            }
            previous = Some(s.id.clone());
        }
        self
    }
}

/// Fills `${name}` in every string of `args` from `vars`; a string that is only `${name}` takes
/// the variable's own type (a number stays a number).
pub fn fill_variables(args: &Value, vars: &serde_json::Map<String, Value>) -> Value {
    match args {
        Value::String(s) => {
            if let Some(name) = s.strip_prefix("${").and_then(|r| r.strip_suffix('}'))
                && let Some(v) = vars.get(name)
            {
                return v.clone();
            }
            let mut out = s.clone();
            for (name, v) in vars {
                let text = match v {
                    Value::String(t) => t.clone(),
                    other => other.to_string(),
                };
                out = out.replace(&format!("${{{name}}}"), &text);
            }
            Value::String(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(|v| fill_variables(v, vars)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), fill_variables(v, vars)))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn step(id: &str, deps: &[&str]) -> TaskStep {
        TaskStep {
            id: id.into(),
            title: id.into(),
            action: StepAction::Delay { ms: 1 },
            depends_on: deps.iter().map(|d| (*d).to_owned()).collect(),
            on_error: OnError::Stop,
            delay_ms: None,
            confirm: false,
        }
    }

    fn spec(steps: Vec<TaskStep>) -> TaskSpec {
        TaskSpec {
            title: "t".into(),
            kind: TaskKind::Plan,
            owner: "you".into(),
            steps,
            success: Vec::new(),
            grants: Vec::new(),
            timeout_ms: None,
            notify: Notify::Speak,
            routine_id: None,
            turn_id: None,
            cwd: None,
        }
    }

    #[test]
    fn graphs_are_checked_and_independent_steps_are_ready_together() {
        let s = spec(vec![step("a", &[]), step("b", &[]), step("c", &["a", "b"])]);
        assert_eq!(s.validate(), Ok(()));
        let none = std::collections::HashSet::new();
        let ready: Vec<&str> = s
            .ready(&none, &none)
            .iter()
            .map(|s| s.id.as_str())
            .collect();
        assert_eq!(ready, ["a", "b"]);
        let finished = ["a".to_owned(), "b".to_owned()].into_iter().collect();
        let ready: Vec<&str> = s
            .ready(&finished, &none)
            .iter()
            .map(|s| s.id.as_str())
            .collect();
        assert_eq!(ready, ["c"]);
        assert_eq!(
            spec(vec![step("a", &["b"]), step("b", &["a"])]).validate(),
            Err(GraphError::Cycle)
        );
        assert!(matches!(
            spec(vec![step("a", &["x"])]).validate(),
            Err(GraphError::Missing { .. })
        ));
        assert!(matches!(
            spec(vec![step("a", &[]), step("a", &[])]).validate(),
            Err(GraphError::Duplicate(_))
        ));
        assert_eq!(spec(vec![]).validate(), Err(GraphError::Empty));
    }

    #[test]
    fn a_chain_runs_one_after_another() {
        let s = spec(vec![step("a", &[]), step("b", &[]), step("c", &[])]).chain();
        assert_eq!(s.steps[1].depends_on, ["a"]);
        assert_eq!(s.steps[2].depends_on, ["b"]);
    }

    #[test]
    fn variables_fill_text_and_keep_numbers() {
        let vars = json!({ "minutes": 25, "name": "Ada" });
        let vars = vars.as_object().unwrap();
        assert_eq!(
            fill_variables(
                &json!({ "m": "${minutes}", "label": "Focus ${minutes} min for ${name}", "x": [1, "${name}"] }),
                vars
            ),
            json!({ "m": 25, "label": "Focus 25 min for Ada", "x": [1, "Ada"] })
        );
    }

    #[test]
    fn statuses_round_trip_and_know_what_is_active() {
        for s in [
            TaskStatus::Pending,
            TaskStatus::Waiting,
            TaskStatus::NeedsYou,
            TaskStatus::Interrupted,
        ] {
            assert_eq!(TaskStatus::parse(s.as_str()), Some(s));
        }
        assert!(TaskStatus::Waiting.is_active() && !TaskStatus::Paused.is_active());
        assert!(TaskStatus::Interrupted.is_final());
        let w = serde_json::to_value(WatchSpec::ProcessExit {
            pid: None,
            name: Some("cargo".into()),
        })
        .unwrap();
        assert_eq!(
            w,
            json!({ "type": "processExit", "pid": null, "name": "cargo" })
        );
    }
}
