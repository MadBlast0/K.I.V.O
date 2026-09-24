//! `computer.use` as a brain sees it (CAPABILITIES §4, CAP-09): the last resort after the native,
//! UI Automation and browser tools. Only offered when Computer use is on; the user confirms each
//! task with its estimated cost, and the engine runs it step by step (`engine::computer`).

use kivo_core::Capability;
use kivo_core::text;
use kivo_core::tool::{
    CapabilityTier, Platform, Reversibility, Risk, SideEffect, ToolError, ToolErrorCode, ToolSpec,
};
use kivo_tools::{Output, Tool};
use serde_json::{Value, json};
use std::sync::Arc;

pub const ID: &str = "computer.use";

pub struct ComputerUse {
    spec: ToolSpec,
}

impl Default for ComputerUse {
    fn default() -> Self {
        Self {
            spec: ToolSpec {
                id: ID.into(),
                title: text::t("tool.computer.use"),
                description: "LAST RESORT: operate an app by looking at the screen and clicking and typing, when no other tool (app commands, UI Automation, the browser tools) can do it. Give the task in plain words and the app it happens in. The user confirms the task and its cost first; each step is checked, and it stops at the step, cost and time limits.".into(),
                params: json!({
                    "type": "object",
                    "properties": {
                        "task": { "type": "string", "description": "What to do, in plain words" },
                        "app": { "type": "string", "description": "The app it happens in" }
                    },
                    "required": ["task", "app"]
                }),
                result: json!({ "type": "object" }),
                risk: Risk::High,
                side_effects: vec![SideEffect::LocalWrite, SideEffect::ExternalComms],
                // Screenshots go to the computer-use model.
                data_egress: true,
                timeout_ms: 600_000,
                cancellable: true,
                tier: CapabilityTier::Input,
                platforms: vec![Platform::Windows],
                reversibility: Reversibility::Irreversible,
                capability: Capability::ComputerUse,
            },
        }
    }
}

impl Tool for ComputerUse {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn run(&self, _args: &Value) -> Result<Output, ToolError> {
        // The turn engine runs it; a direct call (a routine, MCP) isn't allowed.
        Err(ToolError::new(
            ToolErrorCode::Unsupported,
            text::t("computer.onlyInTurn"),
        ))
    }
}

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![Arc::new(ComputerUse::default())]
}
