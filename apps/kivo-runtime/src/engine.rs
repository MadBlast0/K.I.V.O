//! One turn, end to end (ARCHITECTURE §4.2, BRAINS §1, plan §138): push-to-talk or typed text →
//! speech recognition → the grammar fast path → the permission engine → the tool → a spoken reply,
//! with every step shown in the Island, recorded in Activity and audited.
//!
//! The turn owns a cancellation token; Stop, Esc, the emergency stop and a new turn all cancel it,
//! and cancellation reaches recognition, the tool and the speaker within 100 ms (ARCH-26).

use crate::activity::Recorder;
use crate::core::Core;
use crate::infer::{Infer, InferEvent};
use crate::speaker::{Cue, Speaker};
use crate::voice::{Listener, VoiceSignal};
use kivo_core::event::StepStatus;
use kivo_core::event::{CancelReason, EventKind, IntentPath, TurnEvent, TurnSource, VoiceEvent};
use kivo_core::text;
use kivo_core::tool::{ConfirmSpec, ConfirmedBy, Initiator, ToolCall, ToolError, ToolErrorCode};
use kivo_core::{Event, SessionInput, SessionState};
use kivo_intent::{Context as GrammarContext, Index, IndexEntry, IntentRouter, Route};
use kivo_ipc::protocol::{QuietIsland, SpeechStatus, StepView};
use kivo_platform::{Apps, Windows};
use kivo_security::{
    Answer, Context as SecurityContext, Decision, Grant, HardLimits, SessionKind, Taint,
};
use kivo_tools::{Registry, targets};
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// How long the app index is trusted before it is read again.
const APPS_TTL: Duration = Duration::from_secs(120);
/// The Island stays up this long after the answer, then clears (UX-10).
const COLLAPSE_AFTER: Duration = Duration::from_secs(4);
/// How long a first request waits for the speech worker to come up.
const WORKER_START: Duration = Duration::from_secs(8);
/// What KIVO says when the grammar doesn't understand (the brain arrives in M3).
const NOT_UNDERSTOOD: &str = "I can't do that yet.";

/// The turn in progress.
struct Running {
    id: String,
    utterance: u64,
    source: TurnSource,
    cancel: CancellationToken,
    started: Instant,
    /// T0–T10 spans, in milliseconds from the start of the turn (ARCH-28).
    spans: serde_json::Map<String, serde_json::Value>,
    transcript: String,
    /// The confirmation the user is being asked, and the call it belongs to.
    pending: Option<(ConfirmSpec, ToolCall)>,
    /// The id of the speech being played, so a late `SpeakDone` can be matched.
    speaking: Option<u64>,
    /// Becomes true once the speech worker has started this utterance.
    stt_started: tokio::sync::watch::Receiver<bool>,
}

/// What the engine is built from.
pub struct Parts {
    pub core: Arc<Core>,
    pub infer: Infer,
    pub speaker: Arc<Speaker>,
    pub registry: Arc<Registry>,
    pub recorder: Recorder,
    pub apps: Arc<dyn Apps>,
    pub windows: Arc<dyn Windows>,
    /// The installed apps, shared with the tools.
    pub app_catalog: Arc<RwLock<Vec<kivo_platform::AppEntry>>>,
    pub router: IntentRouter,
    /// Whether a fullscreen app or Focus is on (UX-11).
    pub system: Arc<dyn kivo_platform::SystemInfo>,
    /// Windows' own voice, in this process: speaks a failure when the speech worker is gone
    /// (ARCH-09). `None` keeps failures on screen only.
    pub fallback_voice: Option<Arc<dyn kivo_platform::SpeechSynth>>,
}

pub struct Engine {
    pub core: Arc<Core>,
    pub infer: Infer,
    pub speaker: Arc<Speaker>,
    pub registry: Arc<Registry>,
    pub recorder: Recorder,
    apps: Arc<dyn Apps>,
    windows: Arc<dyn Windows>,
    app_catalog: Arc<RwLock<Vec<kivo_platform::AppEntry>>>,
    system: Arc<dyn kivo_platform::SystemInfo>,
    fallback_voice: Option<Arc<dyn kivo_platform::SpeechSynth>>,
    app_index: RwLock<(Index, Instant)>,
    router: Mutex<IntentRouter>,
    turn: Mutex<Option<Running>>,
    listener: RwLock<Option<Arc<Listener>>>,
    next_turn: AtomicU64,
}

