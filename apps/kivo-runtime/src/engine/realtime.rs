//! Realtime conversation mode (BRAINS §8, BRAIN-33): a speech-to-speech brain hears the user and
//! speaks back directly, for the most natural turn-taking.
//!
//! - **The fast path runs first.** A session opens only from a request the grammar didn't take
//!   (so "mute" never opens one): a conversation-type request ("let's talk about…", "practice my
//!   interview"), a voice request on a profile set to Realtime, or the card's "Talk live".
//! - **What stays local:** the wake word, "Kivo stop" and the permission engine. The session's
//!   microphone has KIVO's own voice removed first (`LiveMic`).
//! - **Tools** the model asks for go through `authorize()` like any brain's (invariant 4), from the
//!   same offered set. While a decision is pending, nothing more of the model is played: its
//!   events wait until the user answers. A declined action is only that action: the conversation
//!   goes on.
//! - **It closes** after the chosen seconds of silence (15 by default), when the user says
//!   "that's all", at the realtime cap (30 minutes by default, BRAINS §9), or on Stop.
//! - **Transcripts** show in the card and go to the thread like any conversation; usage is metered
//!   as `realtime`.

use super::{Engine, lock};
use crate::brains::Meter;
use crate::speaker::Cue;
use kivo_brain::realtime::{Control, RealtimeConfig, RealtimeProvider, RtEvent};
use kivo_brain::routing::Route;
use kivo_brain::{Part, PrivacyClass, Role};
use kivo_core::event::{ProviderEvent, TurnSource};
use kivo_core::text;
use kivo_core::{SessionInput, SessionState};
use kivo_ipc::protocol::{BrainChip, LiveView};
use kivo_store::brains::now_ms;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The card's answer is refreshed at most this often while the model talks.
const CARD_EVERY: Duration = Duration::from_millis(60);
/// Shortest silence that closes a session, whatever the settings say.
const MIN_SILENCE: Duration = Duration::from_secs(5);
/// Thread messages carried into a new session's instructions.
const CARRIED_MESSAGES: usize = 12;

/// Why a session ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ended {
    Silence,
    UserSaidSo,
    Cap,
    Cancelled,
    Dropped,
}

impl Ended {
    fn key(self) -> &'static str {
        match self {
            Self::Silence => "silence",
            Self::UserSaidSo => "user",
            Self::Cap => "cap",
            Self::Cancelled => "cancelled",
            Self::Dropped => "dropped",
        }
    }
}

