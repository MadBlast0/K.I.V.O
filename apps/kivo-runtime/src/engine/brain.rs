//! The brain path of a turn (BRAINS §1): route with a reason (BRAIN-21, PLAN-17), assemble the
//! context (CONV-05), stream the answer into the card and, phrase by phrase, into the voice
//! (BRAIN-28), run the brain's tool calls through the permission engine (invariant 4), fail over
//! within the same privacy class (BRAIN-22), meter usage and cost (BRAIN-34/35) and keep the
//! conversation thread (CONV-01).

use super::{Engine, lock};
use crate::brains::Meter;
use crate::speaker::Cue;
use kivo_brain::context::{
    self, ContextItem, Layers, Snapshot, ToolCandidate, Trust, budget, model_class,
};
use kivo_brain::cost::{AtLimit, Limit, Period};
use kivo_brain::persona;
use kivo_brain::routing::{ModelRef, Route, RouteError, TaskClass, classify_task};
use kivo_brain::speech::Chunker;
use kivo_brain::{
    BrainEvent, ChatRequest, Message, NormalizedError, Part, PrivacyClass, ProviderKind, Role,
    Usage,
};
use kivo_core::event::{CancelReason, EventKind, IntentPath, ProviderEvent, TurnEvent};
use kivo_core::text;
use kivo_core::tool::{ConfirmSpec, ConfirmedBy, Initiator, Risk, Strength, ToolCall};
use kivo_core::{Event, SessionInput, SessionState};
use kivo_ipc::protocol::BrainChip;
use kivo_security::{Answer, Decision, Taint};
use kivo_store::brains::now_ms;
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Tool rounds in one turn before KIVO stops and says so.
const MAX_ROUNDS: usize = 8;
/// The card's streamed answer is updated at most this often.
const CARD_EVERY: Duration = Duration::from_millis(60);
/// Answers spoken aloud are kept short (BRAINS §11).
const VOICE_MAX_TOKENS: u32 = 700;
const TEXT_MAX_TOKENS: u32 = 4_096;

/// The voice session (CONVERSATION §1): the thread it adds to and when it was last used.
#[derive(Default)]
pub(super) struct VoiceSession {
    thread: Option<String>,
    last: Option<Instant>,
    /// The live context last sent in this thread, for "what changed" (MEM-11).
    snapshot: Option<(String, Snapshot)>,
    /// When history isn't kept (retention 0, or a guest): this session's turns, in memory only.
    memory: Vec<Message>,
    /// The live context each agent session was last given (MEM-11): agents keep their own
    /// history, so after the first prompt they only need what changed.
    agents: std::collections::HashMap<String, Snapshot>,
}

/// Phrases that start a new thread ("Kivo, new topic", CONV-01).
const NEW_TOPIC: &[&str] = &[
    "new topic",
    "new conversation",
    "start a new conversation",
    "start over",
    "change of topic",
    "different topic",
];

/// "New topic, what's the weather" → `Some("what's the weather")`.
pub(super) fn new_topic(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let trimmed = lower
        .trim_start_matches(|c: char| !c.is_alphanumeric())
        .trim_start_matches("kivo")
        .trim_start_matches(|c: char| !c.is_alphanumeric());
    let offset = lower.len() - trimmed.len();
    NEW_TOPIC.iter().find_map(|p| {
        trimmed.starts_with(p).then(|| {
            text[offset + p.len()..]
                .trim_start_matches(|c: char| !c.is_alphanumeric())
                .trim()
                .to_owned()
        })
    })
}

/// Words that carry a topic (lowercase, at least four letters, not a filler word).
fn topic_words(text: &str) -> std::collections::HashSet<String> {
    const FILLER: &[&str] = &[
        "what", "when", "where", "which", "about", "there", "their", "these", "those", "would",
        "could", "should", "please", "thanks", "thank", "tell", "does", "have", "with", "that",
        "this", "from", "your", "just", "like", "kivo", "into", "some", "more", "also", "then",
        "them", "they", "will", "make", "know",
    ];
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 4 && !FILLER.contains(w))
        .map(str::to_owned)
        .collect()
}

/// Does `text` continue a conversation about `earlier` (CONV-01 "on the same topic")? A request
/// that leans on what came before ("and tomorrow?", "what about it") always does.
pub(super) fn same_topic(text: &str, earlier: &[String]) -> bool {
    let lower = text.to_lowercase();
    let leans = [
        "and ",
        "also ",
        "what about",
        "how about",
        "it ",
        "it?",
        "that ",
        "that?",
        "them",
        "they ",
        "again",
        "more ",
        "why",
    ];
    if leans
        .iter()
        .any(|l| lower.starts_with(l) || lower == l.trim())
    {
        return true;
    }
    let words = topic_words(text);
    earlier.iter().any(|e| !topic_words(e).is_disjoint(&words))
}

/// The provider's words for a failure, said plainly (PLAN-05).
pub(super) fn failure_message(name: &str, error: &NormalizedError) -> String {
    let key = match error {
        NormalizedError::Auth => "brain.auth",
        NormalizedError::RateLimited { .. } => "brain.busy",
        NormalizedError::Quota => "brain.quota",
        NormalizedError::ContextTooLong => "brain.tooLong",
        NormalizedError::ContentFiltered => "brain.filtered",
        NormalizedError::Network(_) => "brain.network",
        NormalizedError::ProviderDown(_) => "brain.down",
        NormalizedError::Cancelled | NormalizedError::Other(_) => "brain.failed",
    };
    text::tf(key, &[("name", &name)])
}

fn route_message(error: &RouteError) -> String {
    match error {
        RouteError::NoBrain => text::t("brain.noBrain"),
        RouteError::NeedsLocal => text::t("brain.needsLocal"),
        RouteError::NotConnected(name) => text::tf("brain.notConnected", &[("name", name)]),
        RouteError::Unavailable(name) => text::tf("brain.unavailable", &[("name", name)]),
    }
}

fn period_word(period: Period) -> String {
    text::t(match period {
        Period::Daily => "brain.periodDaily",
        Period::Weekly => "brain.periodWeekly",
        Period::Monthly { .. } => "brain.periodMonthly",
    })
}

/// "Tuesday 14:05" in local time.
fn local_time(utc_offset_minutes: i32) -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
        + i64::from(utc_offset_minutes) * 60;
    let days = secs.div_euclid(86_400);
    let of_day = secs.rem_euclid(86_400);
    // 1970-01-01 was a Thursday.
    let weekday = [
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
    ][usize::try_from(days.rem_euclid(7)).unwrap_or(0)];
    let (y, m, d) = civil(days);
    format!(
        "{weekday} {y:04}-{m:02}-{d:02} {:02}:{:02}",
        of_day / 3_600,
        of_day % 3_600 / 60
    )
}

/// Days since 1970-01-01 → (year, month, day) (Howard Hinnant's algorithm).
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// What one streamed round produced.
#[derive(Default)]
struct Round {
    text: String,
    calls: Vec<(String, String, serde_json::Value)>,
    usage: Usage,
    reasoning: String,
    error: Option<NormalizedError>,
}