impl Engine {
    pub fn new(parts: Parts) -> Self {
        Self {
            core: parts.core,
            infer: parts.infer,
            speaker: parts.speaker,
            registry: parts.registry,
            recorder: parts.recorder,
            apps: parts.apps,
            windows: parts.windows,
            app_catalog: parts.app_catalog,
            system: parts.system,
            fallback_voice: parts.fallback_voice,
            app_index: RwLock::new((Index::default(), Instant::now() - APPS_TTL * 2)),
            router: Mutex::new(parts.router),
            turn: Mutex::new(None),
            listener: RwLock::new(None),
            next_turn: AtomicU64::new(1),
        }
    }

    pub fn set_listener(&self, listener: Arc<Listener>) {
        *write(&self.listener) = Some(listener);
    }

    fn listener(&self) -> Option<Arc<Listener>> {
        read(&self.listener).clone()
    }

    fn turn_id(&self) -> String {
        format!("t{}", self.next_turn.fetch_add(1, Ordering::Relaxed))
    }

    /// Reads the installed apps again (also refreshes the tools' catalogue).
    pub fn refresh_apps(&self) {
        let Ok(apps) = self.apps.installed() else {
            return;
        };
        let entries = apps
            .iter()
            .map(|a| IndexEntry {
                id: a.id.clone(),
                name: a.name.clone(),
                aliases: a.aliases.clone(),
            })
            .collect();
        *self
            .app_catalog
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = apps;
        *write(&self.app_index) = (Index::new(entries), Instant::now());
    }

    fn apps_index(&self) -> Index {
        if read(&self.app_index).1.elapsed() > APPS_TTL {
            self.refresh_apps();
        }
        read(&self.app_index).0.clone()
    }

    /// Open windows, as the grammar sees them ("switch to Outlook").
    fn windows_index(&self) -> Index {
        let windows = self.windows.list().unwrap_or_default();
        let catalog = self
            .app_catalog
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let entries = windows
            .into_iter()
            .map(|w| {
                let app = catalog.iter().find(|a| {
                    a.id.eq_ignore_ascii_case(&w.app_id)
                        || a.exe
                            .as_ref()
                            .is_some_and(|e| e.eq_ignore_ascii_case(&w.app_id))
                });
                IndexEntry {
                    id: w.id.0.to_string(),
                    name: w.title,
                    aliases: app.map(|a| a.name.clone()).into_iter().collect(),
                }
            })
            .collect();
        Index::new(entries)
    }

    /// Starts listening (push-to-talk, the Talk button, or a follow-up). The microphone opens at
    /// once; the speech models load in parallel (prewarm on turn start, VOICE-34) and the audio
    /// heard meanwhile is kept, so nothing the user says is lost to a cold start.
    pub async fn talk(self: &Arc<Self>, source: TurnSource) -> Result<(), String> {
        if !matches!(self.core.speech_status(), SpeechStatus::Ready) {
            // Speech isn't installed yet: say so instead of listening into nothing.
            let message = match self.core.speech_status() {
                SpeechStatus::Downloading { percent } => {
                    text::tf("turn.downloading", &[("percent", &percent)])
                }
                _ => text::t("turn.sttMissing"),
            };
            self.core.flash_error(&message, COLLAPSE_AFTER);
            self.speaker.cue(Cue::Error);
            return Err(message);
        }
        self.infer.warm();
        let id = self.turn_id();
        self.core.begin_turn(&id, source, "").map_err(|e| e.0)?;
        self.quiet_if_busy();
        let config = self.core.config();
        let utterance = self.infer.next_utterance();
        let (started, started_rx) = tokio::sync::watch::channel(false);
        let running = Running {
            id: id.clone(),
            utterance,
            source,
            cancel: CancellationToken::new(),
            started: Instant::now(),
            spans: serde_json::Map::new(),
            transcript: String::new(),
            pending: None,
            speaking: None,
            stt_started: started_rx,
        };
        *lock(&self.turn) = Some(running);
        self.recorder.turn_started(&id, source);
        self.core
            .bus
            .publish(Event::new(EventKind::Turn(TurnEvent::Started { source })));
        self.core
            .bus
            .publish(Event::new(EventKind::Voice(VoiceEvent::SpeechStarted)));
        self.speaker.cue(Cue::ListenStart);
        if let Some(listener) = self.listener() {
            listener.listen(
                utterance,
                config.voice.auto_end_on_silence || source != TurnSource::PushToTalk,
            );
        }
        self.mark("t1Listening");

        // Start recognition as soon as the worker is up.
        let engine = Arc::clone(self);
        let language = config.general.language.clone();
        tokio::spawn(async move {
            let vocabulary = kivo_voice::language::pack(&language)
                .map(|p| p.vocabulary)
                .unwrap_or_default();
            let result = if engine.infer.wait_ready(WORKER_START).await {
                engine
                    .infer
                    .start_stt(utterance, &language, vocabulary)
                    .await
            } else {
                Err(crate::infer::InferError::NotReady)
            };
            match result {
                Ok(()) => {
                    if let Some(listener) = engine.listener() {
                        listener.stt_ready(utterance);
                    }
                    let _ = started.send(true);
                }
                Err(e) => {
                    let current = lock(&engine.turn).as_ref().map(|t| t.utterance);
                    if current == Some(utterance) {
                        engine.cancel_listening();
                        engine.fail_turn(&text::tf("turn.listenFailed", &[("error", &e)]));
                    }
                }
            }
        });
        Ok(())
    }

