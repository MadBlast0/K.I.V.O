//! Routines and custom commands (ROUTINES §5): a named list of steps with triggers, compiled to a
//! task graph when it runs. Stored as a versioned JSON body.

use crate::task::{OnError, StepAction, TaskGrant, TaskKind, TaskSpec, TaskStep};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The routine body's schema version; bumped (with a migration of the JSON) when it changes.
pub const ROUTINE_VERSION: u32 = 1;

/// What starts a routine (ROUTINES §1).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum Trigger {
    /// Spoken or typed phrases, with `{variables}` ("focus for {minutes}").
    Phrase { phrases: Vec<String>, lang: String },
    /// A global hotkey ("Ctrl+Alt+W").
    Hotkey { chord: String },
    /// Only from the Routines page, the palette or another routine.
    Manual,
    /// A cron-like schedule (`30 7 * * mon-fri`, `@daily`), in a time zone (the PC's own when
    /// none is named; ROUT-11).
    Schedule {
        cron: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tz: Option<String>,
    },
    /// Something happening on the PC (ROUT-11). Runs unattended, so High-risk steps ask on screen
    /// first (ROUT-12).
    Event { event: RoutineEvent },
}

/// The events a routine can start on (ROUTINES §1).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum RoutineEvent {
    /// An app starts (its program or its name: "Spotify", "code").
    AppLaunched { app: String },
    /// A USB device is plugged in: any, or one whose name contains `name`.
    UsbDevice {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// The PC joins a network (a Wi-Fi network's name).
    Network { name: String },
    /// A time of day (`07:30`), on some weekdays (0 = Sunday; none = every day).
    TimeOfDay {
        time: String,
        #[serde(default)]
        days: Vec<u8>,
    },
    /// No keyboard or mouse input for this long.
    Idle { minutes: u32 },
    /// The user is back after being away at least this long.
    Return { away_minutes: u32 },
    /// Running on battery, it drops to this percentage.
    BatteryLow { percent: u8 },
    /// Another routine finished.
    AfterRoutine { routine: String },
}

impl RoutineEvent {
    /// A time of day as a schedule (`30 7 * * 1,2,3`), or why it isn't one.
    pub fn schedule(&self) -> Option<Result<String, String>> {
        let Self::TimeOfDay { time, days } = self else {
            return None;
        };
        let parsed = time.split_once(':').and_then(|(h, m)| {
            let (h, m) = (h.trim().parse::<u8>().ok()?, m.trim().parse::<u8>().ok()?);
            (h < 24 && m < 60).then_some((h, m))
        });
        Some(match parsed {
            Some((h, m)) => {
                let days = if days.is_empty() {
                    "*".to_owned()
                } else {
                    days.iter().map(u8::to_string).collect::<Vec<_>>().join(",")
                };
                Ok(format!("{m} {h} * * {days}"))
            }
            None => Err(format!("“{time}” isn't a time of day (like 07:30)")),
        })
    }
}

/// The type of a phrase variable.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VarKind {
    #[default]
    Text,
    Number,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VarDef {
    pub name: String,
    #[serde(default)]
    pub kind: VarKind,
}

/// One step as the user wrote it.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineStep {
    pub id: String,
    pub action: StepAction,
    #[serde(default)]
    pub on_error: OnError,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub delay_ms: Option<u64>,
    /// Steps in the same group run together; the next group waits for the whole group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel_group: Option<String>,
    /// Asks the user before this step ("Lock the PC?"); a no skips it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirm: bool,
}

/// A routine (ROUT-01). A custom command (ROUT-06) is a routine with one phrase and one tool step.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Routine {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub enabled: bool,
    pub triggers: Vec<Trigger>,
    pub steps: Vec<RoutineStep>,
    #[serde(default)]
    pub variables: Vec<VarDef>,
    /// What the user granted when saving it (ROUT-03): each step's tool with its exact
    /// arguments.
    #[serde(default)]
    pub grants: Vec<TaskGrant>,
    /// A starter routine KIVO ships (ROUT-10): its key, so it isn't added twice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starter: Option<String>,
}

impl Routine {
    /// A custom command: one phrase, one tool call, no AI (ROUT-06).
    pub fn is_custom_command(&self) -> bool {
        self.steps.len() == 1
            && matches!(self.steps[0].action, StepAction::Tool { .. })
            && self
                .triggers
                .iter()
                .all(|t| matches!(t, Trigger::Phrase { .. }))
    }

    pub fn contains_ai(&self) -> bool {
        self.steps.iter().any(|s| s.action.uses_ai())
    }

