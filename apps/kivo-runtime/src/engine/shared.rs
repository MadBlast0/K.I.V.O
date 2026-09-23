//! Calls from agents through KIVO's MCP server (TOOL-37, CONV-24). They are `Initiator::Mcp`
//! calls, decided by the permission engine like a brain's: allowed calls run, refused ones are
//! refused, and a call that needs the user's yes shows the decision card when a KIVO turn is
//! running (the agent KIVO launched is working in it). With no turn to show it in, the call is
//! refused with a plain reason, never run unasked.

use super::{Engine, lock};
use kivo_core::text;
use kivo_core::tool::{Initiator, ToolCall};
use kivo_security::{Answer, Decision};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

impl Engine {
    /// Runs a shared tool for `agent`; the result's data, or why it didn't run.
    pub async fn shared_call(
        self: &Arc<Self>,
        agent: &str,
        tool_id: &str,
        args: serde_json::Value,
    ) -> Result<kivo_tools::Output, String> {
        let caps = self.core.config().capabilities;
        let Some(tool) = self.registry.get(tool_id, &caps) else {
            let message = self.registry.known(tool_id).map_or_else(
                || text::t("turn.notUnderstood"),
                |spec| {
                    text::tf(
                        "policy.capabilityOff",
                        &[("capability", &spec.capability.label())],
                    )
                },
            );
            return Err(message);
        };
        let spec = tool.spec().clone();
        let mut call = ToolCall {
            id: format!("mcp-{}-{}", agent, kivo_store::brains::now_ms()),
            tool: tool_id.to_owned(),
            targets: kivo_tools::targets(tool_id, &args),
            args,
            initiated_by: Initiator::Mcp,
        };
        let decision = self.authorize_call(tool.as_ref(), &mut call);
        let key = self.turn_key();
        self.recorder.tool_decision(&key, &call, &spec, &decision);
        let permit = match decision {
            Decision::Allow(permit) => permit,
            Decision::Deny(denial) => return Err(denial.message),
            Decision::Confirm(confirm) => {
                if lock(&self.turn).is_none() {
                    return Err(text::tf("mcp.needsYou", &[("agent", &agent)]));
                }
                match self.decide(confirm.clone(), call.clone()).await {
                    Some((true, _, by)) => {
                        kivo_security::confirmed(&confirm, &call, Answer::Allow { by })
                            .map_err(|d| d.message)?
                    }
                    _ => return Err(text::t("reply.cancelled")),
                }
            }
        };
        let cancel = lock(&self.turn)
            .as_ref()
            .map_or_else(CancellationToken::new, |t| t.cancel.child_token());
        let (result, output) = kivo_tools::execute(tool, &call, permit, &cancel).await;
        self.recorder.tool_result(&key, &call, &result);
        match (result.status, output) {
            (Ok(_), Some(output)) => Ok(output),
            (Ok(_), None) => Ok(kivo_tools::Output::default()),
            (Err(e), _) => Err(e.message),
        }
    }
}