    /// Over a fullscreen app or during Focus the Island hides or shrinks to a dot (the setting);
    /// KIVO still answers (UX-11).
    fn quiet_if_busy(&self) {
        let attention = self.system.attention().unwrap_or_default();
        if !(attention.fullscreen_app || attention.focus_mode) {
            return;
        }
        let quiet = match self.core.config().overlay.in_fullscreen {
            kivo_core::config::FullscreenBehavior::Hide => QuietIsland::Hidden,
            kivo_core::config::FullscreenBehavior::TinyPill => QuietIsland::Tiny,
        };
        self.core.update_turn(|view| view.quiet = Some(quiet));
    }

    /// Stops the microphone without a result (the speech engine failed to start).
    fn cancel_listening(&self) {
        if let Some(listener) = self.listener() {
            listener.cancel();
        }
    }

    /// The settings changed: apply the ones the engine and the speaker hold (UX §5).
    pub fn settings_changed(&self, config: &kivo_core::KivoConfig) {
        self.speaker
            .set_sounds(config.sounds.enabled, config.sounds.volume);
        self.speaker.set_output_device(
            config
                .voice
                .output_device
                .clone()
                .map(kivo_platform::DeviceId),
        );
    }

    /// True while the microphone is open for this turn.
    pub fn is_listening(&self) -> bool {
        self.listener().is_some_and(|l| l.is_listening())
    }

    /// Push-to-talk released: stop listening and use what was heard.
    pub fn release(&self) {
        if let Some(listener) = self.listener() {
            listener.stop();
        }
    }

    /// A typed request behaves exactly like a spoken one (UX §8).
    pub async fn say(self: &Arc<Self>, text: &str) -> Result<(), String> {
        let text = text.trim();
        if text.is_empty() {
            return Err(text::t("turn.nothingToSend"));
        }
        self.infer.warm();
        let id = self.turn_id();
        self.core
            .begin_turn(&id, TurnSource::Typed, text)
            .map_err(|e| e.0)?;
        self.quiet_if_busy();
        *lock(&self.turn) = Some(Running {
            id: id.clone(),
            utterance: 0,
            source: TurnSource::Typed,
            cancel: CancellationToken::new(),
            started: Instant::now(),
            spans: serde_json::Map::new(),
            transcript: text.to_owned(),
            pending: None,
            speaking: None,
            stt_started: tokio::sync::watch::channel(true).1,
        });
        self.recorder.turn_started(&id, TurnSource::Typed);
        self.core.advance(SessionInput::EndOfSpeech);
        let engine = Arc::clone(self);
        let text = text.to_owned();
        tokio::spawn(async move { engine.handle_transcript(&text).await });
        Ok(())
    }

    /// The pipeline says the utterance ended.
    pub async fn end_of_speech(self: &Arc<Self>, utterance: u64) {
        let matches = lock(&self.turn)
            .as_ref()
            .is_some_and(|t| t.utterance == utterance);
        if !matches {
            return;
        }
        self.mark("t4EndOfSpeech");
        self.core.end_of_speech();
        self.speaker.cue(Cue::ListenStop);
        self.core
            .bus
            .publish(Event::new(EventKind::Voice(VoiceEvent::SpeechEnded)));
        let started = lock(&self.turn).as_ref().map(|t| t.stt_started.clone());
        if let Some(mut started) = started {
            let ready = tokio::time::timeout(WORKER_START, started.wait_for(|s| *s)).await;
            if !matches!(ready, Ok(Ok(_))) {
                self.fail_turn(&text::t("turn.sttSlow"));
                return;
            }
        }
        let text = match self.infer.finish_stt(utterance).await {
            Ok(final_text) => final_text.text,
            Err(e) => {
                self.fail_turn(&text::tf("turn.sttLost", &[("error", &e)]));
                return;
            }
        };
        self.mark("t5FinalTranscript");
        if text.trim().is_empty() {
            let message = text::t("turn.notCaught");
            self.show_error(&message);
            self.speak_and_finish(&message).await;
            return;
        }
        self.handle_transcript(&text).await;
    }

