//! A server's tool as a KIVO tool (TOOL-35): `mcp.<server>.<tool>`, Medium risk unless the user
//! chose otherwise, data egress for remote servers, and every call through the permission engine
//! like any other tool. What a server says (its description, its results) is untrusted: the
//! description is cut and labelled as coming from the server, the card's title never uses the
//! server's words, and results taint the turn (SECURITY §4).

use crate::client::{Connection, MAX_DESCRIPTION_CHARS, McpError, RemoteTool, clip};
use crate::config::{McpServer, ToolSetting, tool_id};
use kivo_core::Capability;
use kivo_core::text;
use kivo_core::tool::{
    CapabilityTier, Platform, Reversibility, Risk, SideEffect, ToolError, ToolErrorCode, ToolSpec,
};
use kivo_tools::{Output, Tool};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// How long a server's tool may run.
const CALL_TIMEOUT: Duration = Duration::from_secs(60);

/// The spec KIVO gives a server's tool.
pub fn spec_for(
    server: &McpServer,
    tool: &RemoteTool,
    setting: Option<&ToolSetting>,
    capability: Capability,
) -> ToolSpec {
    let (description, _) = clip(&tool.description, MAX_DESCRIPTION_CHARS);
    // The input schema must be an object schema; anything else takes no arguments.
    let params = if tool.schema.get("type").and_then(Value::as_str) == Some("object") {
        tool.schema.clone()
    } else {
        json!({ "type": "object", "properties": {} })
    };
    let id = tool_id(&server.id, &tool.name);
    let short = id.rsplit('.').next().unwrap_or_default().to_owned();
    ToolSpec {
        description: text::tf(
            "tool.mcp.description",
            &[("server", &server.name), ("description", &description)],
        ),
        // KIVO's words on the card, never the server's: "Use create_issue (github)".
        title: text::tf(
            "tool.mcp.title",
            &[("tool", &short), ("server", &server.name)],
        ),
        id,
        params,
        result: json!({ "type": "object" }),
        risk: setting.and_then(|s| s.risk).unwrap_or(Risk::Medium),
        side_effects: if server.remote() {
            vec![SideEffect::ExternalComms]
        } else {
            vec![SideEffect::LocalWrite]
        },
        data_egress: server.remote(),
        timeout_ms: u64::try_from(CALL_TIMEOUT.as_millis()).unwrap_or(60_000),
        cancellable: true,
        tier: CapabilityTier::AppCli,
        platforms: vec![Platform::Windows, Platform::MacOs, Platform::Linux],
        reversibility: Reversibility::NotApplicable,
        capability,
    }
}

/// A server's tool, callable through the registry.
pub struct McpTool {
    spec: ToolSpec,
    remote_name: String,
    server_name: String,
    conn: Arc<Connection>,
    handle: tokio::runtime::Handle,
}

impl McpTool {
    pub fn new(
        spec: ToolSpec,
        remote_name: String,
        server_name: String,
        conn: Arc<Connection>,
        handle: tokio::runtime::Handle,
    ) -> Self {
        Self {
            spec,
            remote_name,
            server_name,
            conn,
            handle,
        }
    }
}

impl Tool for McpTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn run(&self, args: &Value) -> Result<Output, ToolError> {
        self.run_cancellable(args, &CancellationToken::new())
    }

    fn run_cancellable(
        &self,
        args: &Value,
        cancel: &CancellationToken,
    ) -> Result<Output, ToolError> {
        let timeout = Duration::from_millis(self.spec.timeout_ms);
        let outcome = self
            .handle
            .block_on(self.conn.call(&self.remote_name, args, timeout, cancel))
            .map_err(|e| match e {
                McpError::Timeout => ToolError::new(
                    ToolErrorCode::Timeout,
                    text::tf("error.mcp.timeout", &[("server", &self.server_name)]),
                ),
                McpError::Cancelled => {
                    ToolError::new(ToolErrorCode::Cancelled, text::t("reply.cancelled"))
                }
                other => ToolError::new(
                    ToolErrorCode::Failed,
                    text::tf("error.mcp.failed", &[("server", &self.server_name)]),
                )
                .with_detail(other.to_string()),
            })?;
        let source = text::tf("mcp.source", &[("server", &self.server_name)]);
        if outcome.is_error {
            // The server's own words stay data, never the spoken message.
            return Err(ToolError::new(
                ToolErrorCode::Failed,
                text::tf("error.mcp.failed", &[("server", &self.server_name)]),
            )
            .with_detail(outcome.text));
        }
        let say = text::tf("reply.mcp.done", &[("server", &self.server_name)]);
        Ok(Output::new(
            say,
            json!({ "text": outcome.text, "truncated": outcome.truncated, "media": outcome.media }),
        )
        .untrusted(source))
    }
}
