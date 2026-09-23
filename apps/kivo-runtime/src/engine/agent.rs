//! A turn handed to a CLI agent over ACP (BRAINS §4, §7; CONVERSATION §5.1): KIVO sends the
//! request with a short handoff (CONV-30), shows the agent's plan, tool work and file changes as
//! steps and Activity, speaks its messages as they stream, and routes every permission the agent
//! asks for into KIVO's own permission engine and confirmation card (BRAIN-15).

use super::brain::failure_message;
use super::{Engine, lock};
use crate::brains::Meter;
use kivo_brain::NormalizedError;
use kivo_brain::acp::{AgentEvent, PermissionAnswer, PermissionAsk, PermissionHandler, pick};
use kivo_brain::context::{self, ContextItem, Trust};
use kivo_brain::routing::Route;
use kivo_brain::speech::Chunker;
use kivo_core::Capability;
use kivo_core::event::StepStatus;
use kivo_core::text;
use kivo_core::tool::{
    CapabilityTier, Initiator, Platform, Reversibility, Risk, SideEffect, ToolCall, ToolSpec,
};
use kivo_core::{SessionInput, SessionState};
use kivo_ipc::protocol::StepView;
use kivo_security::Decision;
use serde_json::json;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

const CARD_EVERY: Duration = Duration::from_millis(60);

/// What an agent's request kind means to KIVO's permission engine: `(risk, side effect,
/// reversibility)`. Reads are low risk; edits and moves change files; deleting and running
/// commands can't be taken back, so they are confirmed up front (SECURITY §2).
pub fn agent_spec(kind: &str) -> ToolSpec {
    let (risk, effect, reversibility) = match kind {
        "read" | "search" | "think" => (
            Risk::Low,
            SideEffect::LocalRead,
            Reversibility::NotApplicable,
        ),
        "fetch" => (
            Risk::Low,
            SideEffect::ExternalComms,
            Reversibility::NotApplicable,
        ),
        "edit" | "move" => (
            Risk::Medium,
            SideEffect::LocalWrite,
            Reversibility::NotApplicable,
        ),
        "delete" => (
            Risk::Medium,
            SideEffect::Destructive,
            Reversibility::Irreversible,
        ),
        "execute" => (
            Risk::Medium,
            SideEffect::LocalWrite,
            Reversibility::Irreversible,
        ),
        _ => (
            Risk::Medium,
            SideEffect::LocalWrite,
            Reversibility::NotApplicable,
        ),
    };
    let kind = match kind {
        "read" | "search" | "think" | "fetch" | "edit" | "move" | "delete" | "execute" => kind,
        _ => "other",
    };
    ToolSpec {
        id: format!("agent.{kind}"),
        description: format!("A CLI agent asks to {kind}"),
        title: "{agent}: {title}".into(),
        params: json!({ "type": "object" }),
        result: json!({ "type": "object" }),
        risk,
        side_effects: vec![effect],
        data_egress: kind == "fetch",
        timeout_ms: 0,
        cancellable: true,
        tier: CapabilityTier::AppCli,
        platforms: vec![Platform::Windows, Platform::MacOs, Platform::Linux],
        reversibility,
        capability: Capability::CliAgents,
    }
}

/// Answers agents' permission requests through the turn engine.
pub struct EnginePermissions(pub Weak<Engine>);

#[async_trait::async_trait]
impl PermissionHandler for EnginePermissions {
    async fn ask(&self, ask: PermissionAsk) -> PermissionAnswer {
        match self.0.upgrade() {
            Some(engine) => engine.agent_permission(ask).await,
            None => pick(&ask, false, false),
        }
    }
}

impl Engine {
    /// The agent asks to do something: KIVO's permission engine decides, asking the user on the
    /// card when the mode and risk say so (BRAIN-15).
    pub async fn agent_permission(self: &Arc<Self>, ask: PermissionAsk) -> PermissionAnswer {
        let agent = self
            .agents
            .owner_of(&ask.session_id)
            .and_then(|id| kivo_brain::catalog::entry(&id).map(|e| e.name.to_owned()))
            .unwrap_or_else(|| "The agent".to_owned());
        let Some((turn, guest, stopped)) = lock(&self.turn)
            .as_ref()
            .map(|t| (t.id.clone(), t.guest, t.cancel.is_cancelled()))
        else {
            // No turn to show it in: the agent is refused rather than let through.
            return pick(&ask, false, false);
        };
        let spec = agent_spec(&ask.kind);
        let mut call = ToolCall {
            id: format!("{turn}-a{}", ask.tool_call_id),
            tool: spec.id.clone(),
            args: json!({ "agent": agent, "title": ask.title }),
            initiated_by: Initiator::Brain,
            targets: Vec::new(),
        };
        let _ = (guest, stopped);
        let decision = self.authorize_spec(&spec, spec.risk, &mut call);
        self.recorder
            .tool_decision(&self.turn_key(), &call, &spec, &decision);
        match decision {
            Decision::Allow(_) => pick(&ask, true, false),
            Decision::Deny(_) => pick(&ask, false, false),
            Decision::Confirm(confirm) => match self.decide_for_agent(confirm, call).await {
                Some((allow, always, _)) => pick(&ask, allow, always),
                None => pick(&ask, false, false),
            },
        }
    }