    /// Nothing was said (or the microphone is blocked).
    pub fn nothing_heard(&self, utterance: u64) {
        let matches = lock(&self.turn)
            .as_ref()
            .is_some_and(|t| t.utterance == utterance);
        if matches {
            self.cancel(CancelReason::Timeout);
        }
    }

    /// A partial transcript from the worker.
    pub fn partial(&self, utterance: u64, text: &str, stable: bool) {
        let mut turn = lock(&self.turn);
        let Some(running) = turn.as_mut().filter(|t| t.utterance == utterance) else {
            return;
        };
        if !stable {
            running.transcript = text.to_owned();
        }
        if running.spans.contains_key("t3FirstPartial") {
            drop(turn);
        } else {
            running
                .spans
                .insert("t3FirstPartial".into(), json!(elapsed_ms(running.started)));
            drop(turn);
        }
        let text = text.to_owned();
        self.core.update_turn(|view| {
            view.transcript = text.clone();
            view.transcript_final = false;
        });
        self.core.bus.publish(Event::new(EventKind::Voice(
            VoiceEvent::PartialTranscript {
                text,
                stable_len: 0,
            },
        )));
    }

    /// Routes a final transcript and does what it asks (BRAINS §2).
    async fn handle_transcript(self: &Arc<Self>, text: &str) {
        let text = text.trim().to_owned();
        self.core.update_turn(|view| {
            view.transcript.clone_from(&text);
            view.transcript_final = true;
        });
        if let Some(running) = lock(&self.turn).as_mut() {
            running.transcript.clone_from(&text);
        }
        self.recorder.transcript(&self.turn_key(), &text);
        self.core
            .bus
            .publish(Event::new(EventKind::Voice(VoiceEvent::FinalTranscript {
                text: text.clone(),
                confidence: None,
            })));

        let apps = self.apps_index();
        let windows = self.windows_index();
        let decision = {
            let mut router = lock(&self.router);
            let decision = router.route(
                &text,
                &GrammarContext {
                    apps: &apps,
                    windows: &windows,
                },
            );
            let metrics = router.metrics();
            tracing::debug!(
                fast_path_ratio = metrics.fast_path_ratio,
                grammar_p95_us = metrics.grammar_p95_micros,
                requests = metrics.requests,
                "intent routing (BRAIN-05)"
            );
            decision
        };
        self.mark("t6Intent");
        match decision.route {
            Route::FastPath(matched) => {
                self.core
                    .bus
                    .publish(Event::new(EventKind::Turn(TurnEvent::IntentDetected {
                        path: IntentPath::FastPath,
                        intent: matched.tool.clone(),
                    })));
                let call = ToolCall {
                    id: format!("{}-c1", self.turn_key()),
                    tool: matched.tool.clone(),
                    args: serde_json::Value::Object(matched.args.clone()),
                    initiated_by: Initiator::UserDirect,
                    targets: targets(&matched.tool, &serde_json::Value::Object(matched.args)),
                };
                self.run_call(call).await;
            }
            Route::Unhandled => {
                self.core
                    .bus
                    .publish(Event::new(EventKind::Turn(TurnEvent::IntentDetected {
                        path: IntentPath::Brain,
                        intent: "unknown".into(),
                    })));
                self.recorder
                    .answer(&self.turn_key(), NOT_UNDERSTOOD, "unhandled");
                self.speak_and_finish(NOT_UNDERSTOOD).await;
            }
        }
    }