    pub fn phrases(&self) -> impl Iterator<Item = &str> {
        self.triggers.iter().flat_map(|t| match t {
            Trigger::Phrase { phrases, .. } => phrases.iter().map(String::as_str).collect(),
            _ => Vec::new(),
        })
    }

    /// Whether something other than the user starts it (a schedule or an event): such runs are
    /// unattended (ROUT-12).
    pub fn is_unattended(&self) -> bool {
        self.triggers
            .iter()
            .any(|t| matches!(t, Trigger::Schedule { .. } | Trigger::Event { .. }))
    }

    /// Checks the schedules and times of day; the error says which is wrong.
    pub fn check_triggers(&self) -> Result<(), String> {
        for t in &self.triggers {
            match t {
                Trigger::Schedule { cron, tz } => {
                    crate::cron::next_fire(cron, tz.as_deref(), 0)?;
                }
                Trigger::Event { event } => {
                    if let Some(Err(e)) = event.schedule() {
                        return Err(e);
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub fn hotkeys(&self) -> impl Iterator<Item = &str> {
        self.triggers.iter().filter_map(|t| match t {
            Trigger::Hotkey { chord } => Some(chord.as_str()),
            _ => None,
        })
    }

    /// The grants the routine needs: every tool step with its arguments as written (variables
    /// left as `${name}`), for the list shown when saving (ROUT-03).
    pub fn needed_grants(&self) -> Vec<TaskGrant> {
        let mut out: Vec<TaskGrant> = Vec::new();
        for s in &self.steps {
            if let StepAction::Tool { tool, args } = &s.action {
                let g = TaskGrant {
                    tool: tool.clone(),
                    args: phrase_vars_to_template(args),
                };
                if !out.contains(&g) {
                    out.push(g);
                }
            }
        }
        out
    }

    /// Whether the saved grants still cover every tool step (an edit adds or changes a step:
    /// the routine must be granted again before it runs).
    pub fn is_granted(&self) -> bool {
        self.needed_grants().iter().all(|g| self.grants.contains(g))
    }

    /// The task graph for one run (ROUT-02): steps in order, a parallel group's steps side by
    /// side, variables filled in.
    pub fn compile(&self, vars: &serde_json::Map<String, Value>) -> TaskSpec {
        let mut steps: Vec<TaskStep> = Vec::new();
        // The steps the next group waits for.
        let mut previous: Vec<String> = Vec::new();
        let mut i = 0;
        while i < self.steps.len() {
            let group = self.steps[i].parallel_group.clone();
            let mut j = i + 1;
            if group.is_some() {
                while j < self.steps.len() && self.steps[j].parallel_group == group {
                    j += 1;
                }
            }
            let mut this_group = Vec::new();
            for s in &self.steps[i..j] {
                let action = fill_action(&s.action, vars);
                steps.push(TaskStep {
                    id: s.id.clone(),
                    title: action_title(&action),
                    action,
                    depends_on: previous.clone(),
                    on_error: s.on_error,
                    delay_ms: s.delay_ms,
                    confirm: s.confirm,
                });
                this_group.push(s.id.clone());
            }
            previous = this_group;
            i = j;
        }
        TaskSpec {
            title: self.name.clone(),
            kind: TaskKind::Routine,
            owner: self.name.clone(),
            steps,
            success: Vec::new(),
            grants: self.grants.clone(),
            timeout_ms: None,
            notify: crate::task::Notify::Silent,
            routine_id: Some(self.id.clone()),
            turn_id: None,
            cwd: None,
        }
    }
}

/// `{minutes}` (the phrase's notation) and `${minutes}` both mean a variable in arguments.
fn phrase_vars_to_template(args: &Value) -> Value {
    match args {
        Value::String(s) => {
            let mut out = String::new();
            let mut rest = s.as_str();
            while let Some(start) = rest.find('{') {
                let escaped = start > 0 && rest.as_bytes()[start - 1] == b'$';
                match rest[start..].find('}') {
                    Some(end)
                        if !escaped
                            && rest[start + 1..start + end]
                                .chars()
                                .all(|c| c.is_alphanumeric() || c == '_')
                            && end > 1 =>
                    {
                        out.push_str(&rest[..start]);
                        out.push('$');
                        out.push_str(&rest[start..=start + end]);
                        rest = &rest[start + end + 1..];
                    }
                    _ => {
                        out.push_str(&rest[..=start]);
                        rest = &rest[start + 1..];
                    }
                }
            }
            out.push_str(rest);
            Value::String(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(phrase_vars_to_template).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), phrase_vars_to_template(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn fill_action(action: &StepAction, vars: &serde_json::Map<String, Value>) -> StepAction {
    let fill_text = |s: &str| match crate::task::fill_variables(
        &phrase_vars_to_template(&Value::String(s.to_owned())),
        vars,
    ) {
        Value::String(t) => t,
        other => other.to_string(),
    };
    match action {
        StepAction::Tool { tool, args } => StepAction::Tool {
            tool: tool.clone(),
            args: crate::task::fill_variables(&phrase_vars_to_template(args), vars),
        },
        StepAction::Say { text } => StepAction::Say {
            text: fill_text(text),
        },
        StepAction::Ask { prompt, profile } => StepAction::Ask {
            prompt: fill_text(prompt),
            profile: profile.clone(),
        },
        StepAction::Decide {
            question,
            options,
            profile,
        } => StepAction::Decide {
            question: fill_text(question),
            options: options.clone(),
            profile: profile.clone(),
        },
        StepAction::Agent { prompt, agent, cwd } => StepAction::Agent {
            prompt: fill_text(prompt),
            agent: agent.clone(),
            cwd: cwd.clone(),
        },
        other => other.clone(),
    }
}

/// A short title for a step, before the runtime renders the tool's own title.
fn action_title(action: &StepAction) -> String {
    match action {
        StepAction::Tool { tool, .. } => tool.clone(),
        StepAction::Watch { .. } => "watch".into(),
        StepAction::Say { text } => text.clone(),
        StepAction::Ask { prompt, .. } => prompt.clone(),
        StepAction::Decide { question, .. } => question.clone(),
        StepAction::Agent { prompt, .. } => prompt.clone(),
        StepAction::Verify { .. } => "verify".into(),
        StepAction::Delay { ms } => format!("wait {ms} ms"),
    }
}

/// Matches `said` against a phrase with `{variables}`; the variables' values on a match. Case,
/// punctuation and extra spaces don't matter; a `Number` variable must be a number (digits or a
/// number word up to twenty).
pub fn match_phrase(
    phrase: &str,
    said: &str,
    variables: &[VarDef],
) -> Option<serde_json::Map<String, Value>> {
    let norm = |s: &str| -> Vec<String> {
        s.to_lowercase()
            .split(|c: char| c.is_whitespace())
            .map(|w| {
                w.trim_matches(|c: char| !c.is_alphanumeric() && c != '{' && c != '}' && c != '_')
                    .to_owned()
            })
            .filter(|w| !w.is_empty())
            .collect()
    };
    let p = norm(phrase);
    let mut s = norm(said);
    // "Kivo, …" and a trailing "please" are not part of the phrase.
    if s.first().is_some_and(|w| w == "kivo" || w == "hey") {
        s.remove(0);
        if s.first().is_some_and(|w| w == "kivo") {
            s.remove(0);
        }
    }
    if s.last().is_some_and(|w| w == "please") {
        s.pop();
    }
    let mut vars = serde_json::Map::new();
    if match_words(&p, &s, variables, &mut vars) {
        Some(vars)
    } else {
        None
    }
}

fn match_words(
    p: &[String],
    s: &[String],
    variables: &[VarDef],
    vars: &mut serde_json::Map<String, Value>,
) -> bool {
    let Some(first) = p.first() else {
        return s.is_empty();
    };
    if let Some(name) = first.strip_prefix('{').and_then(|r| r.strip_suffix('}')) {
        let kind = variables
            .iter()
            .find(|v| v.name == name)
            .map_or(VarKind::Text, |v| v.kind);
        // A variable takes one word or more (the fewest that let the rest match).
        for take in 1..=s.len() {
            let words = s[..take].join(" ");
            let value = match kind {
                VarKind::Number if take == 1 => match number(&words) {
                    Some(n) => Value::from(n),
                    None => continue,
                },
                VarKind::Number => continue,
                VarKind::Text => Value::String(words),
            };
            let mut attempt = vars.clone();
            attempt.insert(name.to_owned(), value);
            if match_words(&p[1..], &s[take..], variables, &mut attempt) {
                *vars = attempt;
                return true;
            }
        }
        return false;
    }
    s.first() == Some(first) && match_words(&p[1..], &s[1..], variables, vars)
}

fn number(word: &str) -> Option<i64> {
    if let Ok(n) = word.parse::<i64>() {
        return Some(n);
    }
    const WORDS: [&str; 21] = [
        "zero",
        "one",
        "two",
        "three",
        "four",
        "five",
        "six",
        "seven",
        "eight",
        "nine",
        "ten",
        "eleven",
        "twelve",
        "thirteen",
        "fourteen",
        "fifteen",
        "sixteen",
        "seventeen",
        "eighteen",
        "nineteen",
        "twenty",
    ];
    WORDS
        .iter()
        .position(|w| *w == word)
        .and_then(|i| i64::try_from(i).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn focus() -> Routine {
        Routine {
            id: "r1".into(),
            name: "Focus".into(),
            description: String::new(),
            enabled: true,
            triggers: vec![Trigger::Phrase {
                phrases: vec![
                    "focus for {minutes}".into(),
                    "focus for {minutes} minutes".into(),
                ],
                lang: "en".into(),
            }],
            steps: vec![
                RoutineStep {
                    id: "a".into(),
                    action: StepAction::Tool {
                        tool: "system.focus_on".into(),
                        args: json!({}),
                    },
                    on_error: OnError::Continue,
                    delay_ms: None,
                    parallel_group: Some("g".into()),
                    confirm: false,
                },
                RoutineStep {
                    id: "b".into(),
                    action: StepAction::Tool {
                        tool: "media.pause".into(),
                        args: json!({}),
                    },
                    on_error: OnError::Stop,
                    delay_ms: None,
                    parallel_group: Some("g".into()),
                    confirm: false,
                },
                RoutineStep {
                    id: "c".into(),
                    action: StepAction::Tool {
                        tool: "timer.start".into(),
                        args: json!({ "minutes": "{minutes}", "label": "Focus {minutes} min" }),
                    },
                    on_error: OnError::Stop,
                    delay_ms: None,
                    parallel_group: None,
                    confirm: false,
                },
            ],
            variables: vec![VarDef {
                name: "minutes".into(),
                kind: VarKind::Number,
            }],
            grants: Vec::new(),
            starter: None,
        }
    }

    #[test]
    fn phrases_match_with_variables_and_ignore_the_name() {
        let r = focus();
        let p = "focus for {minutes} minutes";
        assert_eq!(
            match_phrase(p, "Kivo, focus for 25 minutes.", &r.variables),
            Some(json!({ "minutes": 25 }).as_object().unwrap().clone())
        );
        assert_eq!(
            match_phrase(p, "focus for twenty minutes please", &r.variables).unwrap()["minutes"],
            20
        );
        assert!(match_phrase(p, "focus for many minutes", &r.variables).is_none());
        assert!(match_phrase("cinema", "cinema time", &[]).is_none());
        assert_eq!(
            match_phrase("open {thing} now", "open my big project now", &[]).unwrap()["thing"],
            "my big project"
        );
    }

    #[test]
    fn a_routine_compiles_to_groups_in_order_with_variables_filled() {
        let mut r = focus();
        r.grants = r.needed_grants();
        let vars = json!({ "minutes": 25 });
        let task = r.compile(vars.as_object().unwrap());
        assert_eq!(task.validate(), Ok(()));
        assert!(task.steps[0].depends_on.is_empty() && task.steps[1].depends_on.is_empty());
        assert_eq!(task.steps[2].depends_on, ["a", "b"]);
        assert_eq!(
            task.steps[2].action,
            StepAction::Tool {
                tool: "timer.start".into(),
                args: json!({ "minutes": 25, "label": "Focus 25 min" })
            }
        );
        assert_eq!(task.routine_id.as_deref(), Some("r1"));
        // The grant keeps the variable as a variable.
        assert_eq!(
            r.grants[2].args,
            json!({ "minutes": "${minutes}", "label": "Focus ${minutes} min" })
        );
        assert!(r.is_granted());
        r.steps[2].action = StepAction::Tool {
            tool: "timer.start".into(),
            args: json!({ "minutes": 999 }),
        };
        assert!(!r.is_granted(), "an edited step needs granting again");
    }

    #[test]
    fn custom_commands_and_ai_badges() {
        let mut r = focus();
        assert!(!r.is_custom_command() && !r.contains_ai());
        r.steps.truncate(1);
        assert!(r.is_custom_command());
        r.steps.push(RoutineStep {
            id: "d".into(),
            action: StepAction::Ask {
                prompt: "Summarize my day".into(),
                profile: Some("Fast".into()),
            },
            on_error: OnError::Stop,
            delay_ms: None,
            parallel_group: None,
            confirm: false,
        });
        assert!(r.contains_ai() && !r.is_custom_command());
    }
}
