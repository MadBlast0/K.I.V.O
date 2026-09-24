//! Drafting routines by voice or chat (ROUT-13): "create a routine called work mode that opens VS
//! Code and Slack" goes to a brain, which calls `routines.draft` with the steps. The draft opens in
//! the builder for the user to review; nothing is saved or run until they do, and saving shows the
//! permissions it needs (ROUT-03).

use crate::routines::{DRAFT_TOOL, Routines};
use kivo_core::Capability;
use kivo_core::routine::{Routine, RoutineStep, Trigger};
use kivo_core::task::{OnError, StepAction};
use kivo_core::text;
use kivo_core::tool::{
    CapabilityTier, Platform, Reversibility, Risk, SideEffect, ToolError, ToolErrorCode, ToolSpec,
};
use kivo_tools::{Output, Tool};
use serde_json::{Value, json};
use std::sync::{Arc, OnceLock, Weak};

/// The routines, set once they exist (they need the registry this tool lives in).
pub type RoutineHandle = Arc<OnceLock<Weak<Routines>>>;

pub struct Draft {
    spec: ToolSpec,
    handle: RoutineHandle,
}

impl Draft {
    pub fn new(handle: RoutineHandle) -> Self {
        Self {
            spec: ToolSpec {
                id: DRAFT_TOOL.into(),
                title: text::t("tool.routines.draft"),
                description: "Draft a routine (a named list of steps the user can run with a phrase) for the user to review in KIVO's routine builder. It is not saved or run: the user checks it, sees the permissions it needs, and saves it. Each step is either a tool call ({\"tool\": id, \"args\": {...}}) using KIVO's tools, or something to say ({\"say\": text}).".into(),
                params: json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "A short name, like Work mode" },
                        "description": { "type": "string" },
                        "phrases": { "type": "array", "items": { "type": "string" }, "description": "What the user says to run it" },
                        "steps": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "tool": { "type": "string" },
                                    "args": { "type": "object" },
                                    "say": { "type": "string" }
                                }
                            }
                        }
                    },
                    "required": ["name", "steps"]
                }),
                result: json!({ "type": "object" }),
                risk: Risk::Safe,
                side_effects: vec![SideEffect::None],
                data_egress: false,
                timeout_ms: 5_000,
                cancellable: false,
                tier: CapabilityTier::Native,
                platforms: vec![Platform::Windows, Platform::MacOs, Platform::Linux],
                reversibility: Reversibility::NotApplicable,
                capability: Capability::Routines,
            },
            handle,
        }
    }
}

/// The routine a brain described, or why it can't be one.
pub fn from_args(args: &Value, known: impl Fn(&str) -> bool) -> Result<Routine, String> {
    let name = args["name"].as_str().map(str::trim).unwrap_or_default();
    if name.is_empty() {
        return Err(text::t("routine.noName"));
    }
    let mut steps = Vec::new();
    for (i, s) in args["steps"].as_array().into_iter().flatten().enumerate() {
        let action = if let Some(tool) = s["tool"].as_str() {
            if !known(tool) || tool.starts_with("session.") || tool == DRAFT_TOOL {
                return Err(text::tf("routine.importUnknownTool", &[("tool", &tool)]));
            }
            StepAction::Tool {
                tool: tool.to_owned(),
                args: if s["args"].is_object() {
                    s["args"].clone()
                } else {
                    json!({})
                },
            }
        } else if let Some(say) = s["say"].as_str().filter(|t| !t.trim().is_empty()) {
            StepAction::Say {
                text: say.trim().to_owned(),
            }
        } else {
            continue;
        };
        steps.push(RoutineStep {
            id: format!("s{}", i + 1),
            action,
            on_error: OnError::Stop,
            delay_ms: None,
            parallel_group: None,
            confirm: false,
        });
    }
    if steps.is_empty() {
        return Err(text::t("routine.noSteps"));
    }
    let phrases: Vec<String> = args["phrases"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|p| p.trim().to_lowercase())
        .filter(|p| !p.is_empty())
        .take(5)
        .collect();
    Ok(Routine {
        id: String::new(),
        name: name.chars().take(60).collect(),
        description: args["description"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_owned(),
        enabled: false,
        triggers: if phrases.is_empty() {
            vec![Trigger::Manual]
        } else {
            vec![Trigger::Phrase {
                phrases,
                lang: "en".into(),
            }]
        },
        steps,
        variables: Vec::new(),
        grants: Vec::new(),
        starter: None,
    })
}

impl Tool for Draft {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn run(&self, args: &Value) -> Result<Output, ToolError> {
        let routines = self
            .handle
            .get()
            .and_then(Weak::upgrade)
            .ok_or_else(|| ToolError::new(ToolErrorCode::Failed, text::t("task.notReady")))?;
        let draft = from_args(args, |tool| routines.knows_tool(tool))
            .map_err(|e| ToolError::new(ToolErrorCode::InvalidArgs, e))?;
        let name = draft.name.clone();
        routines.offer_draft(draft);
        Ok(Output {
            say: text::tf("routine.drafted", &[("name", &name)]),
            data: json!({ "drafted": name }),
            ..Output::default()
        })
    }
}

pub fn tools(handle: &RoutineHandle) -> Vec<Arc<dyn Tool>> {
    vec![Arc::new(Draft::new(Arc::clone(handle)))]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_brains_description_becomes_a_draft_and_nothing_more() {
        let known = |t: &str| ["apps.launch", "audio.volume_set"].contains(&t);
        let r = from_args(
            &json!({
                "name": "Work mode",
                "phrases": ["Work Mode", ""],
                "steps": [
                    { "tool": "apps.launch", "args": { "app": "Slack" } },
                    { "say": "Ready." },
                    { "nothing": true }
                ]
            }),
            known,
        )
        .unwrap();
        assert!(!r.enabled && r.grants.is_empty() && r.id.is_empty());
        assert_eq!(r.steps.len(), 2);
        assert_eq!(r.phrases().collect::<Vec<_>>(), ["work mode"]);
        assert!(
            from_args(
                &json!({ "name": "x", "steps": [{ "tool": "shell.rm_rf" }] }),
                known
            )
            .is_err()
        );
        assert!(
            from_args(
                &json!({ "name": "x", "steps": [{ "tool": "session.enable" }] }),
                |_| true
            )
            .is_err()
        );
        assert!(from_args(&json!({ "name": "", "steps": [{ "say": "hi" }] }), known).is_err());
        assert!(from_args(&json!({ "name": "x", "steps": [] }), known).is_err());
    }
}
