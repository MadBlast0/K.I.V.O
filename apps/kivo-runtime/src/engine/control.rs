//! Computer control inside a turn (M4): one authorization path for every caller (the fast path,
//! brains, agents), taint, destination binding, the tool's own risk assessment, Plan first,
//! Windows Hello for High risk, the active-use indicators, and the emergency stop's full reach.

use super::{Engine, lock};
use crate::speaker::Cue;
use kivo_core::text;
use kivo_core::tool::{
    ConfirmSpec, ConfirmedBy, GrantDuration, Provenance, Risk, Strength, Target, ToolCall,
};
use kivo_core::{Capability, SessionInput};
use kivo_security::{
    Context as SecurityContext, Decision, HardLimits, SessionKind, Taint, authorize, bind,
};
use std::sync::Arc;

/// Capabilities whose use shows an indicator in the tray and the Island (CAP-06).
pub(crate) fn indicator(capability: Capability) -> Option<&'static str> {
    match capability {
        Capability::ScreenAwareness => Some("screen"),
        Capability::ComputerUse => Some("input"),
        Capability::Shell => Some("shell"),
        _ => None,
    }
}

impl Engine {
    /// The user's own words for this task: the request, and earlier requests in its thread
    /// (destination binding, SECURITY §4).
    pub(super) fn user_words(&self) -> Vec<String> {
        let (transcript, thread) = lock(&self.turn)
            .as_ref()
            .map(|t| (t.transcript.clone(), t.thread.clone()))
            .unwrap_or_default();
        let mut words = vec![transcript];
        if let Some(thread) = thread {
            let earlier = self
                .recorder_db(|db| db.messages(&thread))
                .unwrap_or_default();
            words.extend(
                earlier
                    .into_iter()
                    .rev()
                    .filter(|m| m.role == "user")
                    .take(5)
                    .map(|m| m.text),
            );
        }
        words
    }

    /// Whether untrusted content has entered this turn, and from where.
    pub(super) fn taint(&self) -> Taint {
        let sources = lock(&self.turn)
            .as_ref()
            .map(|t| t.tainted.clone())
            .unwrap_or_default();
        if sources.is_empty() {
            Taint::Clean
        } else {
            Taint::Tainted(sources)
        }
    }

    /// Untrusted content from `source` entered the turn (SEC-13).
    pub(super) fn taint_with(&self, source: &str) {
        if let Some(t) = lock(&self.turn).as_mut()
            && !t.tainted.iter().any(|s| s == source)
        {
            t.tainted.push(source.to_owned());
        }
    }

    /// Fills in what a call acts on, binds its destinations to the user's words, and asks the
    /// permission engine (invariant 4). Every caller goes through here.
    pub(super) fn authorize_call(
        &self,
        tool: &dyn kivo_tools::Tool,
        call: &mut ToolCall,
    ) -> Decision {
        for t in tool
            .targets(&call.args)
            .into_iter()
            .chain(kivo_tools::targets(&call.tool, &call.args))
        {
            if !call.targets.contains(&t) {
                call.targets.push(t);
            }
        }
        // Hard limits the tool itself sees (a password field) end it here, before any question.
        if let Err(e) = tool.hard_limit(&call.args) {
            return Decision::Deny(kivo_security::Denial {
                code: kivo_security::DenyCode::HardLimit,
                message: e.message,
            });
        }
        // A local connector the user switched off: KIVO doesn't use that app (INT-03).
        let config = self.core.config();
        for t in &call.targets {
            if let Target::App { id, .. } = t
                && let Some(name) = crate::connectors::switched_off(&config, id)
            {
                return Decision::Deny(kivo_security::Denial {
                    code: kivo_security::DenyCode::BlockedApp,
                    message: text::tf("connectors.off", &[("name", &name)]),
                });
            }
        }
        let assessed = tool.assess(&call.args, call.initiated_by);
        self.authorize_spec(tool.spec(), assessed, call)
    }

    /// `authorize_call` for a step of work the user already approved as a whole (computer use,
    /// CAP-10): `task_grants` bound what it may do.
    pub(super) fn authorize_with(
        &self,
        tool: &dyn kivo_tools::Tool,
        call: &mut ToolCall,
        task_grants: Option<&[kivo_security::Grant]>,
    ) -> Decision {
        if task_grants.is_none() {
            return self.authorize_call(tool, call);
        }
        if let Err(e) = tool.hard_limit(&call.args) {
            return Decision::Deny(kivo_security::Denial {
                code: kivo_security::DenyCode::HardLimit,
                message: e.message,
            });
        }
        for t in tool.targets(&call.args) {
            if !call.targets.contains(&t) {
                call.targets.push(t);
            }
        }
        let assessed = tool.assess(&call.args, call.initiated_by);
        self.authorize_inner(tool.spec(), assessed, call, task_grants)
    }

    /// `authorize_call` for a spec without a registered tool (an agent's request).
    pub(super) fn authorize_spec(
        &self,
        spec: &kivo_core::tool::ToolSpec,
        assessed: Risk,
        call: &mut ToolCall,
    ) -> Decision {
        self.authorize_inner(spec, assessed, call, None)
    }