    /// Decides on a call and, when allowed, runs it.
    async fn run_call(self: &Arc<Self>, call: ToolCall) {
        let capabilities = self.core.config().capabilities;
        let Some(tool) = self.registry.get(&call.tool, &capabilities) else {
            // The capability is off (or the tool doesn't exist here): say so plainly (CAP-02).
            let message = self.registry.known(&call.tool).map_or_else(
                || text::t("turn.notUnderstood"),
                |spec| {
                    text::tf(
                        "policy.capabilityOff",
                        &[("capability", &spec.capability.label())],
                    )
                },
            );
            self.recorder.tool_denied(&self.turn_key(), &call, &message);
            let capability = self.registry.known(&call.tool).map(|spec| spec.capability);
            self.core
                .update_turn(|view| view.capability_off = capability);
            self.speak_and_finish(&message).await;
            return;
        };
        let spec = tool.spec().clone();
        let limits = HardLimits {
            stopped: lock(&self.turn)
                .as_ref()
                .is_some_and(|t| t.cancel.is_cancelled()),
            blocked_apps: self.core.config().permissions.blocked_apps,
        };
        let grants: Vec<Grant> = self.recorder.grants();
        let decision = kivo_security::authorize(
            &spec,
            &call,
            &SecurityContext {
                mode: self.core.state().borrow().mode,
                session: SessionKind::Owner,
                taint: Taint::Clean,
                capabilities: &capabilities,
                limits: &limits,
                grants: &grants,
            },
        );
        self.mark("t7Permission");
        self.recorder
            .tool_decision(&self.turn_key(), &call, &spec, &decision);
        match decision {
            Decision::Allow(permit) => {
                self.execute(tool, call, permit, &spec.title).await;
            }
            Decision::Confirm(confirm) => {
                let title = confirm.action.clone();
                if let Some(running) = lock(&self.turn).as_mut() {
                    running.pending = Some((confirm.clone(), call.clone()));
                }
                self.core.advance(SessionInput::NeedConfirmation);
                self.core.update_turn(|view| {
                    view.confirm = Some(confirm.clone());
                    view.target_app = confirm.target.clone();
                });
                self.speaker.cue(Cue::Thinking);
                let question = format!("{title}?");
                self.speak(&question).await;
            }
            Decision::Deny(denial) => {
                if let kivo_security::DenyCode::CapabilityOff(capability) = denial.code {
                    self.core
                        .update_turn(|view| view.capability_off = Some(capability));
                }
                self.recorder
                    .answer(&self.turn_key(), &denial.message, "denied");
                self.speak_and_finish(&denial.message).await;
            }
        }
    }

    /// The user answered a confirmation card (SEC-10).
    pub async fn answer_confirmation(
        self: &Arc<Self>,
        call_id: &str,
        allow: bool,
        always: bool,
    ) -> Result<(), String> {
        let pending = lock(&self.turn).as_mut().and_then(|t| {
            t.pending
                .as_ref()
                .filter(|(spec, _)| spec.call_id == call_id)
                .cloned()
                .inspect(|_| t.pending = None)
        });
        let Some((spec, call)) = pending else {
            return Err(text::t("turn.nothingWaiting"));
        };
        self.core.update_turn(|view| view.confirm = None);
        if !allow {
            self.core.advance(SessionInput::Denied);
            self.recorder.confirmation(&self.turn_key(), &call, false);
            self.speaker.cue(Cue::Hangup);
            self.core
                .update_turn(|view| view.answer = Some(text::t("reply.cancelled")));
            self.finish_turn("cancelled", Some(&text::t("reply.cancelled")));
            return Ok(());
        }
        let permit = kivo_security::confirmed(
            &spec,
            &call,
            Answer::Allow {
                by: ConfirmedBy::Click,
            },
        )
        .map_err(|e| e.message)?;
        self.recorder.confirmation(&self.turn_key(), &call, true);
        if always && spec.allow_always {
            self.recorder.add_grant(&call);
        }
        self.core.advance(SessionInput::Confirmed);
        let capabilities = self.core.config().capabilities;
        let Some(tool) = self.registry.get(&call.tool, &capabilities) else {
            return Err(text::t("turn.gone"));
        };
        let title = tool.spec().title.clone();
        self.execute(tool, call, permit, &title).await;
        Ok(())
    }