/// Lower case, letters, digits and apostrophes, single spaces (as the privacy word lists).
fn normalize(text: &str) -> String {
    text.to_lowercase()
        .replace('’', "'")
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn has_phrase(text: &str, key: &str) -> bool {
    let padded = format!(" {} ", normalize(text));
    text::t(key)
        .split('|')
        .map(normalize)
        .filter(|w| !w.is_empty())
        .any(|w| padded.contains(&format!(" {w} ")))
}

/// A conversation-type request ("let's talk about Rome", "practice my interview").
pub fn wants_conversation(text: &str) -> bool {
    has_phrase(text, "realtime.openers")
}

/// The user ends the conversation ("that's all", "goodbye"): only a short line that is nothing
/// but the closing words counts, so "that's all I need to know about X" doesn't end it.
pub fn ends_conversation(text: &str) -> bool {
    let words = normalize(text);
    words.split(' ').count() <= 5 && has_phrase(text, "realtime.closers")
}

impl Engine {
    /// A realtime session is open.
    pub fn live(&self) -> bool {
        lock(&self.turn).as_ref().is_some_and(|t| t.live)
    }

    /// Whether this request opens a realtime conversation, and with which provider.
    pub(super) fn live_wanted(
        &self,
        text: &str,
        route: &Route,
        forced: bool,
    ) -> Option<Arc<dyn RealtimeProvider>> {
        let config = self.core.config();
        // A request that must stay on the PC went to a local brain: never realtime.
        if route.privacy != PrivacyClass::Cloud {
            return None;
        }
        let provider = self.brains.realtime_provider(&config)?;
        let voice = lock(&self.turn)
            .as_ref()
            .is_some_and(|t| t.source != TurnSource::Typed);
        let profile_live = voice
            && self
                .brains
                .profiles()
                .iter()
                .any(|p| p.id == route.profile && p.realtime);
        (forced || profile_live || wants_conversation(text)).then_some(provider)
    }

    /// Whether the card may offer "Talk live" (the capability is on and a provider is ready).
    pub(super) fn live_available(&self) -> bool {
        self.brains.realtime_provider(&self.core.config()).is_some()
    }

    /// The card's "Talk live": a new turn that opens a realtime conversation at once.
    pub async fn start_live(self: &Arc<Self>) -> Result<(), String> {
        let config = self.core.config();
        let Some(provider) = self.brains.realtime_provider(&config) else {
            return Err(text::t("realtime.unavailable"));
        };
        let route = self
            .brains
            .route(&config, "", None, false)
            .map_err(|e| e.to_string())?;
        if route.privacy != PrivacyClass::Cloud {
            return Err(text::t("realtime.unavailable"));
        }
        let opening = text::t("realtime.cardOpening");
        let id = self.turn_id();
        self.core
            .begin_turn(&id, TurnSource::Typed, &opening)
            .map_err(|e| e.0)?;
        self.anchor_island();
        *lock(&self.turn) = Some(super::Running {
            transcript: opening,
            ..super::Running::new(id.clone(), 0, TurnSource::Typed)
        });
        self.recorder.turn_started(&id, TurnSource::Typed);
        self.core.advance(SessionInput::EndOfSpeech);
        let engine = Arc::clone(self);
        tokio::spawn(async move { engine.realtime_turn(None, route, provider).await });
        Ok(())
    }

    /// Moves the session state to `want` through the transitions the state machine has.
    fn live_state(&self, want: SessionState) {
        let now = self.core.state().borrow().session;
        if now == want {
            return;
        }
        match want {
            SessionState::Listening => {
                if matches!(
                    now,
                    SessionState::Idle
                        | SessionState::Thinking
                        | SessionState::Acting
                        | SessionState::Speaking
                ) {
                    self.core.advance(SessionInput::LiveListening);
                }
            }
            SessionState::Thinking => {
                if now == SessionState::Listening {
                    self.core.advance(SessionInput::EndOfSpeech);
                } else if now == SessionState::Idle {
                    self.core.advance(SessionInput::LiveListening);
                    self.core.advance(SessionInput::EndOfSpeech);
                }
            }
            SessionState::Speaking => {
                self.live_state(SessionState::Thinking);
                if matches!(
                    self.core.state().borrow().session,
                    SessionState::Thinking | SessionState::Acting
                ) {
                    self.core.advance(SessionInput::StartSpeaking);
                }
            }
            _ => {}
        }
    }

    /// A realtime conversation, from opening to close. `opening` is the request that opened it
    /// (the card's "Talk live" has none).
    pub(super) async fn realtime_turn(
        self: &Arc<Self>,
        opening: Option<&str>,
        route: Route,
        provider: Arc<dyn RealtimeProvider>,
    ) {
        let config = self.core.config();
        let turn = self.turn_key();
        let Some((cancel, guest, chosen)) = lock(&self.turn)
            .as_ref()
            .map(|t| (t.cancel.clone(), t.guest, t.thread.clone()))
        else {
            return;
        };
        let keep = config.privacy.retention_days > 0 && !guest;
        let thread = self.pick_thread(opening.unwrap_or_default(), chosen, keep, &config);
        // The instructions and tools a brain would get (persona, live context, memory, the
        // offered tools), and the thread so far.
        let attempt = route.target.clone();
        let (request, ..) = self.brain_request(
            &config,
            &route,
            &attempt,
            route.kind,
            opening.unwrap_or_default(),
            thread.as_deref(),
            &[],
            true,
            false,
        );
        let mut instructions = request.system_text();
        instructions.push_str("\n\n");
        instructions.push_str(&text::t("realtime.instructions"));
        let earlier: Vec<String> = request
            .messages
            .iter()
            .rev()
            .skip(usize::from(opening.is_some()))
            .take(CARRIED_MESSAGES)
            .filter_map(|m| {
                let said: String = m
                    .parts
                    .iter()
                    .filter_map(|p| match p {
                        Part::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                let who = match m.role {
                    Role::User => "User",
                    Role::Assistant => "You",
                    _ => return None,
                };
                (!said.trim().is_empty()).then(|| format!("{who}: {}", said.trim()))
            })
            .collect();
        if !earlier.is_empty() {
            instructions.push_str("\n\nEarlier in this conversation:\n");
            instructions.push_str(&earlier.into_iter().rev().collect::<Vec<_>>().join("\n"));
        }
        let model = if config.brains.realtime.model.is_empty() {
            provider.default_model().to_owned()
        } else {
            config.brains.realtime.model.clone()
        };
        let setup = RealtimeConfig {
            model: model.clone(),
            instructions,
            tools: request.tools.clone(),
            voice: config.brains.realtime.voice.clone(),
        };
        let name = provider.name();
        let profile_name = self
            .brains
            .profiles()
            .into_iter()
            .find(|p| p.id == route.profile)
            .map_or_else(|| route.profile.clone(), |p| p.name);
        let cap_minutes = self.brains.task_caps().realtime_minutes;
        self.core.update_turn(|view| {
            view.brain = Some(BrainChip {
                name: name.clone(),
                profile: profile_name.clone(),
                reason: text::tf("realtime.reason", &[("profile", &profile_name)]),
                local: false,
                ..BrainChip::default()
            });
            view.live_offer = false;
        });
        let session = tokio::select! {
            s = provider.connect(setup, cancel.child_token()) => s,
            () = cancel.cancelled() => return,
        };
        let mut session = match session {
            Ok(s) => s,
            Err(error) => {
                let message = super::brain::failure_message(&name, &error);
                self.recorder
                    .brain_problem(&turn, &message, &error.to_string());
                self.speaker.cue(Cue::Error);
                self.core
                    .update_turn(|view| view.error = Some(message.clone()));
                self.speak_and_finish(&message).await;
                return;
            }
        };
        let device = config
            .voice
            .input_device
            .clone()
            .map(kivo_platform::DeviceId);
        let mic = match crate::live_mic::LiveMic::start(
            self.speaker.audio(),
            device,
            Arc::clone(&self.speaker),
            session.audio.clone(),
        ) {
            Ok(mic) => mic,
            Err(e) => {
                tracing::warn!(%e, "couldn't open the microphone for a live conversation");
                let _ = session.control.send(Control::Close);
                self.fail_turn(&text::t("turn.micBlocked"));
                return;
            }
        };
        if let Some(t) = lock(&self.turn).as_mut() {
            t.live = true;
            t.cloud_brain = Some(name.clone());
        }
        let started = Instant::now();
        let started_ms = now_ms();
        self.core.update_turn(|view| {
            view.live = Some(LiveView {
                provider: name.clone(),
                started: u64::try_from(started_ms).unwrap_or_default(),
                max_minutes: cap_minutes,
            });
        });
        self.speaker.cue(Cue::ListenStart);
        if let Some(text) = opening {
            let _ = session.control.send(Control::Text(text.to_owned()));
            if let Some(id) = &thread {
                let _ = self.recorder_db(|db| db.add_message(id, "user", text, None, Some(&turn)));
            }
        }
        let silence =
            Duration::from_secs(u64::from(config.brains.realtime.silence_seconds)).max(MIN_SILENCE);
        let cap = cap_minutes.map(|m| Duration::from_secs(u64::from(m) * 60));
        let mut last_heard = Instant::now();
        let mut answer = String::new();
        let mut shown = Instant::now() - CARD_EVERY;
        let mut cost = 0.0;
        let mut known_cost = true;
        let mut calls = 0usize;
        let mut exchanges = 0u32;
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        let ended = loop {
            tokio::select! {
                () = cancel.cancelled() => break Ended::Cancelled,
                _ = tick.tick() => {
                    if self.speaker.speaking() {
                        last_heard = Instant::now();
                    } else if self.core.state().borrow().session == SessionState::Speaking {
                        // The model finished talking: listen again.
                        self.live_state(SessionState::Listening);
                    }
                    if last_heard.elapsed() >= silence {
                        break Ended::Silence;
                    }
                    if cap.is_some_and(|c| started.elapsed() >= c) {
                        break Ended::Cap;
                    }
                }
                event = session.events.recv() => {
                    let Some(event) = event else { break Ended::Dropped };
                    match event {
                        RtEvent::Ready => self.live_state(SessionState::Listening),
                        RtEvent::Audio { pcm, rate } => {
                            last_heard = Instant::now();
                            self.live_state(SessionState::Speaking);
                            if self.speak_replies() {
                                self.speaker.speak(&pcm, rate);
                            }
                            if let Some(t) = lock(&self.turn).as_mut() {
                                t.spoken_any = true;
                            }
                        }
                        RtEvent::UserSpeaking => {
                            // The user talks over the model: it stops at once (barge-in).
                            last_heard = Instant::now();
                            self.speaker.stop();
                            self.live_state(SessionState::Listening);
                        }
                        RtEvent::UserTranscript(said) => {
                            last_heard = Instant::now();
                            exchanges += 1;
                            answer.clear();
                            self.core.update_turn(|view| {
                                view.transcript.clone_from(&said);
                                view.transcript_final = true;
                                view.answer = None;
                            });
                            if let Some(id) = &thread {
                                let _ = self.recorder_db(|db| db.add_message(id, "user", &said, None, Some(&turn)));
                            }
                            if ends_conversation(&said) {
                                break Ended::UserSaidSo;
                            }
                            self.live_state(SessionState::Thinking);
                        }
                        RtEvent::ModelTranscript(delta) => {
                            answer.push_str(&delta);
                            if shown.elapsed() >= CARD_EVERY {
                                shown = Instant::now();
                                let now = answer.clone();
                                self.core.update_turn(|view| view.answer = Some(now));
                            }
                        }
                        RtEvent::ModelTurnDone => {
                            if !answer.trim().is_empty() {
                                let said = answer.trim().to_owned();
                                self.core.update_turn(|view| view.answer = Some(said.clone()));
                                self.recorder.answer(&turn, &said, "done");
                                if let Some(id) = &thread {
                                    let _ = self.recorder_db(|db| db.add_message(id, "assistant", &said, Some(&name), Some(&turn)));
                                }
                                answer.clear();
                            }
                        }
                        RtEvent::ToolCall { id, name: tool, args } => {
                            calls += 1;
                            // Through the permission engine; the model waits for the result,
                            // and nothing more of it plays while a decision is pending.
                            self.live_state(SessionState::Thinking);
                            let prepared = self.prepare_brain_tool(&id, &tool, args, calls);
                            let mut approved = std::collections::HashMap::new();
                            let output = match self.finish_brain_tool(prepared, &mut approved).await {
                                Some(parts) => parts
                                    .into_iter()
                                    .find_map(|p| match p {
                                        Part::ToolResult { content, .. } => Some(content),
                                        _ => None,
                                    })
                                    .unwrap_or_else(|| "{\"ok\":true}".into()),
                                None if cancel.is_cancelled() || lock(&self.turn).is_none() => break Ended::Cancelled,
                                None => text::t("realtime.declined"),
                            };
                            last_heard = Instant::now();
                            let _ = session.control.send(Control::ToolResult { id, name: tool, output });
                            self.live_state(SessionState::Thinking);
                        }
                        RtEvent::Usage(usage) => {
                            let spent = self.brains.meter(&Meter {
                                kind: "realtime",
                                provider: provider.id(),
                                model: &model,
                                profile: Some(&route.profile),
                                usage,
                                turn: Some(&turn),
                                task: None,
                                routine: None,
                            });
                            match spent {
                                Some(c) => cost += c,
                                None => known_cost = false,
                            }
                            self.publish_provider(ProviderEvent::UsageRecorded {
                                provider: provider.id().to_owned(),
                                cost: spent,
                            });
                            let show = config.brains.show_cost && known_cost;
                            self.core.update_turn(|view| {
                                if let Some(chip) = view.brain.as_mut() {
                                    chip.cost = show.then_some(cost);
                                }
                            });
                        }
                        RtEvent::Resumed => tracing::debug!("realtime session renewed"),
                        RtEvent::Error(error) => {
                            tracing::warn!(%error, "realtime provider reported a problem");
                        }
                        RtEvent::Closed(error) => {
                            if let Some(error) = error {
                                self.provider_failed(provider.id(), &error);
                            }
                            break Ended::Dropped;
                        }
                    }
                }
            }
        };
        drop(mic);
        let _ = session.control.send(Control::Close);
        if let Some(t) = lock(&self.turn).as_mut() {
            t.live = false;
        }
        let minutes = started.elapsed().as_secs_f64() / 60.0;
        let price = if known_cost {
            cost
        } else {
            provider.cost_per_minute() * minutes
        };
        self.recorder
            .realtime_done(&turn, &name, minutes, price, exchanges, ended.key());
        if let Some(id) = &thread {
            self.publish_provider(ProviderEvent::ThreadChanged { thread: id.clone() });
        }
        if ended == Ended::Cancelled {
            // Stop and "Kivo stop" already wound the turn down.
            return;
        }
        self.speaker.stop();
        self.speaker.cue(Cue::Hangup);
        self.core.update_turn(|view| view.live = None);
        let closing = text::t(match ended {
            Ended::Silence => "realtime.closedSilence",
            Ended::UserSaidSo => "realtime.closedUser",
            Ended::Cap => "realtime.closedCap",
            Ended::Cancelled | Ended::Dropped => "realtime.closedDropped",
        });
        self.live_state(SessionState::Thinking);
        self.speak_and_finish(&closing).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversations_open_and_close_on_their_phrases() {
        assert!(wants_conversation("Let's talk about the Roman empire"));
        assert!(wants_conversation("can you practice my interview with me"));
        assert!(!wants_conversation("open notepad"));
        assert!(!wants_conversation("mute"));
        assert!(ends_conversation("That's all, thanks"));
        assert!(ends_conversation("goodbye"));
        assert!(!ends_conversation(
            "that's all I need to know about the budget for next year"
        ));
        assert!(!ends_conversation("tell me more"));
    }
}