    fn authorize_inner(
        &self,
        spec: &kivo_core::tool::ToolSpec,
        assessed: Risk,
        call: &mut ToolCall,
        task_grants: Option<&[kivo_security::Grant]>,
    ) -> Decision {
        let taint = self.taint();
        let tainted = matches!(taint, Taint::Tainted(_));
        let words = self.user_words();
        for t in &mut call.targets {
            if let Target::Destination {
                address,
                provenance,
            } = t
                && *provenance != Provenance::Untrusted
            {
                *provenance = bind(address, &words, tainted);
            }
        }
        let config = self.core.config();
        let (stopped, guest) = lock(&self.turn)
            .as_ref()
            .map_or((false, false), |t| (t.cancel.is_cancelled(), t.guest));
        let limits = HardLimits {
            stopped,
            blocked_apps: config.permissions.blocked_apps.clone(),
        };
        let grants = self.recorder.grants();
        authorize(
            spec,
            call,
            &SecurityContext {
                mode: self.core.state().borrow().mode,
                session: if guest {
                    SessionKind::Guest
                } else {
                    SessionKind::Owner
                },
                taint,
                capabilities: &config.capabilities,
                limits: &limits,
                grants: &grants,
                tools: &config.tools,
                privacy: config.privacy.mode,
                assessed: Some(assessed),
                task_grants,
            },
        )
    }

    /// Confirms a High-risk action with Windows Hello (SEC-11, CONV-29): the system prompt shows,
    /// and only a verified face, fingerprint or PIN approves. Voice can start it, never finish it.
    pub async fn approve_with_hello(self: &Arc<Self>, call_id: &str) -> Result<(), String> {
        let action = lock(&self.turn)
            .as_ref()
            .and_then(|t| t.pending.as_ref())
            .filter(|(spec, _)| spec.call_id == call_id)
            .map(|(spec, _)| spec.action.clone())
            .ok_or_else(|| text::t("turn.nothingWaiting"))?;
        if !self.verifier.available() {
            return Err(text::t("decision.noHello"));
        }
        let verifier = Arc::clone(&self.verifier);
        let message = text::tf("decision.helloPrompt", &[("action", &action)]);
        let verified = tokio::task::spawn_blocking(move || verifier.verify(&message))
            .await
            .map_err(|e| e.to_string())?
            .unwrap_or(false);
        if !verified {
            self.speaker.cue(Cue::Error);
            return Err(text::t("decision.helloFailed"));
        }
        self.answer_by(call_id, true, None, ConfirmedBy::Hello)
            .await
    }

    /// Offers Windows Hello on a High-risk card when this PC has it (SEC-11).
    pub(super) fn with_hello(&self, mut spec: ConfirmSpec) -> ConfirmSpec {
        spec.hello = spec.strength == Strength::Strong && self.verifier.available();
        spec
    }

    /// The Plan first card for a round of steps (SECURITY §1.1): the steps in order, one answer.
    pub(super) fn plan_card(&self, steps: &[(ConfirmSpec, ToolCall)], round: usize) -> ConfirmSpec {
        let list: Vec<String> = steps
            .iter()
            .enumerate()
            .map(|(i, (s, _))| format!("{}. {}", i + 1, s.action))
            .collect();
        let risk = steps.iter().map(|(s, _)| s.risk).max().unwrap_or(Risk::Low);
        let strength = if steps.iter().any(|(s, _)| s.strength == Strength::Strong) {
            Strength::Strong
        } else {
            Strength::Normal
        };
        ConfirmSpec {
            call_id: format!("{}-plan{round}", self.turn_key()),
            tool: "plan".into(),
            action: text::tf("policy.planTitle", &[("steps", &list.join("\n"))]),
            target: None,
            why: text::t("policy.planMode"),
            provenance: steps
                .first()
                .map(|(s, _)| s.provenance.clone())
                .unwrap_or_default(),
            risk,
            strength,
            allow_always: false,
            plan: true,
            hello: strength == Strength::Strong && self.verifier.available(),
            watch: false,
        }
    }

    /// Shows a sensitive capability as in use (CAP-06) while `f` runs.
    pub(super) fn in_use(&self, capability: Capability, on: bool) {
        if let Some(kind) = indicator(capability) {
            self.core.set_in_use(kind, on);
        }
    }

    /// The parts of the emergency stop beyond the turn (SEC-27): every running command's process
    /// tree is killed and synthetic input stops at once.
    pub(super) fn stop_controls(&self) {
        let killed = self.commands.as_ref().map_or(0, |c| c.kill_all());
        if let Some(abort) = &self.input_abort {
            abort.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        if killed > 0 {
            tracing::warn!(killed, "stopped running commands");
        }
        self.core.clear_in_use();
    }

    /// A Hello or plan answer arrived: the card clears and the state moves on.
    pub(super) fn confirmed_state(&self) {
        self.core.advance(SessionInput::Confirmed);
    }

    /// Saves "Always allow" for the chosen duration (SEC-08).
    pub(super) fn grant(&self, call: &ToolCall, duration: Option<GrantDuration>) {
        if let Some(d) = duration {
            self.recorder.add_grant_for(call, d, None);
        }
    }
}