impl Engine {
    /// Answers a request with a brain.
    pub(super) async fn brain_turn(self: &Arc<Self>, text: &str) {
        let config = self.core.config();
        let turn = self.turn_key();
        // "Kivo, new topic …" starts a new thread (CONV-01).
        let text = match new_topic(text) {
            Some(rest) => {
                {
                    let mut session = lock(&self.session);
                    session.thread = None;
                    session.memory.clear();
                    session.snapshot = None;
                }
                if rest.is_empty() {
                    self.speak_and_finish(&text::t("brain.newTopic")).await;
                    return;
                }
                rest
            }
            None => text.to_owned(),
        };
        let Some(choice) = lock(&self.turn).as_ref().map(|t| t.brain_choice.clone()) else {
            return;
        };
        // Sensitive data never goes to a cloud brain (SECURITY §6, BRAINS §5 rule 3).
        let sensitive = !kivo_security::classify(&text).cloud_ok();
        let mut routed = self
            .brains
            .route(&config, &text, choice.as_deref(), sensitive);
        if matches!(routed, Err(RouteError::Unavailable(_))) {
            // Every candidate failed its last check: look again before saying so.
            self.brains.recheck_unhealthy().await;
            routed = self
                .brains
                .route(&config, &text, choice.as_deref(), sensitive);
        }
        let mut route = match routed {
            Ok(route) => route,
            Err(e) => {
                self.core
                    .bus
                    .publish(Event::new(EventKind::Turn(TurnEvent::IntentDetected {
                        path: IntentPath::Brain,
                        intent: "none".into(),
                    })));
                if let Some(running) = lock(&self.turn).as_mut() {
                    running.outcome = "unhandled";
                }
                // A CLI agent asked for by name while CLI agents are off: say so, and the
                // Island offers to turn them on (CAP-02).
                let agent_off = matches!(&e, RouteError::NotConnected(name)
                    if kivo_brain::catalog::CATALOG
                        .iter()
                        .any(|c| c.name == name && c.kind == ProviderKind::Cli))
                    && !config
                        .capabilities
                        .enabled(kivo_core::Capability::CliAgents);
                let message = if agent_off {
                    self.core.update_turn(|view| {
                        view.capability_off = Some(kivo_core::Capability::CliAgents);
                    });
                    text::tf(
                        "policy.capabilityOff",
                        &[("capability", &kivo_core::Capability::CliAgents.label())],
                    )
                } else {
                    route_message(&e)
                };
                self.recorder.brain_problem(&turn, &message, &e.to_string());
                self.speak_and_finish(&message).await;
                return;
            }
        };
        // A stale health is checked in the background; a failure fails over (DISC-14).
        if self.brains.stale(&route.target.provider) {
            let brains = Arc::clone(&self.brains);
            let id = route.target.provider.clone();
            tokio::spawn(async move {
                brains.check(&id).await;
            });
        }
        // Spending limits (BRAIN-36): the default is no limit, track only.
        let warning = match self.spending_check(&config, &route, &text, sensitive).await {
            Spend::Go(w) => w,
            Spend::Rerouted(new_route, w) => {
                route = *new_route;
                w
            }
            Spend::Stop => return,
        };
        self.show_route(&route, warning);
        if route.kind == ProviderKind::Cli {
            self.agent_turn(&text, &route).await;
            return;
        }
        self.api_turn(&text, route).await;
    }

    /// Tells the Control Center about usage and conversation changes (DISCOVERY §3: pushed).
    pub(super) fn publish_provider(&self, event: ProviderEvent) {
        self.core
            .bus
            .publish(Event::new(EventKind::Provider(event)));
    }

    /// A provider failed: its health changes now and is checked again right away, so a passing
    /// outage doesn't keep it out of routing until the next scheduled check (DISC-14).
    pub(super) fn provider_failed(&self, id: &str, error: &NormalizedError) {
        // A CLI agent that crashed is simply started again next time (M3-X3); only a refused
        // sign-in marks it.
        let agent = kivo_brain::catalog::entry(id).is_some_and(|e| e.kind == ProviderKind::Cli);
        if agent && !matches!(error, NormalizedError::Auth) {
            return;
        }
        self.brains.failed(id, error);
        if matches!(error, NormalizedError::Auth | NormalizedError::Quota) {
            return;
        }
        let brains = Arc::clone(&self.brains);
        let id = id.to_owned();
        tokio::spawn(async move {
            brains.check(&id).await;
        });
    }

    /// Puts the brain chip in the card, and the route in Activity and the turn record (PLAN-17).
    fn show_route(&self, route: &Route, warning: Option<String>) {
        let profile = self
            .brains
            .profiles()
            .into_iter()
            .find(|p| p.id == route.profile)
            .map_or_else(|| route.profile.clone(), |p| p.name);
        self.core
            .bus
            .publish(Event::new(EventKind::Turn(TurnEvent::IntentDetected {
                path: IntentPath::Brain,
                intent: route.target.provider.clone(),
            })));
        self.core
            .bus
            .publish(Event::new(EventKind::Turn(TurnEvent::BrainSelected {
                profile: route.profile.clone(),
                provider: route.target.provider.clone(),
                model: route.target.model.clone(),
                reason: route.reason.clone(),
            })));
        let chip = BrainChip {
            name: route.target_name.clone(),
            profile,
            reason: route.reason.clone(),
            local: route.privacy == PrivacyClass::Local,
            cost: None,
            warning,
            context_used: 0,
            context_budget: 0,
        };
        self.core.update_turn(|view| view.brain = Some(chip));
        self.recorder.brain_route(
            &self.turn_key(),
            &route.reason,
            json!({
                "profile": route.profile,
                "provider": route.target.provider,
                "model": route.target.model,
            }),
        );
    }

    /// Where the spending limits stand for this request, and what to do at a limit.
    async fn spending_check(
        self: &Arc<Self>,
        config: &kivo_core::KivoConfig,
        route: &Route,
        text: &str,
        sensitive: bool,
    ) -> Spend {
        let feature = if route.kind == ProviderKind::Cli {
            "agents"
        } else {
            "brain"
        };
        let states = self
            .brains
            .limit_states(&route.target.provider, &route.profile, feature);
        let warning = states
            .iter()
            .filter_map(|(l, s)| s.warning.map(|w| (l, w)))
            .max_by_key(|(_, w)| *w)
            .map(|(l, w)| {
                text::tf(
                    "brain.limitWarning",
                    &[("percent", &w), ("period", &period_word(l.period))],
                )
            });
        let Some((limit, _)) = states.iter().find(|(_, s)| s.reached) else {
            return Spend::Go(warning);
        };
        // Local brains cost nothing: they are never held back by a limit.
        if route.privacy == PrivacyClass::Local {
            return Spend::Go(warning);
        }
        let amount = format!("${:.2}", limit.amount);
        match limit.at_limit {
            AtLimit::Block => {
                let message = text::tf(
                    "brain.limitBlocked",
                    &[("period", &period_word(limit.period))],
                );
                self.speak_and_finish(&message).await;
                Spend::Stop
            }
            AtLimit::LocalOnly | AtLimit::CheaperProfile => {
                let mut config = config.clone();
                let profile = if limit.at_limit == AtLimit::CheaperProfile {
                    Some("cheap")
                } else {
                    config.privacy.mode = kivo_core::config::PrivacyMode::Local;
                    None
                };
                match self.brains.route(&config, text, profile, sensitive) {
                    Ok(r) if r.target != route.target => Spend::Rerouted(Box::new(r), warning),
                    _ => {
                        let message = text::tf(
                            "brain.limitBlocked",
                            &[("period", &period_word(limit.period))],
                        );
                        self.speak_and_finish(&message).await;
                        Spend::Stop
                    }
                }
            }
            AtLimit::Ask => {
                if self.ask_over_limit(limit, &amount).await {
                    Spend::Go(warning)
                } else {
                    Spend::Stop
                }
            }
        }
    }

