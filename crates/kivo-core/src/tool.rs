//! The tool schema (TOOLS_AND_CONTROL §1, TOOL-01): what every tool declares about itself, what a
//! call carries, and what comes back. The permission engine decides on these declarations, never
//! on the tool's own say-so at run time.

use crate::capability::Capability;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Plan §56. May be raised for specific arguments (SECURITY §3).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Risk {
    Safe,
    Low,
    Medium,
    High,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SideEffect {
    None,
    LocalRead,
    LocalWrite,
    Destructive,
    ExternalComms,
    Financial,
    SecuritySensitive,
}

/// The capability ladder (plan §45): native methods first, raw input last.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CapabilityTier {
    Native,
    OsApi,
    AppCli,
    Uia,
    BrowserDom,
    A11y,
    Vision,
    Input,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Platform {
    Windows,
    MacOs,
    Linux,
}

/// How a tool's effect can be taken back (UX §8.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Reversibility {
    /// Nothing to undo (reads, launching an app).
    NotApplicable,
    /// The tool has an undo handler.
    Undoable,
    /// Can't be taken back; confirms up front in Ask, Accept edits, Plan and Auto.
    Irreversible,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSpec {
    /// Namespaced: `system.*`, `apps.*`, `windows.*`, `media.*`, `screen.*`, `browser.*`, …
    pub id: String,
    /// Short and model-facing.
    pub description: String,
    /// What it does, in the user's words, with `{arg}` placeholders ("Open {app}").
    pub title: String,
    /// JSON Schema of the arguments.
    pub params: Value,
    /// JSON Schema of the result.
    pub result: Value,
    pub risk: Risk,
    pub side_effects: Vec<SideEffect>,
    /// Sends data off the device.
    pub data_egress: bool,
    pub timeout_ms: u64,
    pub cancellable: bool,
    pub tier: CapabilityTier,
    pub platforms: Vec<Platform>,
    pub reversibility: Reversibility,
    /// The capability that must be on for this tool to exist at all (CAP-01).
    pub capability: Capability,
}

/// Who asked for a call (SECURITY §2 `initiated_by`).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Initiator {
    /// The user's own words, through the fast path or a button.
    UserDirect,
    Brain,
    Task,
    Mcp,
}

/// Where a value came from (SECURITY §4).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Provenance {
    User,
    System,
    Untrusted,
}

/// Something a call acts on, checked against the hard limits (blocked apps, destination binding).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Target {
    App {
        id: String,
        name: String,
    },
    Window {
        title: String,
        app_id: String,
    },
    Destination {
        address: String,
        provenance: Provenance,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    /// Unique per call (for the permit and the audit row).
    pub id: String,
    pub tool: String,
    pub args: Value,
    pub initiated_by: Initiator,
    pub targets: Vec<Target>,
}

/// A failure the user can understand (plan §146). `code` is for machines and the log; `message`
/// is safe to show and speak; `detail` (raw OS errors) is logged only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[error("{message}")]
pub struct ToolError {
    pub code: ToolErrorCode,
    pub message: String,
    #[serde(skip)]
    pub detail: Option<String>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolErrorCode {
    NotFound,
    InvalidArgs,
    Unsupported,
    AccessDenied,
    Timeout,
    Cancelled,
    Failed,
}

impl ToolError {
    pub fn new(code: ToolErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// What a finished call produced.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    pub status: Result<Value, ToolError>,
    /// Where any data in the result came from (tool outputs from apps and the web are untrusted).
    pub provenance: Provenance,
    pub duration_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn risk_orders_from_safe_to_high() {
        assert!(Risk::Safe < Risk::Low && Risk::Low < Risk::Medium && Risk::Medium < Risk::High);
    }

    #[test]
    fn errors_show_the_message_and_keep_the_os_detail_out_of_json() {
        let e = ToolError::new(ToolErrorCode::NotFound, "Chrome isn't installed")
            .with_detail("HRESULT 0x80070002");
        assert_eq!(e.to_string(), "Chrome isn't installed");
        let json = serde_json::to_string(&e).unwrap();
        assert!(!json.contains("0x8007"), "{json}");
    }
}