    /// Runs an approved call and says what happened.
    async fn execute(
        self: &Arc<Self>,
        tool: Arc<dyn kivo_tools::Tool>,
        call: ToolCall,
        permit: kivo_security::Permit,
        title: &str,
    ) {
        let cancel = lock(&self.turn)
            .as_ref()
            .map_or_else(CancellationToken::new, |t| t.cancel.child_token());
        if self.core.state().borrow().session != SessionState::Acting {
            self.core.advance(SessionInput::StartActing);
        }
        let step_title = kivo_security::render_title(title, &call.args);
        self.core.set_step(StepView {
            id: call.id.clone(),
            title: step_title.clone(),
            status: StepStatus::Running,
            detail: None,
        });
        self.core.update_turn(|view| {
            if view.target_app.is_none() {
                view.target_app = call.targets.iter().find_map(|t| match t {
                    kivo_core::tool::Target::App { name, .. } => Some(name.clone()),
                    _ => None,
                });
            }
        });
        self.core.bus.publish(Event::new(EventKind::Tool(
            kivo_core::event::ToolEvent::Started {
                call_id: call.id.clone(),
            },
        )));
        let (result, output) = kivo_tools::execute(tool, &call, permit, &cancel).await;
        self.mark("t8ToolDone");
        self.recorder.tool_result(&self.turn_key(), &call, &result);
        match (&result.status, output) {
            (Ok(_), Some(output)) => {
                self.core.set_step(StepView {
                    id: call.id.clone(),
                    title: step_title,
                    status: StepStatus::Done,
                    detail: Some(output.say.clone()),
                });
                // An app was launched or closed: the window list changed.
                if call.tool.starts_with("apps.") {
                    let engine = Arc::clone(self);
                    tokio::task::spawn_blocking(move || engine.refresh_apps());
                }
                self.speak_and_finish(&output.say).await;
            }
            (Err(error), _) => {
                self.core.set_step(StepView {
                    id: call.id.clone(),
                    title: step_title,
                    status: StepStatus::Failed,
                    detail: Some(error.message.clone()),
                });
                self.speaker.cue(Cue::Error);
                let message = retry_hint(error);
                self.core
                    .update_turn(|view| view.error = Some(message.clone()));
                self.speak_and_finish(&message).await;
            }
            (Ok(_), None) => self.finish_turn("done", None),
        }
    }

    /// Says `text` and ends the turn once it has been spoken.
    async fn speak_and_finish(self: &Arc<Self>, text: &str) {
        self.core
            .update_turn(|view| view.answer = Some(text.to_owned()));
        self.recorder.answer(&self.turn_key(), text, "done");
        self.speak(text).await;
    }

    /// Streams `text` through the voice; `SpeakDone` finishes the turn.
    async fn speak(self: &Arc<Self>, text: &str) {
        let speak_replies = {
            let config = self.core.config();
            let typed = lock(&self.turn)
                .as_ref()
                .is_some_and(|t| t.source == TurnSource::Typed);
            let capability_on = config
                .capabilities
                .enabled(kivo_core::Capability::SpeakResponses);
            capability_on && (!typed || config.voice.speak_typed_replies)
        };
        let state = self.core.state().borrow().session;
        if state == SessionState::Thinking || state == SessionState::Acting {
            self.core.advance(SessionInput::StartSpeaking);
        }
        if !speak_replies {
            // Nothing is spoken: a soft cue confirms the action instead (VOICE §6).
            self.speaker.cue(Cue::Done);
            self.finish_speaking();
            return;
        }
        // A quick action can finish before the worker (warmed when the turn began) is up: wait
        // for it, unless the turn is cancelled meanwhile.
        if !self.infer.is_ready() {
            let cancel = lock(&self.turn).as_ref().map(|t| t.cancel.clone());
            let Some(cancel) = cancel else { return };
            let ready = tokio::select! {
                ready = self.infer.wait_ready(WORKER_START) => ready,
                () = cancel.cancelled() => return,
            };
            if !ready {
                // No worker: Windows' voice in this process says it instead.
                self.say_in_process(text);
                self.finish_speaking();
                return;
            }
        }
        let id = self.infer.next_utterance();
        if let Some(running) = lock(&self.turn).as_mut() {
            running.speaking = Some(id);
        }
        let voice = self.core.config().voice.tts_voice;
        let voice = (!voice.is_empty()).then_some(voice);
        self.core
            .bus
            .publish(Event::new(EventKind::Voice(VoiceEvent::TtsStarted)));
        if let Err(e) = self.infer.speak(id, text, voice.as_deref()).await {
            tracing::warn!(%e, "couldn't speak");
            self.finish_speaking();
        }
    }

    /// Audio for a spoken reply arrived.
    pub fn speech_audio(&self, id: u64, rate: u32, pcm: &[f32]) {
        let ours = lock(&self.turn)
            .as_ref()
            .is_some_and(|t| t.speaking == Some(id));
        if !ours {
            return;
        }
        let first = lock(&self.turn).as_mut().is_some_and(|t| {
            !t.spans.contains_key("t9FirstAudio") && {
                t.spans
                    .insert("t9FirstAudio".into(), json!(elapsed_ms(t.started)));
                true
            }
        });
        if first {
            tracing::debug!("first spoken audio");
        }
        self.speaker.speak(pcm, rate);
    }

