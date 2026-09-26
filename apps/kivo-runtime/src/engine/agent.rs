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
    pub(super) async fn decide_for_agent(
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
        self.say_phrase(self.question(&spec.action)).await;
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
    /// Uses the model and reasoning level the user chose for this agent (the active brain), when
    /// the agent offers them; the agent's own defaults otherwise.
    pub(crate) async fn apply_agent_choice(
        &self,
        session: &kivo_brain::acp::AcpSession,
        chosen: &kivo_brain::routing::ModelRef,
    ) {
        let options = session.options();
        if options.models.iter().any(|m| m.id == chosen.model)
            && options.model.as_deref() != Some(chosen.model.as_str())
            && let Err(e) = session.set_model(&chosen.model).await
        {
            tracing::warn!(%e, model = chosen.model, "the agent didn't take the model");
        }
        if let Some(effort) = chosen.reasoning
            && let Err(e) = session.set_reasoning(effort).await
        {
            tracing::warn!(%e, "the agent didn't take the reasoning level");
        }
    }

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
        // The agent works in this folder: KIVO works in that project too (CONV-10).
        if let Some(workspaces) = self.workspaces() {
            workspaces.worked_in(&cwd);
        }
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
        // The agent's models and reasoning setting, and the ones the user chose for it.
        self.brains.note_agent(&id, session.options());
        self.apply_agent_choice(&session, &route.target).await;
        let keep = config.privacy.retention_days > 0 && !guest;
        let thread = self.pick_thread(text, chosen, keep, &config);
        if let Some(t) = &thread {
            self.recorder_db(|db| db.add_message(t, "user", text, None, Some(&turn)));
        }
        self.agents
            .touched(&id, session.id(), &cwd, thread.as_deref());
        // The handoff: the request plus at most 300 tokens of the user's saved context (CONV-30).
        // Relevant memories first (an agent's provider is a cloud one: sensitive notes stay out
        // unless allowed, MEM-07), then stated preferences.
        let mut notes: Vec<ContextItem> = Vec::new();
        if let Some(memory) = self.memory() {
            let current = self.workspaces().and_then(|w| w.current());
            let recalled = memory.recall(
                &crate::memory::Ask {
                    text,
                    app: None,
                    project: Some(&cwd),
                    workspace: current.as_ref().map(|w| w.name.as_str()),
                    cloud: true,
                    guest,
                },
                crate::memory::HANDOFF_TOKENS,
            );
            memory.record_used(&turn, &recalled);
            notes.extend(
                recalled
                    .into_iter()
                    .map(|r| ContextItem::new(r.text, "user-provided memory", Trust::System)),
            );
        }
        let preferences = self.recorder_db(|db| db.preferences()).unwrap_or_default();
        if !guest {
            notes.extend(
                preferences
                    .into_iter()
                    .map(|(k, v)| ContextItem::new(format!("{k}: {v}"), "preference", Trust::User)),
            );
        }
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
        // Coding work on tests or a build is checked before it's called done (BRAIN-31/32), and
        // shows in Tasks as a Coding task.
        let check = crate::checks::wanted(text);
        let check_command =
            check.and_then(|c| crate::checks::command(&cwd, &self.workspace_notes(&cwd), c));
        let coding_task = check.and_then(|_| {
            self.tasks().and_then(|tasks| {
                tasks.record(&coding_spec(
                    text,
                    &name,
                    &turn,
                    &cwd,
                    check_command.as_deref(),
                ))
            })
        });
        if let (Some(tasks), Some(task)) = (self.tasks(), &coding_task) {
            tasks.record_step(task, "agent", "running", None);
            self.core
                .update_turn(|view| view.task_id = Some(task.clone()));
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
        // What the agent did, for the workspace's notes (CONV-19).
        let mut plan: Vec<String> = Vec::new();
        let mut files: Vec<String> = Vec::new();
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
                    plan = entries.iter().map(|e| e.content.clone()).collect();
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
                    if !files.contains(&path) {
                        files.push(path.clone());
                    }
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
                if let (Some(tasks), Some(task)) = (self.tasks(), &coding_task) {
                    tasks.record_step(task, "agent", "done", Some(&clip(&answer)));
                }
                // BRAIN-31: "done" only once the project's own check passes.
                if check.is_some() {
                    let verdict = self
                        .verify_coding(
                            &session,
                            &name,
                            check_command.as_deref(),
                            &cwd,
                            coding_task.as_deref(),
                            &cancel,
                        )
                        .await;
                    answer.push_str("\n\n");
                    answer.push_str(&verdict);
                    plan.push(verdict.lines().next().unwrap_or_default().to_owned());
                }
                // Workspace notes: what was done here, for next time (CONV-19). Never for guests.
                if !guest
                    && let (Some(memory), Some(workspace)) =
                        (self.memory(), self.workspaces().and_then(|w| w.current()))
                {
                    let mut bullets = plan.clone();
                    if !files.is_empty() {
                        let list: Vec<String> = files.iter().take(8).cloned().collect();
                        bullets.push(text::tf(
                            "memory.changedFiles",
                            &[("files", &list.join(", "))],
                        ));
                    }
                    if bullets.is_empty() {
                        bullets.push(clip(answer.lines().next().unwrap_or_default()));
                    }
                    let request: String = text.chars().take(80).collect();
                    memory.workspace_log(&workspace.name, &format!("{name}: {request}"), &bullets);
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
                if let (Some(tasks), Some(task)) = (self.tasks(), &coding_task) {
                    tasks.record_step(task, "agent", "failed", Some(&e.to_string()));
                    tasks.record_end(task, false, &failure_message(&name, &e));
                }
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

fn clip(s: &str) -> String {
    s.chars().take(400).collect()
}

/// The Coding task a turn records: the agent's work, then the check (BRAIN-32).
fn coding_spec(
    request: &str,
    agent: &str,
    turn: &str,
    cwd: &std::path::Path,
    check: Option<&str>,
) -> kivo_core::task::TaskSpec {
    use kivo_core::task::{Criterion, Notify, OnError, StepAction, TaskKind, TaskSpec, TaskStep};
    let step = |id: &str, title: String, action: StepAction| TaskStep {
        id: id.into(),
        title,
        action,
        depends_on: Vec::new(),
        on_error: OnError::Stop,
        delay_ms: None,
        confirm: false,
    };
    TaskSpec {
        title: request.chars().take(80).collect(),
        kind: TaskKind::Coding,
        owner: agent.to_owned(),
        steps: std::iter::once(step(
            "agent",
            text::tf("brain.agentWorking", &[("name", &agent)]),
            StepAction::Agent {
                prompt: request.to_owned(),
                agent: None,
                cwd: Some(cwd.display().to_string()),
            },
        ))
        .chain(check.map(|command| {
            let mut s = step(
                "check",
                String::new(),
                StepAction::Verify {
                    criterion: Criterion::CommandSucceeds {
                        command: command.to_owned(),
                        cwd: Some(cwd.display().to_string()),
                    },
                },
            );
            s.depends_on = vec!["agent".into()];
            s
        }))
        .collect(),
        success: Vec::new(),
        grants: Vec::new(),
        timeout_ms: None,
        notify: Notify::Silent,
        routine_id: None,
        turn_id: Some(turn.to_owned()),
        cwd: Some(cwd.display().to_string()),
    }
}

impl Engine {
    /// The current workspace's notes when it is `cwd` (they may name the test command).
    fn workspace_notes(&self, cwd: &std::path::Path) -> String {
        self.workspaces()
            .and_then(|w| {
                w.current()
                    .filter(|c| std::path::Path::new(&c.path) == cwd)
                    .map(|c| w.instructions(&format!("workspace:{}", c.id)))
            })
            .unwrap_or_default()
    }

    /// Runs the project's check after an agent's coding work (BRAIN-31): the tests (or the build)
    /// again. If it still fails, the agent gets the failure once more; KIVO says "fixed" only when
    /// the check passes. Returns what to add to the answer, already spoken.
    async fn verify_coding(
        self: &Arc<Self>,
        session: &Arc<kivo_brain::acp::AcpSession>,
        agent: &str,
        command: Option<&str>,
        cwd: &std::path::Path,
        task: Option<&str>,
        cancel: &tokio_util::sync::CancellationToken,
    ) -> String {
        let turn = self.turn_key();
        let tasks = self.tasks();
        let finish = |ok: bool, text: String| {
            if let (Some(tasks), Some(task)) = (&tasks, task) {
                tasks.record_step(
                    task,
                    "check",
                    if ok { "done" } else { "failed" },
                    Some(&text),
                );
                tasks.record_end(task, ok, &text);
            }
            text
        };
        let Some(command) = command.map(str::to_owned) else {
            let said = text::t("coding.noCheck");
            self.say_phrase(said.clone()).await;
            return finish(false, said);
        };
        if let (Some(tasks), Some(task)) = (&tasks, task) {
            tasks.record_step(task, "check", "running", Some(&command));
        }
        for attempt in 0..2 {
            if cancel.is_cancelled() {
                return finish(false, text::t("reply.cancelled"));
            }
            let outcome = self.run_check(&command, cwd, attempt).await;
            match outcome {
                Ok((true, _)) => {
                    let said = if attempt == 0 {
                        text::tf("coding.passed", &[("command", &command)])
                    } else {
                        text::tf(
                            "coding.passedAfterRetry",
                            &[("command", &command), ("name", &agent)],
                        )
                    };
                    self.say_phrase(said.clone()).await;
                    return finish(true, said);
                }
                Ok((false, tail)) if attempt == 0 => {
                    // Once more, with what still fails.
                    let said = text::tf(
                        "coding.retrying",
                        &[("command", &command), ("name", &agent)],
                    );
                    self.say_phrase(said).await;
                    let prompt = text::tf(
                        "coding.retryPrompt",
                        &[("command", &command), ("output", &tail)],
                    );
                    let mut events = session.prompt(&prompt, cancel.child_token());
                    while let Some(event) = events.recv().await {
                        match event {
                            AgentEvent::Tool {
                                id: tool,
                                title,
                                kind,
                                status,
                            } => {
                                self.core.set_step(StepView {
                                    id: format!("{turn}-r{tool}"),
                                    title: if title.is_empty() { kind } else { title },
                                    status: step_status(&status),
                                    detail: None,
                                });
                            }
                            AgentEvent::Done(_) | AgentEvent::Error(_) => break,
                            _ => {}
                        }
                    }
                }
                Ok((false, tail)) => {
                    let said = text::tf(
                        "coding.stillFailing",
                        &[("command", &command), ("output", &tail)],
                    );
                    let short = text::tf("coding.stillFailingShort", &[("command", &command)]);
                    self.say_phrase(short).await;
                    if let Some(running) = lock(&self.turn).as_mut() {
                        running.outcome = "failed";
                    }
                    return finish(false, said);
                }
                Err(reason) => {
                    let said = text::tf("coding.couldntCheck", &[("reason", &reason)]);
                    self.say_phrase(said.clone()).await;
                    return finish(false, said);
                }
            }
        }
        finish(false, text::t("coding.noCheck"))
    }

    /// Runs one check command in the turn, through the permission engine (asking when the mode
    /// says so): whether it passed, and the end of its output.
    async fn run_check(
        self: &Arc<Self>,
        command: &str,
        cwd: &std::path::Path,
        attempt: usize,
    ) -> Result<(bool, String), String> {
        let capabilities = self.core.config().capabilities;
        let Some(tool) = self.registry.get("shell.run", &capabilities) else {
            return Err(text::tf(
                "policy.capabilityOff",
                &[("capability", &Capability::Shell.label())],
            ));
        };
        let mut call = ToolCall {
            id: format!("{}-check{attempt}", self.turn_key()),
            tool: "shell.run".into(),
            args: json!({ "command": command, "cwd": cwd.display().to_string() }),
            initiated_by: Initiator::UserDirect,
            targets: Vec::new(),
        };
        let decision = self.authorize_call(tool.as_ref(), &mut call);
        self.recorder
            .tool_decision(&self.turn_key(), &call, tool.spec(), &decision);
        let permit = match decision {
            Decision::Allow(p) => p,
            Decision::Deny(d) => return Err(d.message),
            Decision::Confirm(spec) => match self.decide(spec.clone(), call.clone()).await {
                Some((true, _, by)) => {
                    kivo_security::confirmed(&spec, &call, kivo_security::Answer::Allow { by })
                        .map_err(|d| d.message)?
                }
                _ => return Err(text::t("reply.cancelled")),
            },
        };
        let title = text::tf("coding.checkStep", &[("command", &command)]);
        let (result, output) = self.run_step(tool, &call, permit, &title).await;
        if let Err(e) = &result.status {
            return Err(e.message.clone());
        }
        let data = output.map(|o| o.data).unwrap_or_default();
        let passed = data["exitCode"].as_i64() == Some(0);
        let tail = crate::checks::tail(
            data["stdout"].as_str().unwrap_or_default(),
            data["stderr"].as_str().unwrap_or_default(),
            6,
        );
        Ok((passed, tail))
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