    /// "Go over your daily spending limit of $5.00?" on the card; true when the user allows it.
    async fn ask_over_limit(self: &Arc<Self>, limit: &Limit, amount: &str) -> bool {
        let period = period_word(limit.period);
        let call = ToolCall {
            id: format!("{}-limit", self.turn_key()),
            tool: "brain.overLimit".into(),
            args: json!({}),
            initiated_by: Initiator::UserDirect,
            targets: Vec::new(),
        };
        let spec = ConfirmSpec {
            call_id: call.id.clone(),
            tool: call.tool.clone(),
            action: text::tf(
                "brain.limitAsk",
                &[("period", &period), ("amount", &amount)],
            ),
            target: None,
            why: text::t("brain.limitWhy"),
            provenance: text::t("brain.overLimitTool"),
            risk: Risk::Low,
            strength: Strength::Normal,
            allow_always: false,
            plan: false,
            hello: false,
        };
        matches!(self.decide(spec, call).await, Some((true, _, _)))
    }

    /// Shows a decision on the card and waits for the answer (click or voice, CONV-26).
    /// `None` when the turn ended first.
    pub(super) async fn decide(
        self: &Arc<Self>,
        spec: ConfirmSpec,
        call: ToolCall,
    ) -> Option<(bool, bool, ConfirmedBy)> {
        let spec = self.with_hello(spec);
        let (tx, rx) = tokio::sync::oneshot::channel();
        let cancel = {
            let mut turn = lock(&self.turn);
            let running = turn.as_mut()?;
            running.pending = Some((spec.clone(), call));
            running.waiter = Some(tx);
            running.cancel.clone()
        };
        self.core.advance(SessionInput::NeedConfirmation);
        let question = format!("{}?", spec.action);
        self.core.update_turn(|view| {
            view.confirm = Some(spec.clone());
            view.target_app.clone_from(&spec.target);
        });
        self.speaker.cue(Cue::Question);
        self.say_phrase(question).await;
        // Nothing more is being said: listen for the spoken answer now.
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
            // Denied: the turn ends, as for any declined action.
            self.core.advance(SessionInput::Denied);
            self.speaker.cue(Cue::Cancelled);
            self.core
                .update_turn(|view| view.answer = Some(text::t("reply.cancelled")));
            self.finish_turn("cancelled", Some(&text::t("reply.cancelled")));
        }
        answer
    }

    /// A request to an API or local brain, with tool rounds and failover.
    async fn api_turn(self: &Arc<Self>, text: &str, route: Route) {
        let config = self.core.config();
        let turn = self.turn_key();
        let Some((cancel, source, guest, chosen)) = lock(&self.turn)
            .as_ref()
            .map(|t| (t.cancel.clone(), t.source, t.guest, t.thread.clone()))
        else {
            return;
        };
        let voice = source != kivo_core::event::TurnSource::Typed;
        let keep = config.privacy.retention_days > 0 && !guest;
        let thread = self.pick_thread(text, chosen, keep, &config);
        if let Some(id) = &thread {
            let _ = self.recorder_db(|db| db.add_message(id, "user", text, None, Some(&turn)));
        }
        if let Some(running) = lock(&self.turn).as_mut() {
            running.streaming = true;
        }
        let mut attempt = route.target.clone();
        let mut tried = vec![attempt.clone()];
        let mut extra: Vec<Message> = Vec::new();
        let mut answer = String::new();
        let mut chunker = Chunker::default();
        let mut cost = 0.0;
        let mut known_cost = true;
        let mut reasoning = String::new();
        let mut rounds = 0;
        let mut calls_made = 0usize;
        let mut last_card = Instant::now() - CARD_EVERY;
        let outcome = loop {
            let Some(provider) = self.brains.provider(&attempt.provider) else {
                break Err(NormalizedError::ProviderDown("not connected".into()));
            };
            let info = provider.info();
            if attempt.model.is_empty() {
                // A local server with no model known yet: ask it what it has.
                attempt.model = provider
                    .models()
                    .await
                    .ok()
                    .and_then(|m| m.into_iter().next())
                    .map(|m| m.id)
                    .unwrap_or_default();
            }
            // May this brain be shown a screenshot (CAP-08)? A model that sees, and a local
            // brain or cloud vision allowed by the settings and the privacy mode.
            let sees = self.brains.sees(&attempt.provider, &attempt.model)
                && (info.privacy == PrivacyClass::Local
                    || (config.tools.cloud_vision
                        && matches!(
                            config.privacy.mode,
                            kivo_core::config::PrivacyMode::Cloud
                                | kivo_core::config::PrivacyMode::Custom
                        )));
            self.vision.store(sees, std::sync::atomic::Ordering::SeqCst);
            let request = self.brain_request(
                &config,
                &route,
                &attempt,
                info.kind,
                text,
                thread.as_deref(),
                &extra,
                voice,
            );
            let (request, used, budget_tokens, to_compact) = request;
            self.core.update_turn(|view| {
                if let Some(chip) = view.brain.as_mut() {
                    chip.context_used = used;
                    chip.context_budget = budget_tokens;
                }
            });
            let mut stream = provider.chat(request, cancel.child_token());
            let mut round = Round::default();
            while let Some(event) = stream.recv().await {
                match event {
                    BrainEvent::TextDelta(delta) => {
                        if answer.is_empty() && round.text.is_empty() {
                            self.mark("t7FirstToken");
                        }
                        round.text.push_str(&delta);
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
                    BrainEvent::ToolCall { id, name, args } => round.calls.push((id, name, args)),
                    BrainEvent::Reasoning(r) => round.reasoning.push_str(&r),
                    BrainEvent::Usage(u) => round.usage.add(u),
                    BrainEvent::Done(_) => break,
                    BrainEvent::Error(e) => {
                        round.error = Some(e);
                        break;
                    }
                    BrainEvent::ToolCallDelta { .. } => {}
                }
            }
            if cancel.is_cancelled() {
                return;
            }
            reasoning.push_str(&round.reasoning);
            if round.usage != Usage::default() {
                let spent = self.brains.meter(&Meter {
                    kind: "brain",
                    provider: &attempt.provider,
                    model: &attempt.model,
                    profile: Some(&route.profile),
                    usage: round.usage,
                    turn: Some(&turn),
                    task: None,
                    routine: None,
                });
                match spent {
                    Some(c) => cost += c,
                    None => known_cost = false,
                }
                self.publish_provider(ProviderEvent::UsageRecorded {
                    provider: attempt.provider.clone(),
                    cost: spent,
                });
                let show = config.brains.show_cost && known_cost;
                self.core.update_turn(|view| {
                    if let Some(chip) = view.brain.as_mut() {
                        chip.cost = show.then_some(cost);
                    }
                });
            }
            if let Some(error) = round.error {
                self.provider_failed(&attempt.provider, &error);
                // Nothing said yet and another brain of the same privacy class is ready:
                // switch to it (BRAIN-22). Otherwise say what went wrong.
                let next = route
                    .fallbacks
                    .iter()
                    .find(|f| !tried.contains(f))
                    .filter(|_| error.fails_over() && answer.is_empty() && calls_made == 0)
                    .cloned();
                if let Some(next) = next {
                    let from = info.name.clone();
                    let to = self
                        .brains
                        .provider(&next.provider)
                        .map_or_else(|| next.provider.clone(), |p| p.info().name);
                    let note = text::tf("brain.failedOver", &[("from", &from), ("to", &to)]);
                    self.recorder
                        .brain_problem(&turn, &note, &error.to_string());
                    self.core.update_turn(|view| {
                        if let Some(chip) = view.brain.as_mut() {
                            chip.name.clone_from(&to);
                            chip.reason = format!("{} · {note}", chip.profile);
                        }
                    });
                    tried.push(next.clone());
                    attempt = next;
                    continue;
                }
                break Err(error);
            }
            self.brains.succeeded(&attempt.provider);
            if to_compact > 0
                && let Some(id) = &thread
            {
                self.compact_later(id.clone(), to_compact);
            }
            if round.calls.is_empty() {
                break Ok(());
            }
            rounds += 1;
            if rounds >= MAX_ROUNDS {
                let note = text::t("brain.tooManySteps");
                answer.push_str(&format!("\n\n{note}"));
                for phrase in chunker.push(&format!(" {note}")) {
                    self.say_phrase(phrase).await;
                }
                break Ok(());
            }
            // The brain asked for tools: each goes through the permission engine.
            let mut asked = Message {
                role: Role::Assistant,
                parts: Vec::new(),
            };
            if !round.text.is_empty() {
                asked.parts.push(Part::Text {
                    text: round.text.clone(),
                });
            }
            let mut results = Message {
                role: Role::Tool,
                parts: Vec::new(),
            };
            let mut prepared = Vec::new();
            for (id, name, args) in round.calls {
                calls_made += 1;
                asked.parts.push(Part::ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    args: args.clone(),
                });
                prepared.push(self.prepare_brain_tool(&id, &name, args, calls_made));
            }
            // Plan first: the round's changes are shown as one plan and approved together
            // (SECURITY §1.1); reads run as they come.
            let plan_mode =
                self.core.state().borrow().mode == kivo_core::config::PermissionMode::Plan;
            let mut approved: std::collections::HashMap<String, kivo_security::Permit> =
                std::collections::HashMap::new();
            if plan_mode {
                let steps: Vec<(ConfirmSpec, ToolCall)> = prepared
                    .iter()
                    .filter_map(|p| match p {
                        Prepared::Ask { confirm, call, .. } if confirm.plan => {
                            Some((confirm.clone(), call.clone()))
                        }
                        _ => None,
                    })
                    .collect();
                if !steps.is_empty() {
                    let card = self.plan_card(&steps, rounds);
                    let placeholder = ToolCall {
                        id: card.call_id.clone(),
                        tool: "plan".into(),
                        args: json!({ "steps": steps.len() }),
                        initiated_by: Initiator::Brain,
                        targets: Vec::new(),
                    };
                    let Some((allow, _, by)) = self.decide(card, placeholder).await else {
                        return;
                    };
                    if !allow {
                        return;
                    }
                    match kivo_security::approve_plan(&steps, Answer::Allow { by }) {
                        Ok(permits) => {
                            for permit in permits {
                                approved.insert(permit.call_id().to_owned(), permit);
                            }
                        }
                        Err(denial) => {
                            self.say_phrase(denial.message).await;
                            return;
                        }
                    }
                }
            }
            for p in prepared {
                let Some(parts) = self.finish_brain_tool(p, &mut approved).await else {
                    // Declined or cancelled: the turn has ended.
                    return;
                };
                results.parts.extend(parts);
            }
            extra.push(asked);
            extra.push(results);
        };
        // The rest of the answer, then the end of the stream.
        for phrase in chunker.finish() {
            self.say_phrase(phrase).await;
        }
        if config.brains.keep_reasoning && !reasoning.is_empty() {
            self.recorder.reasoning(&turn, &route.reason, &reasoning);
        }
        let brain_name = self
            .brains
            .provider(&attempt.provider)
            .map_or_else(|| route.target_name.clone(), |p| p.info().name);
        match outcome {
            Ok(()) => {
                if answer.trim().is_empty() {
                    answer = if calls_made > 0 {
                        text::t("reply.done")
                    } else {
                        text::tf("brain.empty", &[("name", &brain_name)])
                    };
                    self.say_phrase(answer.clone()).await;
                }
                self.core
                    .update_turn(|view| view.answer = Some(answer.clone()));
                self.recorder.answer(&turn, &answer, "done");
                match &thread {
                    Some(id) => {
                        let _ = self.recorder_db(|db| {
                            db.add_message(id, "assistant", &answer, Some(&brain_name), Some(&turn))
                        });
                        self.publish_provider(ProviderEvent::ThreadChanged { thread: id.clone() });
                    }
                    None => {
                        let mut session = lock(&self.session);
                        session.memory.push(Message::user(text));
                        session.memory.push(Message::assistant(&answer));
                    }
                }
            }
            Err(error) => {
                let message = failure_message(&brain_name, &error);
                self.recorder
                    .brain_problem(&turn, &message, &error.to_string());
                if let Some(running) = lock(&self.turn).as_mut() {
                    running.outcome = "failed";
                }
                self.speaker.cue(Cue::Error);
                self.core.update_turn(|view| {
                    view.error = Some(message.clone());
                    if !answer.is_empty() {
                        view.answer = Some(answer.clone());
                    }
                });
                self.say_phrase(message).await;
            }
        }
        self.end_stream();
    }

    /// Runs the database closure, logging a failure (history is best-effort).
    pub(super) fn recorder_db<T>(
        &self,
        f: impl FnOnce(&kivo_store::Database) -> Result<T, kivo_store::db::DbError>,
    ) -> Option<T> {
        let db = self.brains.database();
        let db = lock(&db);
        match f(&db) {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!(%e, "couldn't update the conversation");
                None
            }
        }
    }

    /// The thread this request belongs to (CONV-01); `None` when history isn't kept.
    pub(super) fn pick_thread(
        &self,
        text: &str,
        chosen: Option<String>,
        keep: bool,
        config: &kivo_core::KivoConfig,
    ) -> Option<String> {
        if !keep {
            return None;
        }
        if let Some(id) = chosen {
            return Some(id);
        }
        let session_length =
            Duration::from_secs(u64::from(config.brains.voice_session_minutes) * 60);
        let mut session = lock(&self.session);
        let within_session = session.last.is_some_and(|t| t.elapsed() < session_length);
        session.last = Some(Instant::now());
        if within_session && let Some(thread) = &session.thread {
            return Some(thread.clone());
        }
        // A new voice session joins a recent thread on the same topic.
        let since = now_ms() - i64::from(config.brains.thread_join_minutes) * 60_000;
        let joined = self
            .recorder_db(|db| {
                let Some(c) = db.latest_conversation("voice", since)? else {
                    return Ok(None);
                };
                let messages = db.messages(&c.id)?;
                let mut earlier: Vec<String> = messages
                    .iter()
                    .rev()
                    .take(6)
                    .map(|m| m.text.clone())
                    .collect();
                earlier.push(c.title.clone());
                Ok(same_topic(text, &earlier).then_some(c.id))
            })
            .flatten();
        let id = joined.or_else(|| {
            let id = format!("c-{}", uuid::Uuid::now_v7().simple());
            let title: String = text.chars().take(60).collect();
            self.recorder_db(|db| db.create_conversation(&id, "voice", &title))
                .map(|c| c.id)
        })?;
        session.thread = Some(id.clone());
        Some(id)
    }

    /// The live context: time, language, mode and the window in front (BRAINS §6 "Always").
    fn live_snapshot(&self, config: &kivo_core::KivoConfig) -> (Snapshot, Option<ContextItem>) {
        let mut snapshot = Snapshot::default();
        snapshot.set("local time", local_time(self.brains.utc_offset()));
        snapshot.set("language", config.general.language.clone());
        let mode = serde_json::to_value(self.core.state().borrow().mode)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        snapshot.set("permission mode", mode);
        let mut title = None;
        if let Ok(Some(window)) = self.windows.foreground() {
            snapshot.set("active app", window.app_id.clone());
            // A window title is written by another program: data, never instructions.
            let text: String = window.title.chars().take(120).collect();
            title = Some(ContextItem::new(text, "window title", Trust::Untrusted));
        }
        (snapshot, title)
    }

    /// The live context for an agent session (MEM-11): the whole block for a new session, then
    /// only the deltas since its last prompt (empty when nothing changed).
    pub(super) fn agent_live_context(
        &self,
        session: &str,
        config: &kivo_core::KivoConfig,
    ) -> String {
        let (snapshot, title) = self.live_snapshot(config);
        let mut state = lock(&self.session);
        let text = match state.agents.get(session) {
            Some(before) => {
                let deltas = snapshot.deltas(before);
                if deltas.is_empty() {
                    String::new()
                } else {
                    format!("Since the last message: {}", deltas.join("; "))
                }
            }
            None => {
                let mut block = snapshot.block();
                if let Some(title) = title {
                    block.push('\n');
                    block.push_str(&title.render());
                }
                block
            }
        };
        state.agents.insert(session.to_owned(), snapshot);
        text
    }

    /// Builds a request in the CONV-05 order within the model's budget. Returns the request, the
    /// tokens used, the budget and how many older turns should be compacted.
    #[allow(clippy::too_many_arguments, reason = "one request's inputs")]
    fn brain_request(
        &self,
        config: &kivo_core::KivoConfig,
        route: &Route,
        attempt: &ModelRef,
        kind: ProviderKind,
        text: &str,
        thread: Option<&str>,
        extra: &[Message],
        voice: bool,
    ) -> (ChatRequest, u32, u32, usize) {
        let profile = self
            .brains
            .profiles()
            .into_iter()
            .find(|p| p.id == route.profile);
        let persona_id = profile
            .as_ref()
            .and_then(|p| p.persona.clone())
            .unwrap_or_else(|| config.brains.persona.clone());
        let persona = persona::persona(&persona_id, &config.brains.custom_persona);
        let addendum = profile
            .as_ref()
            .map(|p| p.system_prompt_addendum.clone())
            .unwrap_or_default();
        let mut layers = Layers {
            system: persona::system_prompt(&persona, voice, &addendum),
            ..Layers::default()
        };
        // Stated preferences ("call me Sam", "use metric", MEM-03) and, for voice, the words
        // speech recognition may get wrong, so the brain can repair them (VOICE-23).
        let (preferences, vocabulary) = self
            .recorder_db(|db| Ok((db.preferences()?, db.vocabulary(40)?)))
            .unwrap_or_default();
        let mut instructions = String::new();
        if !preferences.is_empty() {
            instructions.push_str("The user's stated preferences:");
            for (key, value) in &preferences {
                instructions.push_str(&format!("\n- {key}: {value}"));
            }
        }
        if voice && !vocabulary.is_empty() {
            instructions.push_str(&format!(
                "\nThe request was transcribed from speech; if a word looks misheard, it may be \
                 one of the user's words: {}.",
                vocabulary.join(", ")
            ));
        }
        layers.instructions = instructions;
        // Live context: the whole block, plus what changed since this thread's last request.
        let (snapshot, title) = self.live_snapshot(config);
        let mut live = snapshot.block();
        if let Some(title) = title {
            live.push('\n');
            live.push_str(&title.render());
        }
        {
            let mut session = lock(&self.session);
            let key = thread.unwrap_or("session").to_owned();
            if let Some((last_thread, before)) = &session.snapshot
                && *last_thread == key
            {
                let deltas = snapshot.deltas(before);
                if !deltas.is_empty() {
                    live.push_str("\nSince the last message: ");
                    live.push_str(&deltas.join("; "));
                }
            }
            session.snapshot = Some((key, snapshot));
        }
        layers.live = live;
        // The thread: its running summary, the turns since, recalled older messages.
        match thread {
            Some(id) => {
                let (conversation, messages) = self
                    .recorder_db(|db| Ok((db.conversation(id)?, db.messages(id)?)))
                    .unwrap_or_default();
                let summarized = conversation.as_ref().map_or(0, |c| c.summarized);
                layers.summary = conversation.map(|c| c.summary).unwrap_or_default();
                for m in messages.iter().filter(|m| m.id > summarized) {
                    layers.turns.push(match m.role.as_str() {
                        "assistant" => Message::assistant(&m.text),
                        _ => Message::user(&m.text),
                    });
                }
                // Older messages of this thread that match the request (CONV-05 item 7).
                if summarized > 0 {
                    let found = self
                        .recorder_db(|db| db.search_messages(text, 20))
                        .unwrap_or_default();
                    layers.recalled = found
                        .into_iter()
                        .filter(|m| m.conversation_id == id && m.id <= summarized)
                        .take(3)
                        .map(|m| {
                            ContextItem::new(m.text, format!("earlier {}", m.role), Trust::User)
                        })
                        .collect();
                }
            }
            None => {
                layers.turns = lock(&self.session).memory.clone();
                layers.turns.push(Message::user(text));
            }
        }
        // Files attached to this request: data for the brain, fenced as untrusted (SECURITY §4).
        let attachments = lock(&self.turn)
            .as_ref()
            .map(|t| t.attachments.clone())
            .unwrap_or_default();
        if !attachments.is_empty()
            && let Some(last) = layers.turns.iter_mut().rev().find(|m| m.role == Role::User)
        {
            for (name, content) in &attachments {
                let item = ContextItem::new(
                    content.clone(),
                    format!("attached file {name}"),
                    Trust::Untrusted,
                );
                last.parts.push(Part::Text {
                    text: format!("\n\n{}", item.render()),
                });
            }
        }
        layers.turns.extend(extra.iter().cloned());
        // Tools, by task class and relevance, at most 20 (BRAIN-27).
        let capabilities = config.capabilities.clone();
        // A tainted turn gets only what the task needs, and nothing that sends, deletes or runs
        // commands (SEC-15).
        let tainted = matches!(self.taint(), Taint::Tainted(_));
        let candidates: Vec<ToolCandidate> = self
            .registry
            .available(&capabilities)
            .filter(|spec| !tainted || !risky_after_taint(spec))
            .map(|spec| ToolCandidate {
                id: spec.id.clone(),
                description: spec.description.clone(),
                params: spec.params.clone(),
            })
            .collect();
        let scope = profile
            .as_ref()
            .map_or(kivo_brain::routing::ToolScope::All, |p| {
                p.allowed_tools.clone()
            });
        let task = classify_task(text);
        let limit = if tainted { 8 } else { 20 };
        layers.tools = context::select_tools(&candidates, text, task, &scope, limit);
        // Only what was offered can be called (SEC-15): a brain naming another tool is refused.
        if let Some(t) = lock(&self.turn).as_mut() {
            t.offered = layers
                .tools
                .iter()
                .map(|d| context::tool_id(&d.name))
                .collect();
        }
        let window = self.brains.window(&attempt.provider, &attempt.model);
        let class = model_class(kind, window);
        let budget_tokens = budget(
            class,
            window,
            voice,
            profile.as_ref().and_then(|p| p.max_context),
        );
        let assembled = context::assemble(&layers, budget_tokens);
        let used = assembled.total();
        let request = ChatRequest {
            model: attempt.model.clone(),
            system: assembled.system,
            messages: assembled.messages,
            tools: assembled.tools,
            max_tokens: if voice && task != TaskClass::Coding {
                VOICE_MAX_TOKENS
            } else {
                TEXT_MAX_TOKENS
            },
            temperature: None,
        };
        (request, used, budget_tokens, assembled.to_compact)
    }

    /// One tool call from a brain, through the permission engine (invariant 4): decided now,
    /// run later by `finish_brain_tool`.
    fn prepare_brain_tool(
        &self,
        wire_id: &str,
        name: &str,
        args: serde_json::Value,
        n: usize,
    ) -> Prepared {
        let tool_id = context::tool_id(name);
        let mut call = ToolCall {
            id: format!("{}-b{n}", self.turn_key()),
            tool: tool_id,
            targets: Vec::new(),
            args,
            initiated_by: Initiator::Brain,
        };
        let wire = (wire_id.to_owned(), name.to_owned());
        let offered = lock(&self.turn)
            .as_ref()
            .is_none_or(|t| t.offered.contains(&call.tool));
        if !offered {
            let reason = text::t("brain.toolNotOffered");
            self.recorder.tool_denied(&self.turn_key(), &call, &reason);
            return Prepared::Done(wire, reason);
        }
        let capabilities = self.core.config().capabilities;
        let Some(tool) = self.registry.get(&call.tool, &capabilities) else {
            let reason = self.registry.known(&call.tool).map_or_else(
                || "no such tool".to_owned(),
                |spec| {
                    text::tf(
                        "policy.capabilityOff",
                        &[("capability", &spec.capability.label())],
                    )
                },
            );
            self.recorder.tool_denied(&self.turn_key(), &call, &reason);
            return Prepared::Done(wire, text::tf("brain.toolMissing", &[("reason", &reason)]));
        };
        let decision = self.authorize_call(tool.as_ref(), &mut call);
        self.mark("t7Permission");
        self.recorder
            .tool_decision(&self.turn_key(), &call, tool.spec(), &decision);
        match decision {
            Decision::Allow(permit) => Prepared::Run {
                wire,
                tool,
                call,
                permit,
            },
            Decision::Deny(denial) => Prepared::Done(wire, denial.message),
            Decision::Confirm(confirm) => Prepared::Ask {
                wire,
                tool,
                call,
                confirm,
            },
        }
    }

    /// Runs a prepared call (asking first if it must) and returns what to tell the brain, or
    /// `None` when the turn ended (declined or cancelled).
    async fn finish_brain_tool(
        self: &Arc<Self>,
        prepared: Prepared,
        approved: &mut std::collections::HashMap<String, kivo_security::Permit>,
    ) -> Option<Vec<Part>> {
        let result = |wire: &(String, String), content: String, is_error: bool| Part::ToolResult {
            id: wire.0.clone(),
            name: wire.1.clone(),
            content,
            is_error,
        };
        let (wire, tool, call, permit) = match prepared {
            Prepared::Done(wire, message) => return Some(vec![result(&wire, message, true)]),
            Prepared::Run {
                wire,
                tool,
                call,
                permit,
            } => (wire, tool, call, permit),
            Prepared::Ask {
                wire,
                tool,
                call,
                confirm,
            } => {
                // Approved with the plan: exactly this step.
                if let Some(permit) = approved.remove(&call.id) {
                    (wire, tool, call, permit)
                } else {
                    let (allow, _, by) = self.decide(confirm.clone(), call.clone()).await?;
                    if !allow {
                        return None;
                    }
                    match kivo_security::confirmed(&confirm, &call, Answer::Allow { by }) {
                        Ok(permit) => (wire, tool, call, permit),
                        Err(denial) => return Some(vec![result(&wire, denial.message, true)]),
                    }
                }
            }
        };
        let state = self.core.state().borrow().session;
        if matches!(state, SessionState::Thinking | SessionState::Speaking) {
            self.core.advance(SessionInput::StartActing);
        }
        let title = tool.spec().title.clone();
        let (outcome, output) = self.run_step(tool, &call, permit, &title).await;
        Some(match (outcome.status, output) {
            (Ok(_), Some(output)) => {
                let body =
                    json!({ "ok": true, "said": output.say, "data": output.data }).to_string();
                // Someone else's content goes to the brain fenced and labelled (SEC-15).
                let content = match &output.source {
                    Some(source) => {
                        ContextItem::new(body, source.clone(), Trust::Untrusted).render()
                    }
                    None => body,
                };
                let mut parts = vec![result(&wire, content, false)];
                if let Some(png) = output.image
                    && self.vision.load(std::sync::atomic::Ordering::SeqCst)
                {
                    use base64::Engine as _;
                    let source = output.source.clone().unwrap_or_default();
                    let brain = lock(&self.turn)
                        .as_ref()
                        .and_then(|_| self.core.state().borrow().turn.clone())
                        .and_then(|t| t.brain.map(|b| b.name))
                        .unwrap_or_default();
                    let note = text::tf(
                        "brain.sentScreenshot",
                        &[("source", &source), ("brain", &brain)],
                    );
                    self.recorder.screenshot_sent(&self.turn_key(), &note);
                    self.core.update_turn(|view| view.note = Some(note.clone()));
                    parts.push(Part::Image {
                        media_type: "image/png".into(),
                        data: base64::engine::general_purpose::STANDARD.encode(png),
                    });
                }
                parts
            }
            (Ok(_), None) => vec![result(&wire, json!({ "ok": true }).to_string(), false)],
            (Err(error), _) => vec![result(&wire, error.message, true)],
        })
    }

    /// A phrase of a streamed answer: spoken now, or queued behind the one playing (BRAIN-28).
    pub(super) async fn say_phrase(self: &Arc<Self>, phrase: String) {
        if !self.speak_replies() {
            return;
        }
        {
            let mut turn = lock(&self.turn);
            let Some(t) = turn.as_mut() else { return };
            t.reply.push_str(&phrase);
            t.reply.push(' ');
            t.spoken_any = true;
            if t.speaking.is_some() || !t.phrases.is_empty() {
                t.phrases.push_back(phrase);
                return;
            }
            // Claimed now, so the next phrase queues behind this one.
            t.speaking = Some(0);
        }
        self.start_phrase(phrase).await;
    }

    /// Starts speaking one phrase.
    pub(super) async fn start_phrase(self: &Arc<Self>, phrase: String) {
        let state = self.core.state().borrow().session;
        if matches!(state, SessionState::Thinking | SessionState::Acting) {
            self.core.advance(SessionInput::StartSpeaking);
        }
        if !self.infer.is_ready() {
            let Some(cancel) = lock(&self.turn).as_ref().map(|t| t.cancel.clone()) else {
                return;
            };
            let ready = tokio::select! {
                ready = self.infer.wait_ready(super::WORKER_START) => ready,
                () = cancel.cancelled() => return,
            };
            if !ready {
                // No worker: Windows' voice in this process says it instead.
                self.say_in_process(&phrase);
                self.speech_done_local();
                return;
            }
        }
        let id = self.infer.next_utterance();
        let first = {
            let mut turn = lock(&self.turn);
            let Some(t) = turn.as_mut() else { return };
            t.speaking = Some(id);
            !t.spans.contains_key("t9FirstAudio")
        };
        if first {
            self.core.bus.publish(Event::new(EventKind::Voice(
                kivo_core::event::VoiceEvent::TtsStarted,
            )));
        }
        let voice = self.core.config().voice.tts_voice;
        let voice = (!voice.is_empty()).then_some(voice);
        if let Err(e) = self.infer.speak(id, &phrase, voice.as_deref()).await {
            tracing::warn!(%e, "couldn't speak");
            self.speech_done_local();
        }
    }

    /// A phrase couldn't go to the worker: move on as if it had been spoken.
    fn speech_done_local(self: &Arc<Self>) {
        let id = lock(&self.turn).as_ref().and_then(|t| t.speaking);
        if let Some(id) = id {
            self.speech_done(id, None, false);
        }
    }

    /// The brain's answer is complete: the turn ends once the last phrase has been spoken.
    pub(super) fn end_stream(self: &Arc<Self>) {
        let (idle, spoke) = {
            let mut turn = lock(&self.turn);
            let Some(t) = turn.as_mut() else { return };
            t.streaming = false;
            (t.speaking.is_none() && t.phrases.is_empty(), t.spoken_any)
        };
        if idle {
            if !spoke {
                // Nothing is spoken (typed turn, quiet mode): a soft cue instead (VOICE §6).
                self.speaker.cue(Cue::Done);
            }
            self.finish_speaking();
        }
    }

    /// Summarizes the oldest turns of a thread into its running summary, with the cheapest
    /// suitable brain, after the answer (CONV-06).
    fn compact_later(self: &Arc<Self>, thread: String, turns: usize) {
        let engine = Arc::clone(self);
        tokio::spawn(async move {
            if let Err(e) = engine.compact(&thread, Some(turns)).await {
                tracing::debug!(%e, "compaction skipped");
            }
        });
    }

    /// Compacts `thread`: the oldest `turns` unsummarized messages (all but the last four when
    /// `None`, "Compact now") go into the running summary. The full history stays in SQLite.
    pub async fn compact(
        self: &Arc<Self>,
        thread: &str,
        turns: Option<usize>,
    ) -> Result<(), String> {
        let config = self.core.config();
        let (conversation, messages) = self
            .recorder_db(|db| Ok((db.conversation(thread)?, db.messages(thread)?)))
            .ok_or("no history")?;
        let conversation = conversation.ok_or("no such conversation")?;
        let open: Vec<_> = messages
            .into_iter()
            .filter(|m| m.id > conversation.summarized)
            .collect();
        let count = turns
            .unwrap_or_else(|| open.len().saturating_sub(context::KEEP_TURNS))
            .min(open.len().saturating_sub(context::KEEP_TURNS));
        if count == 0 {
            return Ok(());
        }
        let batch = &open[..count];
        // The cheapest suitable brain: a local one when there is one (never a CLI agent).
        let route = self
            .brains
            .route(&config, "summarize", Some("cheap"), true)
            .or_else(|_| {
                self.brains
                    .route(&config, "summarize", Some("cheap"), false)
            })
            .map_err(|e| e.to_string())?;
        if route.kind == ProviderKind::Cli {
            return Err("no chat brain for summaries".into());
        }
        let provider = self
            .brains
            .provider(&route.target.provider)
            .ok_or("not connected")?;
        let mut transcript = String::new();
        if !conversation.summary.is_empty() {
            transcript.push_str(&format!("Summary so far: {}\n\n", conversation.summary));
        }
        for m in batch {
            transcript.push_str(&format!("{}: {}\n", m.role, m.text));
        }
        let request = ChatRequest {
            model: route.target.model.clone(),
            system: vec![kivo_brain::SystemBlock {
                text: text::t("brain.summarize"),
                cacheable: true,
            }],
            messages: vec![Message::user(transcript)],
            tools: Vec::new(),
            max_tokens: 400,
            temperature: Some(0.2),
        };
        let collected =
            kivo_brain::collect(provider.chat(request, tokio_util::sync::CancellationToken::new()))
                .await;
        if let Some(e) = collected.error {
            return Err(e.to_string());
        }
        self.brains.meter(&Meter {
            kind: "brain",
            provider: &route.target.provider,
            model: &route.target.model,
            profile: Some("cheap"),
            usage: collected.usage,
            turn: None,
            task: None,
            routine: None,
        });
        let summary = collected.text.trim();
        if summary.is_empty() {
            return Err("empty summary".into());
        }
        let last = batch.last().map_or(conversation.summarized, |m| m.id);
        self.recorder_db(|db| db.set_summary(thread, summary, last));
        Ok(())
    }

    /// "No, make it 30 %" for a decision: the pending action is dropped and the corrected
    /// request goes to a brain (CONV-27 "edit").
    pub(super) async fn edit_request(self: &Arc<Self>, spec: &ConfirmSpec, change: &str) {
        let original = {
            let mut turn = lock(&self.turn);
            let Some(t) = turn.as_mut() else { return };
            t.pending = None;
            t.waiter = None;
            t.transcript.clone()
        };
        self.core.update_turn(|view| {
            view.confirm = None;
            view.waiting = false;
        });
        self.recorder.answer(
            &self.turn_key(),
            &format!("{} → {change}", spec.action),
            "edited",
        );
        // Back to thinking with the corrected request.
        let state = self.core.state().borrow().session;
        if state == SessionState::AwaitingConfirmation {
            self.core.advance(SessionInput::Confirmed);
        }
        let corrected = format!(
            "{original}\n(The user changed this request before it ran: \"{change}\". The action \
             that was waiting, \"{}\", was not done.)",
            spec.action
        );
        self.brain_turn(&corrected).await;
    }

    /// What a request would start with, layer by layer (Settings → Context, CONV-30): each
    /// layer's text and size, the tools offered, and the default brain's budget.
    pub fn context_preview(&self, thread: Option<&str>) -> serde_json::Value {
        let config = self.core.config();
        let route = self.brains.route(&config, "", None, false).ok();
        let (kind, window, profile) = route.as_ref().map_or((ProviderKind::Api, None, None), |r| {
            (
                r.kind,
                self.brains.window(&r.target.provider, &r.target.model),
                self.brains
                    .profiles()
                    .into_iter()
                    .find(|p| p.id == r.profile),
            )
        });
        let persona_id = profile
            .as_ref()
            .and_then(|p| p.persona.clone())
            .unwrap_or_else(|| config.brains.persona.clone());
        let persona = persona::persona(&persona_id, &config.brains.custom_persona);
        let addendum = profile
            .as_ref()
            .map(|p| p.system_prompt_addendum.clone())
            .unwrap_or_default();
        let system = persona::system_prompt(&persona, true, &addendum);
        let preferences = self.recorder_db(|db| db.preferences()).unwrap_or_default();
        let instructions = preferences
            .iter()
            .map(|(k, v)| format!("- {k}: {v}"))
            .collect::<Vec<_>>()
            .join("\n");
        let (snapshot, _) = self.live_snapshot(&config);
        let live = snapshot.block();
        let (summary, turns) = thread
            .and_then(|id| {
                self.recorder_db(|db| {
                    let c = db.conversation(id)?;
                    let m = db.messages(id)?;
                    Ok((c, m))
                })
            })
            .map_or((String::new(), 0), |(c, m)| {
                let summarized = c.as_ref().map_or(0, |c| c.summarized);
                let text: u32 = m
                    .iter()
                    .filter(|m| m.id > summarized)
                    .map(|m| context::tokens(&m.text) + 4)
                    .sum();
                (c.map(|c| c.summary).unwrap_or_default(), text)
            });
        let capabilities = config.capabilities.clone();
        let tools: Vec<(String, u32)> = self
            .registry
            .available(&capabilities)
            .map(|s| {
                (
                    s.id.clone(),
                    context::tokens(&s.description) + context::tokens(&s.params.to_string()) + 8,
                )
            })
            .collect();
        let class = model_class(kind, window);
        let budget_voice = budget(
            class,
            window,
            true,
            profile.as_ref().and_then(|p| p.max_context),
        );
        let budget_chat = budget(
            class,
            window,
            false,
            profile.as_ref().and_then(|p| p.max_context),
        );
        json!({
            "brain": route.as_ref().map(|r| r.target_name.clone()),
            "budget": { "voice": budget_voice, "chat": budget_chat },
            "layers": [
                { "id": "system", "text": system, "tokens": context::tokens(&system), "max": context::SYSTEM_MAX },
                { "id": "instructions", "text": instructions, "tokens": context::tokens(&instructions), "max": context::INSTRUCTIONS_MAX },
                { "id": "live", "text": live, "tokens": context::tokens(&live), "max": context::LIVE_MAX },
                { "id": "summary", "text": summary, "tokens": context::tokens(&summary), "max": context::SUMMARY_MAX },
                { "id": "turns", "tokens": turns },
                { "id": "tools", "tokens": tools.iter().map(|t| t.1).sum::<u32>(), "count": tools.len(), "max": 20 },
            ],
        })
    }

    /// "That's not what I meant" on a card (BRAIN-06): the request, its route and the note are
    /// kept for improving routing, and counted in the router's metrics.
    pub fn misroute(&self, turn: &str, note: &str) -> serde_json::Value {
        let count = self
            .recorder_db(|db| {
                let record = db.turn(turn)?;
                let (transcript, route) = record
                    .map(|r| {
                        (
                            r.transcript.unwrap_or_default(),
                            r.route.unwrap_or_default(),
                        )
                    })
                    .unwrap_or_default();
                db.add_misroute(turn, &transcript, &route, note)?;
                db.misroute_count()
            })
            .unwrap_or(0);
        lock(&self.router).misrouted();
        self.recorder
            .brain_problem(turn, "That's not what I meant", note);
        json!({ "misroutes": count })
    }

    /// Stops a brain turn from outside (the Chat page's Stop), like Esc.
    pub fn stop_brain(&self) {
        self.cancel(CancelReason::UserButton);
    }
}