    /// A spoken reply finished (or failed).
    pub fn speech_done(&self, id: u64, error: Option<&str>, cancelled: bool) {
        let ours = lock(&self.turn)
            .as_ref()
            .is_some_and(|t| t.speaking == Some(id));
        if !ours {
            return;
        }
        if let Some(message) = error {
            tracing::warn!(message, "speaking failed");
        }
        if !cancelled {
            self.finish_speaking();
        }
    }

    /// Waits for the speaker to drain, then ends the turn (or starts a follow-up).
    fn finish_speaking(&self) {
        let Some(id) = lock(&self.turn).as_ref().map(|t| t.id.clone()) else {
            return;
        };
        let answer = self.core.turn_view().and_then(|v| v.answer);
        self.finish_turn("done", answer.as_deref());
        tracing::debug!(turn = id, "turn complete");
    }

    /// Ends the turn: the session returns to Idle (or a follow-up window) and the Island clears.
    fn finish_turn(&self, outcome: &str, reply: Option<&str>) {
        let Some(running) = lock(&self.turn).take() else {
            return;
        };
        self.infer.touch();
        let state = self.core.state().borrow().session;
        let follow_up_seconds = self.core.config().voice.follow_up_seconds;
        let spoken = state == SessionState::Speaking;
        let next = if spoken {
            SessionInput::SpeechFinished
        } else {
            SessionInput::Finished
        };
        if state.is_active() {
            self.core.advance(next);
        }
        let total = elapsed_ms(running.started);
        let mut spans = running.spans.clone();
        spans.insert("t10Complete".into(), json!(total));
        self.recorder
            .turn_finished(&running.id, &running.transcript, outcome, reply, &spans);
        self.core
            .bus
            .publish(Event::new(EventKind::Turn(TurnEvent::Completed)));
        self.core
            .bus
            .publish(Event::new(EventKind::Voice(VoiceEvent::TtsStopped {
                reason: kivo_core::event::TtsStopReason::Finished,
            })));
        // The Island keeps the answer for a moment, then collapses (UX-10). A follow-up window
        // (M2) keeps it listening; for now the session just returns to Idle.
        let core = Arc::clone(&self.core);
        let turn_id = running.id.clone();
        tokio::spawn(async move {
            tokio::time::sleep(COLLAPSE_AFTER).await;
            let same_turn = core.turn_view().is_some_and(|v| v.id == turn_id);
            if same_turn {
                core.clear_turn();
                if core.state().borrow().session == SessionState::FollowUp {
                    core.advance(SessionInput::FollowUpTimeout);
                }
            }
        });
        if follow_up_seconds == 0 && self.core.state().borrow().session == SessionState::FollowUp {
            self.core.advance(SessionInput::FollowUpTimeout);
        }
    }

    /// Shows an error in the Island (and moves the session to Error).
    fn show_error(&self, message: &str) {
        self.core
            .update_turn(|view| view.error = Some(message.to_owned()));
        self.speaker.cue(Cue::Error);
        tracing::warn!(message, "turn failed");
    }

    /// The engine broke: tell the user and end the turn (ARCH-09).
    fn fail_turn(&self, message: &str) {
        self.show_error(message);
        self.say_failure(message);
        self.recorder.answer(&self.turn_key(), message, "failed");
        if self.core.state().borrow().session.is_active() {
            self.core.advance(SessionInput::Fail);
            self.core.advance(SessionInput::ErrorShown);
        }
        self.core
            .bus
            .publish(Event::new(EventKind::Turn(TurnEvent::Failed {
                message: message.to_owned(),
            })));
        if let Some(running) = lock(&self.turn).take() {
            let spans = running.spans.clone();
            self.recorder.turn_finished(
                &running.id,
                &running.transcript,
                "failed",
                Some(message),
                &spans,
            );
        }
        let core = Arc::clone(&self.core);
        tokio::spawn(async move {
            tokio::time::sleep(COLLAPSE_AFTER).await;
            core.clear_turn();
        });
    }

    /// Stops the turn (Esc, Stop, the emergency stop, barge-in): recognition, the tool and the
    /// voice all stop, and the Island clears (ARCH-26).
    pub fn cancel(&self, reason: CancelReason) {
        let Some(running) = lock(&self.turn).take() else {
            // Nothing running: stop whatever the session is doing and clear what the Island shows
            // ("Not now" on a finished request's card). The reply may still be playing (it is
            // synthesized faster than it is heard), so the voice stops too.
            self.speaker.stop();
            let _ = self.core.cancel_turn();
            self.core.clear_turn();
            return;
        };
        running.cancel.cancel();
        let _ = self.infer.cancel_stt(running.utterance);
        if let Some(id) = running.speaking {
            let _ = self.infer.cancel_speech(id);
        }
        self.speaker.stop();
        if let Some(listener) = self.listener() {
            listener.cancel();
        }
        let _ = self.core.cancel_turn();
        self.core
            .bus
            .publish(Event::new(EventKind::Turn(TurnEvent::Cancelled { reason })));
        let spans = running.spans.clone();
        self.recorder
            .turn_finished(&running.id, &running.transcript, "cancelled", None, &spans);
    }