    /// A decision for an agent: unlike a KIVO action, "deny" doesn't end the turn; the agent is
    /// told no and carries on.
    async fn decide_for_agent(
        self: &Arc<Self>,
        spec: kivo_core::tool::ConfirmSpec,
        call: ToolCall,
    ) -> Option<(bool, bool, kivo_core::tool::ConfirmedBy)> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let cancel = {
            let mut turn = lock(&self.turn);
            let running = turn.as_mut()?;
            running.pending = Some((spec.clone(), call));
            running.waiter = Some(tx);
            running.cancel.clone()
        };
        self.core.advance(SessionInput::NeedConfirmation);
        self.core
            .update_turn(|view| view.confirm = Some(spec.clone()));
        self.speaker.cue(crate::speaker::Cue::Question);
        self.say_phrase(format!("{}?", spec.action)).await;
        let idle = lock(&self.turn)
            .as_ref()
            .is_some_and(|t| t.speaking.is_none() && t.phrases.is_empty());
        if idle {
            self.listen_for_answer();
        }
        let answer = tokio::select! {
            answer = rx => answer.ok(),
            () = cancel.cancelled() => None,
        };
        if let Some((false, _, _)) = answer {
            // The decision is made and the agent's work continues.
            self.core.advance(SessionInput::Confirmed);
            self.speaker.cue(crate::speaker::Cue::Cancelled);
        }
        answer
    }

    /// Hands the request to a CLI agent and follows its work.
    pub(super) async fn agent_turn(self: &Arc<Self>, text: &str, route: &Route) {
        let config = self.core.config();
        let turn = self.turn_key();
        let id = route.target.provider.clone();
        let name = route.target_name.clone();
        let Some((cancel, guest, chosen)) = lock(&self.turn)
            .as_ref()
            .map(|t| (t.cancel.clone(), t.guest, t.thread.clone()))
        else {
            return;
        };
        let found = self.brains.agent(&id);
        if found.as_ref().is_some_and(|f| f.signed_in == Some(false)) {
            let message = text::tf("brain.agentSignIn", &[("name", &name)]);
            self.speak_and_finish(&message).await;
            return;
        }
        let cwd = self.agents.workspace();
        let session = match self
            .agents
            .session(&id, found.as_ref().map(|f| f.program.as_path()), &cwd)
            .await
        {
            Ok(s) => s,
            Err(e) => {
                self.provider_failed(&id, &e);
                let message = if found.is_none() {
                    text::tf("brain.agentNotFound", &[("name", &name)])
                } else {
                    failure_message(&name, &e)
                };
                self.recorder.brain_problem(&turn, &message, &e.to_string());
                self.speaker.cue(crate::speaker::Cue::Error);
                self.core
                    .update_turn(|view| view.error = Some(message.clone()));
                self.speak_and_finish(&message).await;
                return;
            }
        };
        let keep = config.privacy.retention_days > 0 && !guest;
        let thread = self.pick_thread(text, chosen, keep, &config);
        if let Some(t) = &thread {
            self.recorder_db(|db| db.add_message(t, "user", text, None, Some(&turn)));
        }
        self.agents
            .touched(&id, session.id(), &cwd, thread.as_deref());
        // The handoff: the request plus at most 300 tokens of the user's saved context (CONV-30).
        let preferences = self.recorder_db(|db| db.preferences()).unwrap_or_default();
        let notes: Vec<ContextItem> = preferences
            .into_iter()
            .map(|(k, v)| ContextItem::new(format!("{k}: {v}"), "preference", Trust::User))
            .collect();
        let mut handoff = context::handoff(text, &notes);
        let attachments = lock(&self.turn)
            .as_ref()
            .map(|t| t.attachments.clone())
            .unwrap_or_default();
        for (name, content) in &attachments {
            let item = ContextItem::new(
                content.clone(),
                format!("attached file {name}"),
                Trust::Untrusted,
            );
            handoff.push_str("\n\n");
            handoff.push_str(&item.render());
        }
        let live = self.agent_live_context(session.id(), &config);
        if !live.is_empty() {
            handoff.push_str("\n\n");
            handoff.push_str(&live);
        }
        if let Some(running) = lock(&self.turn).as_mut() {
            running.streaming = true;
        }
        self.core.set_step(StepView {
            id: format!("{turn}-agent"),
            title: text::tf("brain.agentWorking", &[("name", &name)]),
            status: StepStatus::Running,
            detail: None,
        });
        if self.core.state().borrow().session == SessionState::Thinking {
            self.core.advance(SessionInput::StartActing);
        }
        let mut events = session.prompt(&handoff, cancel.child_token());
        let mut answer = String::new();
        let mut chunker = Chunker::default();
        let mut reasoning = String::new();
        let mut last_card = Instant::now() - CARD_EVERY;
        let mut failed: Option<NormalizedError> = None;
        let mut stop = String::new();
        while let Some(event) = events.recv().await {
            match event {
                AgentEvent::Message(delta) => {
                    if answer.is_empty() {
                        self.mark("t7FirstToken");
                    }
                    answer.push_str(&delta);
                    for phrase in chunker.push(&delta) {
                        self.say_phrase(phrase).await;
                    }
                    if last_card.elapsed() >= CARD_EVERY {
                        last_card = Instant::now();
                        let shown = answer.clone();
                        self.core.update_turn(|view| view.answer = Some(shown));
                    }
                }
                AgentEvent::Thought(t) => reasoning.push_str(&t),
                AgentEvent::Plan(entries) => {
                    for (i, entry) in entries.iter().enumerate() {
                        self.core.set_step(StepView {
                            id: format!("{turn}-plan{i}"),
                            title: entry.content.clone(),
                            status: step_status(&entry.status),
                            detail: None,
                        });
                    }
                }
                AgentEvent::Tool {
                    id: tool,
                    title,
                    kind,
                    status,
                } => {
                    // The agent's own file and terminal work, shown as activity (BRAIN-15).
                    self.core.set_step(StepView {
                        id: format!("{turn}-t{tool}"),
                        title: if title.is_empty() { kind } else { title },
                        status: step_status(&status),
                        detail: None,
                    });
                }
                AgentEvent::FileDiff { path } => {
                    self.core.set_step(StepView {
                        id: format!("{turn}-f{path}"),
                        title: path,
                        status: StepStatus::Done,
                        detail: None,
                    });
                }
                AgentEvent::ModeChanged(mode) => tracing::debug!(mode, "agent mode changed"),
                AgentEvent::Done(reason) => {
                    stop = reason;
                    break;
                }
                AgentEvent::Error(e) => {
                    failed = Some(e);
                    break;
                }
            }
        }
        if cancel.is_cancelled() || stop == "cancelled" {
            return;
        }
        // Agents on a subscription report no tokens; the request is still counted (BRAIN-34).
        let spent = self.brains.meter(&Meter {
            kind: "agent",
            provider: &id,
            model: &route.target.model,
            profile: Some(&route.profile),
            usage: kivo_brain::Usage::default(),
            turn: Some(&turn),
            task: None,
            routine: None,
        });
        self.publish_provider(kivo_core::event::ProviderEvent::UsageRecorded {
            provider: id.clone(),
            cost: spent,
        });
        if config.brains.keep_reasoning && !reasoning.is_empty() {
            self.recorder.reasoning(&turn, &route.reason, &reasoning);
        }
        for phrase in chunker.finish() {
            self.say_phrase(phrase).await;
        }
        match failed {
            None => {
                self.brains.succeeded(&id);
                self.core.set_step(StepView {
                    id: format!("{turn}-agent"),
                    title: text::tf("brain.agentDone", &[("name", &name)]),
                    status: StepStatus::Done,
                    detail: None,
                });
                if answer.trim().is_empty() {
                    answer = text::tf("brain.agentDone", &[("name", &name)]);
                    self.say_phrase(answer.clone()).await;
                }
                self.core
                    .update_turn(|view| view.answer = Some(answer.clone()));
                self.recorder.answer(&turn, &answer, "done");
                if let Some(t) = &thread {
                    self.recorder_db(|db| {
                        db.add_message(t, "assistant", &answer, Some(&name), Some(&turn))
                    });
                    self.publish_provider(kivo_core::event::ProviderEvent::ThreadChanged {
                        thread: t.clone(),
                    });
                }
            }
            Some(e) => {
                // The agent died or failed: it is started afresh next time (M3-X3).
                self.provider_failed(&id, &e);
                if matches!(e, NormalizedError::ProviderDown(_)) {
                    self.agents.forget(&id, &cwd).await;
                }
                let message = failure_message(&name, &e);
                self.recorder.brain_problem(&turn, &message, &e.to_string());
                if let Some(running) = lock(&self.turn).as_mut() {
                    running.outcome = "failed";
                }
                self.core.set_step(StepView {
                    id: format!("{turn}-agent"),
                    title: text::tf("brain.agentWorking", &[("name", &name)]),
                    status: StepStatus::Failed,
                    detail: Some(message.clone()),
                });
                self.speaker.cue(crate::speaker::Cue::Error);
                self.core
                    .update_turn(|view| view.error = Some(message.clone()));
                self.say_phrase(message).await;
            }
        }
        self.end_stream();
    }
}

fn step_status(status: &str) -> StepStatus {
    match status {
        "completed" | "done" => StepStatus::Done,
        "failed" | "cancelled" => StepStatus::Failed,
        "pending" => StepStatus::Pending,
        _ => StepStatus::Running,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_requests_map_to_kivo_risks() {
        assert_eq!(agent_spec("read").risk, Risk::Low);
        assert_eq!(agent_spec("edit").side_effects, [SideEffect::LocalWrite]);
        assert_eq!(
            agent_spec("execute").reversibility,
            Reversibility::Irreversible
        );
        assert_eq!(
            agent_spec("delete").reversibility,
            Reversibility::Irreversible
        );
        assert_eq!(agent_spec("weird").id, "agent.other");
        assert!(agent_spec("fetch").data_egress);
    }
}