/// A brain's tool call after the permission engine decided: `(wire id, wire name)` travels
/// with it so the result goes back under the brain's own names.
pub(super) enum Prepared {
    Done((String, String), String),
    Run {
        wire: (String, String),
        tool: Arc<dyn kivo_tools::Tool>,
        call: ToolCall,
        permit: kivo_security::Permit,
    },
    Ask {
        wire: (String, String),
        tool: Arc<dyn kivo_tools::Tool>,
        call: ToolCall,
        confirm: ConfirmSpec,
    },
}

/// Tools a tainted turn doesn't get: anything that sends, deletes, spends, runs commands or
/// turns the PC off (SEC-15).
fn risky_after_taint(spec: &kivo_core::tool::ToolSpec) -> bool {
    use kivo_core::tool::SideEffect;
    spec.data_egress
        || spec.side_effects.iter().any(|e| {
            matches!(
                e,
                SideEffect::ExternalComms
                    | SideEffect::Destructive
                    | SideEffect::Financial
                    | SideEffect::SecuritySensitive
            )
        })
        || matches!(
            spec.capability,
            kivo_core::Capability::Shell | kivo_core::Capability::PowerActions
        )
}

/// What the spending limits allow.
enum Spend {
    Go(Option<String>),
    Rerouted(Box<Route>, Option<String>),
    Stop,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_topic_is_found_and_stripped() {
        assert_eq!(new_topic("New topic"), Some(String::new()));
        assert_eq!(
            new_topic("Kivo, new topic: what's the weather?"),
            Some("what's the weather?".into())
        );
        assert_eq!(
            new_topic("Start over. Tell me a joke"),
            Some("Tell me a joke".into())
        );
        assert_eq!(new_topic("what's a new topic for my essay"), None);
    }

    #[test]
    fn follow_ups_stay_on_topic_and_new_subjects_do_not() {
        let earlier = vec!["What's the weather in Paris tomorrow?".to_owned()];
        assert!(same_topic("And the weekend in Paris?", &earlier));
        assert!(same_topic("what about Sunday", &earlier));
        assert!(same_topic("Will Paris be rainy", &earlier));
        assert!(!same_topic("Explain Rust lifetimes", &earlier));
    }

    #[test]
    fn local_time_reads_as_a_weekday_date_and_clock() {
        let t = local_time(0);
        assert!(
            [
                "Monday",
                "Tuesday",
                "Wednesday",
                "Thursday",
                "Friday",
                "Saturday",
                "Sunday"
            ]
            .iter()
            .any(|d| t.starts_with(d)),
            "{t}"
        );
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(19_723), (2024, 1, 1));
    }

    #[test]
    fn failures_are_said_plainly() {
        assert_eq!(
            failure_message(
                "Claude",
                &NormalizedError::RateLimited { retry_after: None }
            ),
            "Claude is busy right now. Try again in a moment."
        );
        assert!(failure_message("Ollama", &NormalizedError::Network("x".into())).contains("reach"));
    }
}