    /// How requests have been routed: the fast-path share and per-stage p95 (BRAIN-05).
    pub fn router_metrics(&self) -> kivo_intent::RouterMetrics {
        lock(&self.router).metrics()
    }

    /// Stop everything (SEC-25): the turn, the voice and (from M5) tasks.
    pub fn stop_everything(&self) {
        self.cancel(CancelReason::EmergencyStop);
        self.speaker.stop();
        self.core.stop_everything();
        self.recorder.emergency_stop();
    }

    /// Speaks a failure with Windows' own voice in this process, since the speech worker (and its
    /// voices) may be what failed. Follows "Speak responses" (CAP).
    fn say_failure(&self, message: &str) {
        if self
            .core
            .config()
            .capabilities
            .enabled(kivo_core::Capability::SpeakResponses)
        {
            self.say_in_process(message);
        }
    }

    /// Windows' own voice, in this process, for when the speech worker can't speak.
    fn say_in_process(&self, message: &str) {
        let Some(voice) = self.fallback_voice.clone() else {
            return;
        };
        let speaker = Arc::clone(&self.speaker);
        let text = message.to_owned();
        tokio::task::spawn_blocking(move || match voice.synthesize(&text, None) {
            Ok(audio) => speaker.speak(&audio.samples, audio.rate),
            Err(e) => tracing::warn!(%e, "couldn't speak the failure"),
        });
    }

    /// The speech worker died mid-turn.
    pub fn engine_lost(&self) {
        if lock(&self.turn).is_some() {
            self.fail_turn(&text::t("turn.ttsLost"));
        }
    }

    fn turn_key(&self) -> String {
        lock(&self.turn)
            .as_ref()
            .map(|t| t.id.clone())
            .unwrap_or_default()
    }

    /// Records a latency span (ARCH-28: T0 … T10).
    fn mark(&self, span: &str) {
        if let Some(running) = lock(&self.turn).as_mut() {
            let ms = elapsed_ms(running.started);
            running
                .spans
                .entry(span.to_owned())
                .or_insert_with(|| json!(ms));
        }
    }
}

/// Voice-pipeline signals and worker events, applied to the engine.
pub async fn handle_signal(engine: &Arc<Engine>, signal: VoiceSignal) {
    match signal {
        VoiceSignal::SpeechStarted { .. } => engine.mark("t2SpeechStarted"),
        VoiceSignal::EndOfSpeech { utterance } => engine.end_of_speech(utterance).await,
        VoiceSignal::NoSpeech { utterance } => engine.nothing_heard(utterance),
        VoiceSignal::MicrophoneUnavailable => {
            engine.fail_turn(&text::t("turn.micBlocked"));
        }
    }
}

pub async fn handle_infer_event(engine: &Arc<Engine>, event: InferEvent) {
    match event {
        InferEvent::Partial { id, text, stable } => engine.partial(id, &text, stable),
        InferEvent::Speech { id, rate, pcm } => engine.speech_audio(id, rate, &pcm),
        InferEvent::SpeakDone {
            id,
            error,
            cancelled,
        } => {
            engine.speech_done(id, error.as_deref(), cancelled);
        }
        InferEvent::Residency {
            slot,
            engine: name,
            state,
        } => {
            tracing::debug!(?slot, engine = name, ?state, "model residency");
        }
        InferEvent::Lost => engine.engine_lost(),
    }
}

/// Adds what the user can do about a failure (plan §146: cause plus an action).
fn retry_hint(error: &ToolError) -> String {
    match error.code {
        ToolErrorCode::NotFound => text::tf("error.hint.notFound", &[("message", &error.message)]),
        ToolErrorCode::AccessDenied => {
            text::tf("error.hint.accessDenied", &[("message", &error.message)])
        }
        ToolErrorCode::Timeout => text::tf("error.hint.timeout", &[("message", &error.message)]),
        _ => error.message.clone(),
    }
}

fn elapsed_ms(from: Instant) -> u64 {
    u64::try_from(from.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn read<T>(l: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    l.read().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn write<T>(l: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    l.write().unwrap_or_else(std::sync::PoisonError::into_inner)
}
