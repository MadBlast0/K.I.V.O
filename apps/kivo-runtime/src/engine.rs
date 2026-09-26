//! One turn, end to end (ARCHITECTURE §4.2, BRAINS §1, plan §138): push-to-talk or typed text →
//! speech recognition → the grammar fast path → the permission engine → the tool → a spoken reply,
//! with every step shown in the Island, recorded in Activity and audited.
//!
//! The turn owns a cancellation token; Stop, Esc, the emergency stop and a new turn all cancel it,
//! and cancellation reaches recognition, the tool and the speaker within 100 ms (ARCH-26).

mod agent;
mod brain;
mod computer;
mod control;
mod realtime;
mod shared;

pub use agent::{EnginePermissions, agent_spec};

/// A provider's failure in plain words (PLAN-05), for the task runner.
pub fn failure_text(name: &str, error: &kivo_brain::NormalizedError) -> String {
    brain::failure_message(name, error)
}

use crate::activity::Recorder;
use crate::brains::Brains;
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
use kivo_ipc::infer::InferSlot;
use kivo_ipc::protocol::{QuietIsland, SpeechStatus, StepView};
use kivo_platform::{Apps, Windows};
use kivo_security::{Answer, Decision};
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
/// The optional "thinking" cue plays once a request has taken this long (VOICE §6).
const THINKING_CUE_AFTER: Duration = Duration::from_secs(1);
/// How long a first request waits for the speech worker to come up.
const WORKER_START: Duration = Duration::from_secs(8);
/// The tool a Draft card sends through (CONV-15).
const DRAFT_TOOL: &str = "agents.send_prompt";
/// How long the Island offers Undo (UX §8.1).
const UNDO_OFFER: Duration = Duration::from_secs(8);
/// How long "Kivo, undo that" and the toast can still take the last change back.
const UNDO_KEPT: Duration = Duration::from_secs(10 * 60);
/// The media activity's id, and how long the track shows after a media command (UX-15).
const MEDIA_ACTIVITY: &str = "media";
const MEDIA_SHOWN_FOR: Duration = Duration::from_secs(10);

/// The last change that can be taken back (UX-43).
struct UndoEntry {
    tool: String,
    data: serde_json::Value,
    title: String,
    at: Instant,
}

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
    /// How the turn ends once its reply is spoken: "done", or "unhandled" when KIVO couldn't
    /// route the request.
    outcome: &'static str,
    /// The wake phrase that started the turn, removed from the transcript (VOICE-05).
    wake_phrase: Option<String>,
    /// What KIVO is saying, so its own "stop" isn't taken for the user's (VOICE-19).
    reply: String,
    /// The tool calls that succeeded, in order: "save what you just did as a routine" (ROUT-13).
    calls: Vec<(String, serde_json::Value)>,
    /// Someone other than the enrolled owner (VOICE-22 "Prefer owner"): a guest session with no
    /// memory or preferences, and medium-risk actions confirmed (SECURITY §1).
    guest: bool,
    /// Listening for a spoken answer to the decision on the card (CONV-26).
    answering: bool,
    /// The spoken answer's audio, for the owner's-voice check (CONV-28).
    answer_audio: Option<Vec<f32>>,
    /// Times KIVO has asked again after an answer it didn't understand.
    reasked: u8,
    /// Phrases of a streamed answer waiting behind the one being spoken (BRAIN-28).
    phrases: std::collections::VecDeque<String>,
    /// A brain's answer is still arriving.
    streaming: bool,
    /// Something of this turn's answer was spoken.
    spoken_any: bool,
    /// A brain turn waiting for the user's decision: (allow, always, how).
    waiter: Option<tokio::sync::oneshot::Sender<(bool, bool, ConfirmedBy)>>,
    /// The profile chosen for this request (Chat's brain switcher).
    brain_choice: Option<String>,
    /// The thread chosen for this request (Chat), instead of the voice session's.
    thread: Option<String>,
    /// Files attached in Chat: (name, text). Their contents are untrusted data (SECURITY §4).
    attachments: Vec<(String, String)>,
    /// Where untrusted content entered this turn (a page, a window, the clipboard, a file):
    /// non-empty means the turn is tainted (SEC-13).
    tainted: Vec<String>,
    /// The tools offered to the brain in this round; only these can be called (SEC-15).
    offered: Vec<String>,
    /// The brain answering this round runs off the device (its name, for the egress check's
    /// message): private tool results aren't shown to it (SEC-20/21).
    cloud_brain: Option<String>,
    /// The Draft's text after the user changed it (CONV-15), for the call that sends it.
    draft_text: Option<String>,
    /// A realtime conversation is open in this turn (BRAIN-33).
    live: bool,
    /// The final transcript is in: a partial that arrives later is stale and ignored.
    heard_final: bool,
}

impl Running {
    fn new(id: String, utterance: u64, source: TurnSource) -> Self {
        Self {
            id,
            utterance,
            source,
            cancel: CancellationToken::new(),
            started: Instant::now(),
            spans: serde_json::Map::new(),
            transcript: String::new(),
            pending: None,
            speaking: None,
            stt_started: tokio::sync::watch::channel(true).1,
            outcome: "done",
            wake_phrase: None,
            reply: String::new(),
            calls: Vec::new(),
            guest: false,
            answering: false,
            answer_audio: None,
            reasked: 0,
            phrases: std::collections::VecDeque::new(),
            streaming: false,
            spoken_any: false,
            waiter: None,
            brain_choice: None,
            thread: None,
            attachments: Vec::new(),
            heard_final: false,
            tainted: Vec::new(),
            offered: Vec::new(),
            cloud_brain: None,
            draft_text: None,
            live: false,
        }
    }
}

/// What the last request said and the tool calls it made (ROUT-13).
type LastCalls = (String, Vec<(String, serde_json::Value)>);

/// Applies the speech engines for new settings.
pub type EnginesHook = Arc<dyn Fn(&kivo_core::KivoConfig) + Send + Sync>;

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
    /// The connected brains (BRAINS §3–5).
    pub brains: Arc<Brains>,
    /// CLI agents over ACP (BRAINS §4, §7).
    pub agents: Arc<crate::agents::Agents>,
    /// Windows Hello, for High-risk confirmations (SEC-11).
    pub verifier: Arc<dyn kivo_platform::UserVerifier>,
    /// The command runner, so the emergency stop can kill every command's process tree.
    pub commands: Option<Arc<dyn kivo_platform::CommandRunner>>,
    /// Raised by the emergency stop to halt synthetic input between keys.
    pub input_abort: Option<Arc<std::sync::atomic::AtomicBool>>,
    /// Whether this turn's brain may be shown a screenshot (CAP-08); the tools read it.
    pub vision: Arc<std::sync::atomic::AtomicBool>,
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
    /// Turn ids count up from the moment the runtime started, so they never repeat one stored
    /// by an earlier run (the `turns` table keys on them).
    next_turn: AtomicU64,
    /// Echo cancellation is removing KIVO's own output from the microphone (VOICE-30).
    echo_cancelled: std::sync::atomic::AtomicBool,
    /// Recognizing the owner's voice (VOICE §5).
    voice_id: RwLock<Option<Arc<crate::voiceid::VoiceId>>>,
    /// Speech ids of previews playing outside a turn ("hear it", voice previews).
    previews: Mutex<std::collections::HashSet<u64>>,
    pub brains: Arc<Brains>,
    pub agents: Arc<crate::agents::Agents>,
    /// The voice session and its thread (CONVERSATION §1).
    session: Mutex<brain::VoiceSession>,
    verifier: Arc<dyn kivo_platform::UserVerifier>,
    /// The last undoable change (UX-43).
    last_undo: Mutex<Option<UndoEntry>>,
    /// App icons already looked up, as data URLs (UX-46).
    icons: Mutex<std::collections::HashMap<String, Option<String>>>,
    commands: Option<Arc<dyn kivo_platform::CommandRunner>>,
    input_abort: Option<Arc<std::sync::atomic::AtomicBool>>,
    vision: Arc<std::sync::atomic::AtomicBool>,
    /// Background tasks (M5), set once they exist.
    tasks: RwLock<Option<Arc<crate::tasks::Tasks>>>,
    /// What each app supports, for "What can I say?" (UX-44).
    app_registry: RwLock<Option<Arc<kivo_tools::appreg::AppRegistry>>>,
    /// Routines and custom commands (ROUTINES), set once they exist.
    routines: RwLock<Option<Arc<crate::routines::Routines>>>,
    /// Instructions and workspaces (CONVERSATION §4).
    workspaces: RwLock<Option<Arc<crate::workspaces::Workspaces>>>,
    /// The last request that did something: what was said and the calls it made (ROUT-13).
    last_calls: Mutex<Option<LastCalls>>,
    /// The screen, for computer use's screenshots (CAP-10).
    screen: RwLock<Option<Arc<dyn kivo_platform::Screen>>>,
    /// Computer use is paused (the Island's Pause).
    cu_pause: std::sync::atomic::AtomicBool,
    /// Applies the speech engines for new settings (the model manager; PLAN-06 by voice).
    engines_hook: RwLock<Option<EnginesHook>>,
    /// Agent Skills (CONV-32): their names and descriptions go into each brain request.
    skills: RwLock<Option<Arc<crate::skills::Skills>>>,
    /// The memory vault (CONVERSATION §6): memories for each brain request, "remember …".
    memory: RwLock<Option<Arc<crate::memory::Memory>>>,
    /// Counts media activities shown, so only the newest one's timer removes it.
    media_shown: Arc<AtomicU64>,
    /// Reads the selection in the app in front when the text box opens (UX-42).
    uia: RwLock<Option<Arc<dyn kivo_platform::UiAutomation>>>,
    /// The last finished turn as the Island showed it: "try again", "that's not what I meant"
    /// and "turn it on" act on it (UX-55).
    previous: Mutex<Option<kivo_ipc::protocol::TurnView>>,
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
            next_turn: AtomicU64::new(u64::try_from(kivo_store::brains::now_ms()).unwrap_or(1)),
            echo_cancelled: std::sync::atomic::AtomicBool::new(false),
            voice_id: RwLock::new(None),
            previews: Mutex::new(std::collections::HashSet::new()),
            brains: parts.brains,
            agents: parts.agents,
            session: Mutex::new(brain::VoiceSession::default()),
            verifier: parts.verifier,
            last_undo: Mutex::new(None),
            icons: Mutex::new(std::collections::HashMap::new()),
            commands: parts.commands,
            input_abort: parts.input_abort,
            vision: parts.vision,
            tasks: RwLock::new(None),
            app_registry: RwLock::new(None),
            routines: RwLock::new(None),
            workspaces: RwLock::new(None),
            last_calls: Mutex::new(None),
            screen: RwLock::new(None),
            cu_pause: std::sync::atomic::AtomicBool::new(false),
            engines_hook: RwLock::new(None),
            media_shown: Arc::new(AtomicU64::new(0)),
            skills: RwLock::new(None),
            memory: RwLock::new(None),
            previous: Mutex::new(None),
            uia: RwLock::new(None),
        }
    }

    pub fn set_screen(&self, screen: Arc<dyn kivo_platform::Screen>) {
        *write(&self.screen) = Some(screen);
    }

    pub(crate) fn screen(&self) -> Option<Arc<dyn kivo_platform::Screen>> {
        read(&self.screen).clone()
    }

    /// Pauses or resumes computer use (the Island's Pause, CAP-12). Returns whether it's paused.
    pub fn computer_pause(&self) -> bool {
        let paused = !self.cu_pause.load(std::sync::atomic::Ordering::SeqCst);
        self.cu_pause
            .store(paused, std::sync::atomic::Ordering::SeqCst);
        paused
    }

    pub(crate) fn cu_paused(&self) -> bool {
        self.cu_pause.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// What applies the speech engines when settings change outside the settings RPC (a product
    /// mode switched by voice).
    pub fn set_engines_hook(&self, hook: EnginesHook) {
        *write(&self.engines_hook) = Some(hook);
    }

    /// Switches the product mode (PLAN-06): the settings it maps to, the engine's own and the
    /// speech engines. Performance loads the models ahead. Returns the new settings.
    pub fn switch_mode(
        self: &Arc<Self>,
        mode: kivo_core::config::ProductMode,
    ) -> kivo_core::KivoConfig {
        let saved = crate::modes::apply(&self.core, self, mode);
        self.settings_changed(&saved);
        if let Some(hook) = read(&self.engines_hook).clone() {
            hook(&saved);
        }
        // Performance: models loaded ahead (plan §142, "aggressive prewarming").
        if mode == kivo_core::config::ProductMode::Performance {
            self.infer.warm();
        }
        saved
    }

    pub fn set_workspaces(&self, workspaces: Arc<crate::workspaces::Workspaces>) {
        *write(&self.workspaces) = Some(workspaces);
    }

    pub fn workspaces(&self) -> Option<Arc<crate::workspaces::Workspaces>> {
        read(&self.workspaces).clone()
    }

    /// A decision's spoken question: "Close Chrome?" — and in voice-only use, the words that
    /// answer it, since the buttons can't be seen (Settings → Accessibility).
    pub(crate) fn question(&self, action: &str) -> String {
        if self.core.config().accessibility.voice_only {
            format!("{action}? {}", text::t("confirm.sayOptions"))
        } else {
            format!("{action}?")
        }
    }

    /// Answers the Island's offer (by voice, or its buttons): "Remember … as a workspace?"
    /// (CONV-10) or "Remember this?" (CONV-20). Returns what to say, and the workspace when one
    /// was remembered.
    pub fn answer_offer(
        &self,
        id: &str,
        accept: bool,
    ) -> Result<(String, Option<kivo_ipc::protocol::WorkspaceItem>), String> {
        if let Some(n) = id.strip_prefix("memory:") {
            let n: i64 = n.parse().map_err(|_| text::t("memory.notFound"))?;
            let memory = self.memory().ok_or_else(|| text::t("memory.notFound"))?;
            return match memory.answer(n, accept, None)? {
                Some(crate::memory::Remembered::Already { .. }) => {
                    Ok((text::t("memory.already"), None))
                }
                Some(_) => Ok((text::t("memory.added"), None)),
                None => Ok((text::t("reply.okay"), None)),
            };
        }
        match self.workspaces().map(|w| w.answer(id, accept)) {
            Some(Ok(Some(w))) => {
                self.agents.set_workspace(w.path.clone().into());
                Ok((
                    text::tf("workspace.remembered", &[("name", &w.name)]),
                    Some(w),
                ))
            }
            Some(Ok(None)) => Ok((text::t("reply.okay"), None)),
            Some(Err(e)) => Err(e),
            None => Err(text::t("workspace.notFound")),
        }
    }

    pub fn set_memory(&self, memory: Arc<crate::memory::Memory>) {
        *write(&self.memory) = Some(memory);
    }

    pub fn memory(&self) -> Option<Arc<crate::memory::Memory>> {
        read(&self.memory).clone()
    }

    pub fn set_routines(&self, routines: Arc<crate::routines::Routines>) {
        *write(&self.routines) = Some(routines);
    }

    pub fn routines(&self) -> Option<Arc<crate::routines::Routines>> {
        read(&self.routines).clone()
    }

    /// A routine's hotkey was pressed (ROUT-05): it runs as a task, and says it started.
    pub fn run_routine(&self, id: &str) -> Result<String, String> {
        let routines = self.routines().ok_or_else(|| text::t("routine.notFound"))?;
        let name = routines.get(id).map(|r| r.name).unwrap_or_default();
        let task = routines.run(id, &serde_json::Map::new())?;
        tracing::info!(routine = %name, "routine started by its hotkey");
        Ok(task)
    }

    /// The installed apps, as last read (the Agents page's desktop AI apps, DISC-06).
    pub fn installed_apps(&self) -> Vec<kivo_platform::AppEntry> {
        self.app_catalog
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// The skills, once they exist.
    pub fn skills(&self) -> Option<Arc<crate::skills::Skills>> {
        read(&self.skills).clone()
    }

    pub fn set_skills(&self, skills: Arc<crate::skills::Skills>) {
        *self
            .skills
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(skills);
    }

    /// The skills layer of a brain request: the enabled skills' names and descriptions.
    pub fn skills_index(&self) -> String {
        self.skills
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .map(|s| s.index())
            .unwrap_or_default()
    }

    pub fn set_uia(&self, uia: Arc<dyn kivo_platform::UiAutomation>) {
        *self
            .uia
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(uia);
    }

    /// Ctrl+Shift+Space (UX-41, UX-42): before the Island takes focus, the text selected in the app
    /// in front is read (UIA TextPattern, never a password field) and kept in the runtime, so the
    /// text box can offer Explain · Rewrite · Translate. Needs the UI Automation capability.
    pub async fn capture_selection(&self) {
        let uia = self
            .uia
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let allowed = self
            .core
            .config()
            .capabilities
            .enabled(kivo_core::Capability::UiAutomation);
        let text = match uia.filter(|_| allowed) {
            Some(uia) => tokio::task::spawn_blocking(move || uia.selected_text())
                .await
                .ok()
                .and_then(Result::ok)
                .flatten()
                .filter(|t| !t.trim().is_empty()),
            None => None,
        };
        self.core.set_selection(text);
    }

    pub fn set_tasks(&self, tasks: Arc<crate::tasks::Tasks>) {
        *write(&self.tasks) = Some(tasks);
    }

    pub fn tasks(&self) -> Option<Arc<crate::tasks::Tasks>> {
        read(&self.tasks).clone()
    }

    pub fn set_app_registry(&self, apps: Arc<kivo_tools::appreg::AppRegistry>) {
        *write(&self.app_registry) = Some(apps);
    }

    /// "What can I say?" (UX-44): examples for the app in front (from the App Capability
    /// Registry), then general ones.
    pub fn help_examples(&self) -> Vec<String> {
        let front = self.windows.foreground().ok().flatten();
        let mut out: Vec<String> = front
            .and_then(|w| {
                read(&self.app_registry)
                    .as_ref()
                    .and_then(|r| r.find_by_exe(&w.app_id).map(|e| e.examples.clone()))
            })
            .unwrap_or_default();
        out.truncate(4);
        for general in text::t("help.general").split('|') {
            if out.len() >= 5 {
                break;
            }
            let general = general.trim().to_owned();
            if !general.is_empty() && !out.contains(&general) {
                out.push(general);
            }
        }
        out
    }

    pub fn set_voice_id(&self, voice_id: Arc<crate::voiceid::VoiceId>) {
        *write(&self.voice_id) = Some(voice_id);
    }

    pub fn voice_id(&self) -> Option<Arc<crate::voiceid::VoiceId>> {
        read(&self.voice_id).clone()
    }

    pub fn set_listener(&self, listener: Arc<Listener>) {
        *write(&self.listener) = Some(listener);
    }

    fn listener(&self) -> Option<Arc<Listener>> {
        read(&self.listener).clone()
    }

    /// The voice pipeline's controller (enrollment and wake-word recordings).
    pub fn listener_handle(&self) -> Option<Arc<Listener>> {
        self.listener()
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
        self.start_turn(source, None).await
    }

    /// A wake word was heard (VOICE §4). The listener has already started the utterance (with the
    /// audio from just before the wake word ended, VOICE-05); this starts the turn around it. A
    /// request in progress is interrupted, as barge-in by wake word.
    pub async fn woke(
        self: &Arc<Self>,
        utterance: u64,
        word: &str,
        phrase: &str,
        score: f32,
        clip: Vec<f32>,
    ) {
        // Whose voice it was is judged at the end of the request, on the whole utterance: the
        // wake word alone is too short for a steady voiceprint (VOICE-14, VOICE-22).
        let _ = clip;
        if lock(&self.turn).is_some() {
            self.cancel_turn(CancelReason::BargeIn, false);
        }
        self.core
            .bus
            .publish(Event::new(EventKind::Voice(VoiceEvent::WakeDetected {
                word_id: word.to_owned(),
                score,
            })));
        match self.start_turn(TurnSource::WakeWord, Some(utterance)).await {
            Ok(()) => {
                if let Some(running) = lock(&self.turn).as_mut() {
                    running.wake_phrase = Some(phrase.to_owned());
                }
            }
            // Speech isn't set up (already said): the listener's utterance goes nowhere.
            Err(_) => self.cancel_listening(),
        }
    }

    /// Listens for a spoken answer to the decision on the card (CONV-26): once KIVO has finished
    /// asking (so it can't answer itself, CONV-28), for 10 s without the wake word. Typed turns
    /// and a deferred card ("wait") just wait for a click.
    fn listen_for_answer(self: &Arc<Self>) {
        let voice_turn = lock(&self.turn)
            .as_ref()
            .is_some_and(|t| t.source != TurnSource::Typed);
        let deferred = self.core.turn_view().is_some_and(|v| v.waiting);
        if !voice_turn || deferred {
            return;
        }
        let Some(listener) = self.listener() else {
            return;
        };
        let engine = Arc::clone(self);
        tokio::spawn(async move {
            // The question may still be playing: it is synthesized faster than it is heard.
            let deadline = Instant::now() + Duration::from_secs(10);
            while engine.speaker.speaking() && Instant::now() < deadline {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            let utterance = engine.infer.next_utterance();
            let (started, started_rx) = tokio::sync::watch::channel(false);
            {
                let mut turn = lock(&engine.turn);
                let Some(running) = turn.as_mut().filter(|t| t.pending.is_some()) else {
                    return;
                };
                running.utterance = utterance;
                running.answering = true;
                running.answer_audio = None;
                running.stt_started = started_rx;
            }
            engine.core.update_turn(|view| view.answering = true);
            listener.listen_for_answer(utterance);
            engine.start_recognition(utterance, started);
        });
    }

    /// The audio of a request or a spoken answer (for the owner's-voice check).
    pub fn heard(&self, utterance: u64, audio: Vec<f32>) {
        if let Some(t) = lock(&self.turn)
            .as_mut()
            .filter(|t| t.utterance == utterance)
        {
            t.answer_audio = Some(audio);
        }
    }

    /// Stage 2 for a spoken request (VOICE-14, VOICE-22): whose voice was it? In "Owner only"
    /// another voice's request is dropped with a soft sound; in "Prefer owner" it becomes a guest
    /// turn. Returns false when the request is dropped.
    async fn voice_check(self: &Arc<Self>) -> bool {
        let config = self.core.config();
        let mode = config.voice.speaker_mode;
        let recognizing = mode != kivo_core::config::SpeakerMode::Off
            && config
                .capabilities
                .enabled(kivo_core::Capability::SpeakerRecognition);
        let audio = lock(&self.turn)
            .as_mut()
            .and_then(|t| t.answer_audio.take());
        let (true, Some(voice_id), Some(audio)) = (recognizing, self.voice_id(), audio) else {
            return true;
        };
        let verdict = tokio::task::spawn_blocking(move || voice_id.check(&audio))
            .await
            .unwrap_or(crate::voiceid::Verdict::Unknown);
        if !matches!(verdict, crate::voiceid::Verdict::Stranger { .. }) {
            return true;
        }
        if mode == kivo_core::config::SpeakerMode::OwnerOnly {
            tracing::info!(?verdict, "a request in another voice; ignored");
            self.speaker.cue(Cue::Cancelled);
            self.cancel_turn(CancelReason::UserVoice, true);
            return false;
        }
        if let Some(t) = lock(&self.turn).as_mut() {
            t.guest = true;
        }
        self.core.update_turn(|view| view.guest = true);
        true
    }

    /// The Draft card (CONV-15): a prompt KIVO will type into another AI, shown with its target
    /// until the user says "send", changes it or cancels.
    pub(crate) fn show_draft(&self, call: &ToolCall) {
        // A proposed plan (BRAIN-30) shows its steps on the card, before it is approved.
        if call.tool == "tasks.propose_plan" {
            let registry = &self.registry;
            let steps: Vec<StepView> = call.args["steps"]
                .as_array()
                .map(|steps| {
                    steps
                        .iter()
                        .enumerate()
                        .map(|(i, s)| {
                            let tool = s["tool"].as_str().unwrap_or_default();
                            let title = registry.known(tool).map_or_else(
                                || s["title"].as_str().unwrap_or(tool).to_owned(),
                                |spec| kivo_security::render_title(&spec.title, &s["args"]),
                            );
                            StepView {
                                id: format!("{}-p{i}", call.id),
                                title,
                                status: StepStatus::Pending,
                                detail: None,
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            self.core.update_turn(|view| view.steps.extend(steps));
            return;
        }
        if call.tool != DRAFT_TOOL {
            return;
        }
        let text = call.args["text"].as_str().unwrap_or_default().to_owned();
        let target = call
            .targets
            .iter()
            .find_map(|t| match t {
                kivo_core::tool::Target::Window { title, .. } => {
                    Some(text::tf("draft.target", &[("title", title)]))
                }
                _ => None,
            })
            .unwrap_or_else(|| text::t("draft.anyTerminal"));
        self.core.update_turn(|view| {
            view.draft = Some(kivo_ipc::protocol::DraftView {
                target: target.clone(),
                text: text.clone(),
            });
        });
    }

    /// Changes the waiting Draft's text; returns it.
    fn change_draft(&self, change: impl FnOnce(&str) -> String) -> Option<String> {
        let new = {
            let mut turn = lock(&self.turn);
            let running = turn.as_mut()?;
            let (_, call) = running
                .pending
                .as_mut()
                .filter(|(_, c)| c.tool == DRAFT_TOOL)?;
            let new = change(call.args["text"].as_str().unwrap_or_default());
            call.args["text"] = serde_json::Value::String(new.clone());
            running.draft_text = Some(new.clone());
            new
        };
        self.core.update_turn(|view| {
            if let Some(d) = view.draft.as_mut() {
                d.text.clone_from(&new);
            }
        });
        Some(new)
    }

    /// The Draft card's Edit (CONV-15): the user rewrote the prompt.
    pub fn edit_draft(&self, call_id: &str, text: &str) -> Result<(), String> {
        let waiting = lock(&self.turn)
            .as_ref()
            .and_then(|t| t.pending.as_ref())
            .is_some_and(|(spec, call)| spec.call_id == call_id && call.tool == DRAFT_TOOL);
        if !waiting {
            return Err(text::t("turn.nothingWaiting"));
        }
        self.change_draft(|_| text.trim().to_owned());
        Ok(())
    }

    /// The Draft's text as the user left it, for the call that sends it.
    pub(crate) fn drafted(&self, mut call: ToolCall) -> ToolCall {
        if call.tool == DRAFT_TOOL
            && let Some(text) = lock(&self.turn).as_mut().and_then(|t| t.draft_text.take())
        {
            call.args["text"] = serde_json::Value::String(text);
        }
        call
    }

    /// What the user said to the decision (CONV-27), under the voice rules (CONV-28).
    async fn decision_answer(self: &Arc<Self>, said: &str) {
        use kivo_intent::answers::{Answer as Said, DraftEdit, draft_edit, parse_answer};
        // A Draft is changed by voice before anything counts as an answer (CONV-15).
        let drafting = lock(&self.turn)
            .as_ref()
            .and_then(|t| t.pending.as_ref())
            .is_some_and(|(_, c)| c.tool == DRAFT_TOOL);
        if drafting && let Some(edit) = draft_edit(said) {
            if let Some(t) = lock(&self.turn).as_mut() {
                t.answering = false;
            }
            self.core.update_turn(|view| view.answering = false);
            let reply = match &edit {
                DraftEdit::ReadBack => self.change_draft(ToOwned::to_owned).unwrap_or_default(),
                _ => {
                    self.change_draft(|t| kivo_intent::answers::apply_draft_edit(t, &edit));
                    text::t("draft.changed")
                }
            };
            self.speak(&reply).await;
            self.listen_for_answer();
            return;
        }
        let (spec, guest, audio, reasked) = {
            let mut turn = lock(&self.turn);
            let Some(t) = turn.as_mut() else { return };
            t.answering = false;
            let Some((spec, _)) = t.pending.clone() else {
                return;
            };
            (spec, t.guest, t.answer_audio.take(), t.reasked)
        };
        self.core.update_turn(|view| view.answering = false);
        let language = self.core.config().general.language;
        match parse_answer(said, &language) {
            Some(answer @ (Said::Approve | Said::ApproveAlways)) => {
                // A spoken approval that doesn't count is said aloud and kept in Activity.
                let strong = spec.strength == kivo_core::tool::Strength::Strong;
                let refused = if guest {
                    Some("decision.guest")
                } else if strong && !self.verifier.available() {
                    // High risk without Windows Hello: only a click; voice alone is never enough.
                    Some("decision.click")
                } else if !self.owner_said_it(audio).await {
                    Some("decision.notOwner")
                } else {
                    None
                };
                if let Some(key) = refused {
                    let message = text::t(key);
                    self.recorder.answer(&self.turn_key(), &message, "refused");
                    self.speak(&message).await;
                    return;
                }
                // High risk: the spoken "approve" starts Windows Hello, which must finish it
                // (CONV-29).
                if strong {
                    if let Err(e) = self.approve_with_hello(&spec.call_id).await {
                        self.speak(&e).await;
                    }
                    return;
                }
                let always = (answer == Said::ApproveAlways && spec.allow_always)
                    .then_some(kivo_core::tool::GrantDuration::Always);
                if let Err(e) = self
                    .answer_by(&spec.call_id, true, always, ConfirmedBy::Voice)
                    .await
                {
                    tracing::warn!(e, "voice approval refused");
                }
            }
            Some(Said::Deny) => {
                let _ = self
                    .answer_by(&spec.call_id, false, None, ConfirmedBy::Voice)
                    .await;
            }
            Some(Said::Defer) => {
                // The card stays with no timeout ("Waiting for you"), and no sound.
                self.core.update_turn(|view| view.waiting = true);
            }
            Some(Said::Explain) => {
                let explanation = text::tf(
                    "decision.explain",
                    &[("action", &spec.action), ("why", &spec.why)],
                );
                self.speak(&explanation).await;
            }
            // "No, make it …": the change goes to a brain as a corrected request (CONV-27).
            Some(Said::Edit(change)) => self.edit_request(&spec, &change).await,
            None => {
                if reasked < 2 {
                    if let Some(t) = lock(&self.turn).as_mut() {
                        t.reasked += 1;
                    }
                    self.speak(&text::t("decision.sayAgain")).await;
                }
            }
        }
    }

    /// CONV-28: with speaker recognition on, a spoken approval counts only in the owner's voice.
    /// With it off, the signed-in user at this PC is taken to be the owner.
    async fn owner_said_it(&self, audio: Option<Vec<f32>>) -> bool {
        let config = self.core.config();
        let recognizing = config.voice.speaker_mode != kivo_core::config::SpeakerMode::Off
            && config
                .capabilities
                .enabled(kivo_core::Capability::SpeakerRecognition);
        let (true, Some(voice_id)) = (recognizing, self.voice_id()) else {
            return true;
        };
        let Some(audio) = audio else {
            return false;
        };
        let verdict = tokio::task::spawn_blocking(move || voice_id.check(&audio))
            .await
            .unwrap_or(crate::voiceid::Verdict::Unknown);
        !matches!(verdict, crate::voiceid::Verdict::Stranger { .. })
    }

    /// The user talked over KIVO (VOICE-31): its reply stops with a short fade and what they
    /// said becomes the next request (the listener has already started the utterance).
    pub async fn barge_in(self: &Arc<Self>, utterance: u64) {
        if lock(&self.turn).is_some() {
            self.cancel_turn(CancelReason::BargeIn, false);
        }
        // Whatever KIVO was saying stops (a 30 ms fade), turn or no turn.
        self.speaker.stop();
        if self
            .start_turn(TurnSource::FollowUp, Some(utterance))
            .await
            .is_err()
        {
            self.cancel_listening();
        }
    }

    /// Speech in the follow-up window (UX-45): a new request without the wake word.
    pub async fn follow_up_heard(self: &Arc<Self>, utterance: u64) {
        if self
            .start_turn(TurnSource::FollowUp, Some(utterance))
            .await
            .is_err()
        {
            self.cancel_listening();
        }
    }

    /// "Kivo stop", "stop" or "cancel" while KIVO was busy (VOICE-19, SEC-26). Without echo
    /// cancellation the microphone also hears KIVO, so a stop word that is in KIVO's own reply
    /// while it plays is taken for KIVO's voice (VOICE-30 removes that voice first).
    pub fn stop_heard(&self, word: &str) {
        let said = word.replace('-', " ");
        let own_voice = lock(&self.turn).as_ref().is_some_and(|t| {
            let reply = t.reply.to_lowercase();
            reply
                .split(|c: char| !c.is_alphanumeric())
                .any(|w| !w.is_empty() && said.split(' ').any(|s| s == w))
        });
        if own_voice && self.speaker.busy() && !self.echo_cancelled() {
            tracing::debug!(word, "stop word in KIVO's own reply; ignored");
            return;
        }
        tracing::info!(word, "stopped by voice");
        self.cancel(CancelReason::UserVoice);
    }

    /// Whether the microphone signal has KIVO's own output removed (VOICE-30).
    fn echo_cancelled(&self) -> bool {
        self.echo_cancelled
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Says whether echo cancellation is running (set by the audio pipeline).
    pub fn set_echo_cancelled(&self, on: bool) {
        self.echo_cancelled
            .store(on, std::sync::atomic::Ordering::Relaxed);
    }

    /// Speaks `text` outside a turn, with `voice` or the chosen one: "hear it" for a wake word and
    /// voice previews (VOICE-16, UX-62). Not while a request is running.
    pub async fn preview(self: &Arc<Self>, text: &str, voice: Option<&str>) -> Result<(), String> {
        if lock(&self.turn).is_some() {
            return Err(text::t("preview.busy"));
        }
        self.infer.warm();
        if !self.infer.wait_ready(WORKER_START).await {
            // No worker: Windows' voice says it in this process.
            self.say_in_process(text);
            return Ok(());
        }
        let id = self.infer.next_utterance();
        lock(&self.previews).insert(id);
        let config_voice = self.core.config().voice.tts_voice;
        let voice = voice
            .map(str::to_owned)
            .or_else(|| (!config_voice.is_empty()).then_some(config_voice));
        let result = self.infer.speak(id, text, voice.as_deref()).await;
        if result.is_err() {
            lock(&self.previews).remove(&id);
        }
        result.map_err(|e| e.to_string())
    }

    /// The fallback voice, for synthesizing background speech in the wake-word false-alarm test.
    pub fn system_voice(&self) -> Option<Arc<dyn kivo_platform::SpeechSynth>> {
        self.fallback_voice.clone()
    }

    async fn start_turn(
        self: &Arc<Self>,
        source: TurnSource,
        pre_started: Option<u64>,
    ) -> Result<(), String> {
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
        self.prewarm();
        let id = self.turn_id();
        self.core.begin_turn(&id, source, "").map_err(|e| e.0)?;
        self.anchor_island();
        self.quiet_if_busy();
        let config = self.core.config();
        let utterance = pre_started.unwrap_or_else(|| self.infer.next_utterance());
        let (started, started_rx) = tokio::sync::watch::channel(false);
        let running = Running {
            stt_started: started_rx,
            ..Running::new(id.clone(), utterance, source)
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
        if pre_started.is_none()
            && let Some(listener) = self.listener()
        {
            listener.listen(
                utterance,
                config.voice.auto_end_on_silence || source != TurnSource::PushToTalk,
            );
        }
        self.mark("t1Listening");
        self.start_recognition(utterance, started);
        Ok(())
    }

    /// Predictive prewarming while the user speaks (PLAN-10), never with side effects: the app
    /// index the fast path resolves against, and the connection to the brain a request would
    /// most likely go to. (Speech recognition is warmed by `infer.warm`.)
    fn prewarm(self: &Arc<Self>) {
        if read(&self.app_index).1.elapsed() > APPS_TTL {
            let engine = Arc::clone(self);
            tokio::task::spawn_blocking(move || engine.refresh_apps());
        }
        let brains = Arc::clone(&self.brains);
        let config = self.core.config();
        tokio::spawn(async move { brains.prewarm(&config).await });
    }

    /// Starts recognition for `utterance` as soon as the worker is up; `started` says when.
    fn start_recognition(
        self: &Arc<Self>,
        utterance: u64,
        started: tokio::sync::watch::Sender<bool>,
    ) {
        let engine = Arc::clone(self);
        let language = self.core.config().general.language;
        tokio::spawn(async move {
            // The language's own words plus the user's (names, apps, projects): engines that
            // take hotwords or a prompt use them (VOICE-23).
            let mut vocabulary = kivo_voice::language::pack(&language)
                .map(|p| p.vocabulary)
                .unwrap_or_default();
            let db = engine.brains.database();
            let own = db
                .lock()
                .map(|db| db.vocabulary(100).unwrap_or_default())
                .unwrap_or_default();
            vocabulary.extend(own);
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
                    // The audio heard so far reaches the worker before the result can be asked
                    // for (an utterance can end before a slow worker has started it).
                    if let Some(listener) = engine.listener() {
                        let sent = listener.stt_ready(utterance);
                        let _ = tokio::time::timeout(Duration::from_secs(2), sent).await;
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
        self.brains.reload(config);
        self.infer.set_speed(config.voice.tts_speed);
        if let Some(listener) = self.listener() {
            listener.set_device(
                config
                    .voice
                    .input_device
                    .clone()
                    .map(kivo_platform::DeviceId),
            );
        }
        self.speaker.configure(&config.sounds);
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
        self.say_in(text, None, None, Vec::new()).await
    }

    /// A typed request in a chosen thread and with a chosen profile (Chat, UX-21).
    pub async fn say_in(
        self: &Arc<Self>,
        text: &str,
        thread: Option<String>,
        profile: Option<String>,
        attachments: Vec<(String, String)>,
    ) -> Result<(), String> {
        let text = text.trim();
        if text.is_empty() {
            return Err(text::t("turn.nothingToSend"));
        }
        self.infer.warm();
        let id = self.turn_id();
        self.core
            .begin_turn(&id, TurnSource::Typed, text)
            .map_err(|e| e.0)?;
        self.anchor_island();
        self.quiet_if_busy();
        *lock(&self.turn) = Some(Running {
            transcript: text.to_owned(),
            thread,
            brain_choice: profile,
            attachments,
            ..Running::new(id.clone(), 0, TurnSource::Typed)
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
        let answering = lock(&self.turn).as_ref().is_some_and(|t| t.answering);
        if answering {
            self.answer_heard(utterance).await;
            return;
        }
        self.mark("t4EndOfSpeech");
        self.core.end_of_speech();
        self.speaker.cue(Cue::ListenStop);
        self.thinking_cue_later();
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
        if !self.voice_check().await {
            return;
        }
        let wake_phrase = lock(&self.turn)
            .as_ref()
            .and_then(|t| t.wake_phrase.clone());
        let text = match wake_phrase {
            Some(phrase) => kivo_intent::strip_wake_phrase(&text, &phrase),
            None => text,
        };
        if text.trim().is_empty() {
            let message = text::t("turn.notCaught");
            self.show_error(&message);
            self.speak_and_finish(&message).await;
            return;
        }
        self.handle_transcript(&text).await;
    }

    /// The spoken answer to a decision ended: recognize it and act on it.
    async fn answer_heard(self: &Arc<Self>, utterance: u64) {
        let started = lock(&self.turn).as_ref().map(|t| t.stt_started.clone());
        if let Some(mut started) = started {
            let ready = tokio::time::timeout(WORKER_START, started.wait_for(|s| *s)).await;
            if !matches!(ready, Ok(Ok(_))) {
                return;
            }
        }
        match self.infer.finish_stt(utterance).await {
            Ok(said) => self.decision_answer(&said.text).await,
            Err(e) => tracing::warn!(%e, "couldn't recognize the answer"),
        }
    }

    /// Nothing was said (or the microphone is blocked).
    pub fn nothing_heard(&self, utterance: u64) {
        // No spoken answer: the card waits for a click (CONV-26).
        let answering = lock(&self.turn).as_mut().is_some_and(|t| {
            let was = t.utterance == utterance && t.answering;
            if was {
                t.answering = false;
            }
            was
        });
        if answering {
            self.core.update_turn(|view| view.answering = false);
            return;
        }
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
        let Some(running) = turn
            .as_mut()
            .filter(|t| t.utterance == utterance && !t.answering && !t.heard_final)
        else {
            return;
        };
        if !stable {
            running.transcript = text.to_owned();
            // The end-of-turn check waits past a pause after the name alone (VOICE-33).
            if let Some(listener) = self.listener() {
                listener.heard_so_far(utterance, text);
            }
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
        // First, so a late partial can't replace the final text on the Island.
        if let Some(running) = lock(&self.turn).as_mut() {
            running.transcript.clone_from(&text);
            running.heard_final = true;
        }
        self.core.update_turn(|view| {
            view.transcript.clone_from(&text);
            view.transcript_final = true;
        });
        self.recorder.transcript(&self.turn_key(), &text);
        self.core
            .bus
            .publish(Event::new(EventKind::Voice(VoiceEvent::FinalTranscript {
                text: text.clone(),
                confidence: None,
            })));

        // "Kivo, stop" caught as a request (barge-in is quicker than the stop-word spotter): the
        // turn ends quietly (VOICE-19).
        if kivo_intent::is_stop_request(&text) {
            tracing::info!("stopped by a spoken request");
            self.cancel(CancelReason::UserVoice);
            return;
        }

        // An offer in the Island ("Remember kivo as a workspace?") answered by voice (UX-55).
        if let Some(offer) = self.core.offer()
            && let Some(answer) =
                kivo_intent::answers::parse_answer(&text, &self.core.config().general.language)
            && matches!(
                answer,
                kivo_intent::answers::Answer::Approve
                    | kivo_intent::answers::Answer::ApproveAlways
                    | kivo_intent::answers::Answer::Deny
            )
        {
            let accept = !matches!(answer, kivo_intent::answers::Answer::Deny);
            let reply = match self.answer_offer(&offer.id, accept) {
                Ok((reply, _)) | Err(reply) => reply,
            };
            self.speak_and_finish(&reply).await;
            return;
        }

        // Routines and custom commands first: the user's own phrases win, and need no AI
        // (ROUT-05, ROUT-06).
        if let Some(routines) = self.routines()
            && let Some((routine, vars)) = routines.match_text(&text)
        {
            self.run_matched_routine(&routines, routine, &vars).await;
            return;
        }

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
                // "Kivo, undo that" (UX-43).
                if matched.tool == "session.undo" {
                    self.undo_in_turn().await;
                    return;
                }
                // "What did I miss?" (UX-40).
                if matched.tool == "session.missed" {
                    let summary = self.tasks().map_or_else(
                        || text::t("notify.nothingMissed"),
                        |t| t.notifier().missed_summary(),
                    );
                    self.speak_and_finish(&summary).await;
                    return;
                }
                // The Island's buttons by voice (UX-55).
                if let Some(page) = matched.tool.strip_prefix("session.")
                    && matches!(page, "retry" | "misroute" | "enable" | "open")
                {
                    let page_arg = matched
                        .args
                        .get("page")
                        .and_then(|v| v.as_str().map(str::to_owned));
                    Box::pin(self.island_by_voice(page, page_arg.as_deref())).await;
                    return;
                }
                // "Save what you just did as a routine" (ROUT-13): a draft to review.
                if matched.tool == "session.saveRoutine" {
                    let last = lock(&self.last_calls).clone();
                    let draft = last.and_then(|(said, calls)| {
                        crate::routines::Routines::draft_from_calls(&said, &calls)
                    });
                    let reply = match (draft, self.routines()) {
                        (Some(draft), Some(routines)) => {
                            routines.offer_draft(draft);
                            text::t("routine.fromLastTurn")
                        }
                        _ => text::t("routine.nothingToSave"),
                    };
                    self.speak_and_finish(&reply).await;
                    return;
                }
                // "Presentation mode", "back to normal" (PLAN-06).
                if matched.tool == "session.mode" {
                    let mode = matched
                        .args
                        .get("mode")
                        .and_then(|v| serde_json::from_value(v.clone()).ok());
                    let Some(mode) = mode else {
                        self.speak_and_finish(&text::t("turn.gone")).await;
                        return;
                    };
                    let engine = Arc::clone(self);
                    let _ = tokio::task::spawn_blocking(move || engine.switch_mode(mode)).await;
                    let name = text::t(&format!("modes.{}", text::key_of(&mode)));
                    let reply = text::tf("modes.switched", &[("mode", &name)]);
                    self.speak_and_finish(&reply).await;
                    return;
                }
                // "What can I say?" (UX-44).
                if matched.tool == "session.help" {
                    let examples = self.help_examples();
                    self.core
                        .update_turn(|view| view.help.clone_from(&examples));
                    self.speak_and_finish(&text::t("help.intro")).await;
                    return;
                }
                // What to remember or forget is kept in the user's own words (a path, a name's
                // case), not the grammar's normalized ones (MEM-05).
                let mut matched = matched;
                if matches!(matched.tool.as_str(), "memory.add" | "memory.forget")
                    && let Some(slot) = matched.args.get("text").and_then(|v| v.as_str())
                    && let Some(own) = kivo_intent::normalize::original_span(&text, slot)
                {
                    matched
                        .args
                        .insert("text".into(), serde_json::Value::String(own));
                }
                let call = ToolCall {
                    id: format!("{}-c1", self.turn_key()),
                    tool: matched.tool.clone(),
                    args: serde_json::Value::Object(matched.args.clone()),
                    initiated_by: Initiator::UserDirect,
                    targets: targets(&matched.tool, &serde_json::Value::Object(matched.args)),
                };
                self.run_call(call).await;
            }
            // Everything else goes to a brain (BRAINS §1).
            Route::Unhandled => self.brain_turn(&text).await,
        }
    }

    /// A routine the user said: a custom command runs its one action now, like a fast-path
    /// command; a routine starts as a task.
    async fn run_matched_routine(
        self: &Arc<Self>,
        routines: &Arc<crate::routines::Routines>,
        routine: kivo_core::routine::Routine,
        vars: &serde_json::Map<String, serde_json::Value>,
    ) {
        self.core
            .bus
            .publish(Event::new(EventKind::Turn(TurnEvent::IntentDetected {
                path: IntentPath::FastPath,
                intent: format!("routine:{}", routine.name),
            })));
        if routine.is_custom_command() {
            let task = routine.compile(vars);
            if let kivo_core::task::StepAction::Tool { tool, args } = &task.steps[0].action {
                let call = ToolCall {
                    id: format!("{}-c1", self.turn_key()),
                    tool: tool.clone(),
                    args: args.clone(),
                    initiated_by: Initiator::UserDirect,
                    targets: targets(tool, args),
                };
                self.run_call(call).await;
                return;
            }
        }
        match routines.run(&routine.id, vars) {
            Ok(task) => {
                self.core
                    .update_turn(|view| view.task_id = Some(task.clone()));
                let message = text::tf("routine.running", &[("name", &routine.name)]);
                self.speak_and_finish(&message).await;
            }
            Err(e) => {
                self.core.update_turn(|view| view.error = Some(e.clone()));
                self.speak_and_finish(&e).await;
            }
        }
    }

    /// An app's icon as a `data:image/png` URL, looked up once (UX-46).
    fn app_icon(&self, id: &str) -> Option<String> {
        if let Some(known) = lock(&self.icons).get(id) {
            return known.clone();
        }
        let url = self.apps.icon(id, 32).and_then(|image| {
            use base64::Engine as _;
            let png = kivo_tools::screen_tools::png(&image).ok()?;
            Some(format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(png)
            ))
        });
        lock(&self.icons).insert(id.to_owned(), url.clone());
        url
    }

    /// The last change, as a call that takes it back (`None` when there is none, or it's too old).
    fn undo_call(&self) -> Option<ToolCall> {
        let entry = lock(&self.last_undo).take()?;
        if entry.at.elapsed() > UNDO_KEPT {
            return None;
        }
        Some(ToolCall {
            id: format!("{}-undo", self.turn_key()),
            tool: entry.tool,
            args: json!({ "undo": entry.data, "title": entry.title }),
            initiated_by: Initiator::UserDirect,
            targets: Vec::new(),
        })
    }

    /// Undo inside a turn (voice or typed "undo that").
    async fn undo_in_turn(self: &Arc<Self>) {
        self.core.update_turn(|view| view.undo = None);
        match self.undo_call() {
            Some(call) => self.run_call(call).await,
            None => self.speak_and_finish(&text::t("reply.nothingToUndo")).await,
        }
    }

    /// The Island's Undo button and the Control Center's toast (UX-43): a turn of its own.
    pub async fn undo_last(self: &Arc<Self>) -> Result<(), String> {
        if lock(&self.last_undo).is_none() {
            return Err(text::t("reply.nothingToUndo"));
        }
        self.say(&text::t("turn.undoRequest")).await
    }

    /// An action the user asked for with a button in the Control Center ("Start" on the Agents
    /// page): a typed turn of its own, whose call goes through the permission engine like a
    /// spoken one, so a High-risk launch waits for a yes in the Island (CONV-14).
    pub async fn act(
        self: &Arc<Self>,
        request: &str,
        tool: &str,
        args: serde_json::Value,
    ) -> Result<(), String> {
        let id = self.turn_id();
        self.core
            .begin_turn(&id, TurnSource::Typed, request)
            .map_err(|e| e.0)?;
        self.anchor_island();
        *lock(&self.turn) = Some(Running {
            transcript: request.to_owned(),
            ..Running::new(id.clone(), 0, TurnSource::Typed)
        });
        self.recorder.turn_started(&id, TurnSource::Typed);
        self.core.advance(SessionInput::EndOfSpeech);
        let call = ToolCall {
            id: format!("{id}-c1"),
            tool: tool.to_owned(),
            targets: targets(tool, &args),
            args,
            initiated_by: Initiator::UserDirect,
        };
        let engine = Arc::clone(self);
        tokio::spawn(async move { engine.run_call(call).await });
        Ok(())
    }

    /// Decides on a call and, when allowed, runs it.
    async fn run_call(self: &Arc<Self>, mut call: ToolCall) {
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
        let decision = self.authorize_call(tool.as_ref(), &mut call);
        self.mark("t7Permission");
        self.recorder
            .tool_decision(&self.turn_key(), &call, &spec, &decision);
        match decision {
            Decision::Allow(permit) => {
                self.execute(tool, call, permit, &spec.title).await;
            }
            Decision::Confirm(confirm) => {
                let confirm = self.with_hello(confirm);
                let title = confirm.action.clone();
                if let Some(running) = lock(&self.turn).as_mut() {
                    running.pending = Some((confirm.clone(), call.clone()));
                }
                self.core.advance(SessionInput::NeedConfirmation);
                self.core.update_turn(|view| {
                    view.confirm = Some(confirm.clone());
                    view.target_app = confirm.target.clone();
                });
                self.show_draft(&call);
                // A distinct cue says "this needs your answer" (CONV-26).
                self.speaker.cue(Cue::Question);
                let question = self.question(&title);
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
        let duration = always.then_some(kivo_core::tool::GrantDuration::Always);
        self.answer_confirmation_for(call_id, allow, duration).await
    }

    /// The user answered, with how long an "Always allow" should last (SEC-08).
    pub async fn answer_confirmation_for(
        self: &Arc<Self>,
        call_id: &str,
        allow: bool,
        duration: Option<kivo_core::tool::GrantDuration>,
    ) -> Result<(), String> {
        // A click while KIVO listens for a spoken answer: stop listening.
        let answering = lock(&self.turn).as_mut().is_some_and(|t| {
            let was = t.answering;
            t.answering = false;
            was
        });
        if answering {
            self.cancel_listening();
            self.core.update_turn(|view| view.answering = false);
        }
        self.answer_by(call_id, allow, duration, ConfirmedBy::Click)
            .await
    }

    async fn answer_by(
        self: &Arc<Self>,
        call_id: &str,
        allow: bool,
        duration: Option<kivo_core::tool::GrantDuration>,
        by: ConfirmedBy,
    ) -> Result<(), String> {
        let duration = duration.filter(|d| *d != kivo_core::tool::GrantDuration::Once);
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
        self.core.update_turn(|view| {
            view.confirm = None;
            view.waiting = false;
            view.draft = None;
        });
        // A brain or an agent is waiting for this decision: it carries on from here.
        let waiter = lock(&self.turn).as_mut().and_then(|t| t.waiter.take());
        if let Some(waiter) = waiter {
            self.recorder
                .confirmation(&self.turn_key(), &call, allow, by);
            let always = duration.is_some() && spec.allow_always;
            if allow && always {
                self.grant(&call, duration);
            }
            if allow {
                if by == ConfirmedBy::Voice {
                    self.speaker.cue(Cue::Approved);
                }
                self.confirmed_state();
            }
            let _ = waiter.send((allow, always, by));
            return Ok(());
        }
        if !allow {
            self.core.advance(SessionInput::Denied);
            self.recorder
                .confirmation(&self.turn_key(), &call, false, by);
            self.speaker.cue(Cue::Cancelled);
            self.core
                .update_turn(|view| view.answer = Some(text::t("reply.cancelled")));
            self.finish_turn("cancelled", Some(&text::t("reply.cancelled")));
            return Ok(());
        }
        let permit =
            kivo_security::confirmed(&spec, &call, Answer::Allow { by }).map_err(|e| e.message)?;
        if by == ConfirmedBy::Voice {
            self.speaker.cue(Cue::Approved);
        }
        self.recorder
            .confirmation(&self.turn_key(), &call, true, by);
        if spec.allow_always {
            self.grant(&call, duration);
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
        let (result, output) = self.run_step(tool, &call, permit, title).await;
        match (&result.status, output) {
            (Ok(_), Some(output)) => {
                self.show_track(&output.data);
                self.speak_and_finish(&output.say).await;
            }
            (Err(error), _) => {
                self.speaker.cue(Cue::Error);
                let message = retry_hint(error);
                self.core
                    .update_turn(|view| view.error = Some(message.clone()));
                self.speak_and_finish(&message).await;
            }
            (Ok(_), None) => self.finish_turn("done", None),
        }
    }

    /// "Try again", "that's not what I meant", "turn it on" and "open my tasks": what the Island's
    /// buttons do, by voice (UX-55). The first three act on the last finished turn.
    async fn island_by_voice(self: &Arc<Self>, action: &str, page: Option<&str>) {
        let previous = lock(&self.previous).clone();
        match action {
            "retry" => match previous.filter(|p| !p.transcript.is_empty()) {
                Some(p) => {
                    let again = p.transcript;
                    self.core
                        .update_turn(|view| view.transcript.clone_from(&again));
                    if let Some(running) = lock(&self.turn).as_mut() {
                        running.transcript.clone_from(&again);
                    }
                    Box::pin(self.handle_transcript(&again)).await;
                }
                None => {
                    self.speak_and_finish(&text::t("voice.nothingToRetry"))
                        .await
                }
            },
            "misroute" => match previous.filter(|p| p.brain.is_some()) {
                Some(p) => {
                    self.misroute(&p.id, "");
                    self.speak_and_finish(&text::t("voice.misrouteNoted")).await;
                }
                None => self.speak_and_finish(&text::t("voice.noBrainAnswer")).await,
            },
            "enable" => match previous.and_then(|p| p.capability_off) {
                Some(capability) => {
                    let mut changed = false;
                    self.core.update_config(|config| {
                        changed = config.capabilities.set(capability, true);
                        config.tools.preset = config.capabilities.preset();
                    });
                    if changed {
                        self.recorder.capability_changed(capability, true);
                    }
                    let reply = text::tf("voice.turnedOn", &[("capability", &capability.label())]);
                    self.speak_and_finish(&reply).await;
                }
                None => {
                    self.speak_and_finish(&text::t("voice.nothingToTurnOn"))
                        .await
                }
            },
            _ => {
                let page = page.unwrap_or("home");
                self.core.open_control_center(Some(page));
                self.finish_turn("done", None);
            }
        }
    }

    /// After a media command, the track playing shows as a live activity for a few seconds (UX-15,
    /// Settings → Live activities → Media).
    fn show_track(&self, data: &serde_json::Value) {
        let Some(track) = data.get("track") else {
            return;
        };
        if !self.core.config().automation.live_activities.media {
            return;
        }
        let Some(title) = track["title"].as_str().filter(|t| !t.is_empty()) else {
            return;
        };
        let detail = [track["artist"].as_str(), track["app"].as_str()]
            .into_iter()
            .flatten()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        self.core.set_activity(kivo_ipc::protocol::LiveActivity {
            id: MEDIA_ACTIVITY.to_owned(),
            kind: "media".to_owned(),
            title: title.to_owned(),
            detail: (!detail.is_empty()).then_some(detail),
            progress: None,
            until: None,
            task_id: None,
        });
        let core = Arc::clone(&self.core);
        let generation = self.media_shown.fetch_add(1, Ordering::SeqCst) + 1;
        let shown = Arc::clone(&self.media_shown);
        tokio::spawn(async move {
            tokio::time::sleep(MEDIA_SHOWN_FOR).await;
            // A newer track keeps showing for its own time.
            if shown.load(Ordering::SeqCst) == generation {
                core.remove_activity(MEDIA_ACTIVITY);
            }
        });
    }

    /// Runs an approved call as a step in the Island and records it; returns what it did.
    async fn run_step(
        self: &Arc<Self>,
        tool: Arc<dyn kivo_tools::Tool>,
        call: &ToolCall,
        permit: kivo_security::Permit,
        title: &str,
    ) -> (kivo_core::tool::ToolResult, Option<kivo_tools::Output>) {
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
        let target = call.targets.iter().find_map(|t| match t {
            kivo_core::tool::Target::App { id, name } => Some((id.clone(), name.clone())),
            _ => None,
        });
        let icon = target.as_ref().and_then(|(id, _)| self.app_icon(id));
        self.core.update_turn(|view| {
            if view.target_app.is_none() {
                view.target_app = target.as_ref().map(|(_, name)| name.clone());
                view.target_icon.clone_from(&icon);
            }
        });
        self.core.bus.publish(Event::new(EventKind::Tool(
            kivo_core::event::ToolEvent::Started {
                call_id: call.id.clone(),
            },
        )));
        let capability = tool.spec().capability;
        let undoable = tool.spec().reversibility == kivo_core::tool::Reversibility::Undoable;
        let undoing = call.args.get("undo").is_some();
        self.in_use(capability, true);
        let (result, output) = if undoing {
            kivo_tools::registry::undo(tool, call, permit, &cancel).await
        } else {
            kivo_tools::execute(tool, call, permit, &cancel).await
        };
        self.in_use(capability, false);
        self.mark("t8ToolDone");
        // Someone else's content entered the turn: it is tainted from here on (SEC-13).
        if let Some(source) = output.as_ref().and_then(|o| o.source.as_deref()) {
            self.taint_with(source);
        }
        self.recorder.tool_result(&self.turn_key(), call, &result);
        if result.status.is_ok()
            && !undoing
            && let Some(running) = lock(&self.turn).as_mut()
        {
            running.calls.push((call.tool.clone(), call.args.clone()));
        }
        // "Where is the Export button?": the Island goes beside it (UX-39).
        if let Some(point) = output.as_ref().and_then(|o| {
            serde_json::from_value::<kivo_ipc::protocol::PointTarget>(o.data["point"].clone()).ok()
        }) {
            self.core
                .update_turn(|view| view.point = Some(point.clone()));
        }
        match (&result.status, &output) {
            (Ok(_), Some(output)) => {
                self.core.set_step(StepView {
                    id: call.id.clone(),
                    title: step_title.clone(),
                    status: StepStatus::Done,
                    detail: Some(output.say.clone()),
                });
                // It started a task (a reminder, a watcher, a plan): "Open in Tasks".
                if let Some(task) = output.data["task"].as_str() {
                    let task = task.to_owned();
                    self.core.update_turn(|view| view.task_id = Some(task));
                }
                // Undoable (not an undo itself): offer to take it back (UX-43).
                if undoable && call.args.get("undo").is_none() {
                    *lock(&self.last_undo) = Some(UndoEntry {
                        tool: call.tool.clone(),
                        data: output.data.clone(),
                        title: step_title.clone(),
                        at: Instant::now(),
                    });
                    let until = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(0))
                        + u64::try_from(UNDO_OFFER.as_millis()).unwrap_or(0);
                    self.core.update_turn(|view| {
                        view.undo = Some(kivo_ipc::protocol::UndoOffer {
                            title: step_title.clone(),
                            until,
                        });
                    });
                }
                // A command or an app CLI ran in a folder: KIVO works in that project (CONV-10).
                if matches!(call.tool.as_str(), "shell.run" | "apps.cli")
                    && let Some(folder) = call
                        .args
                        .get("cwd")
                        .or_else(|| call.args.get("path"))
                        .and_then(serde_json::Value::as_str)
                    && let Some(workspaces) = self.workspaces()
                {
                    workspaces.worked_in(std::path::Path::new(folder));
                }
                // An app was launched or closed: the window list changed.
                if call.tool.starts_with("apps.") {
                    let engine = Arc::clone(self);
                    tokio::task::spawn_blocking(move || engine.refresh_apps());
                }
            }
            (Err(error), _) => {
                self.core.set_step(StepView {
                    id: call.id.clone(),
                    title: step_title,
                    status: StepStatus::Failed,
                    detail: Some(error.message.clone()),
                });
            }
            (Ok(_), None) => {}
        }
        (result, output)
    }

    /// Says `text` and ends the turn once it has been spoken.
    async fn speak_and_finish(self: &Arc<Self>, text: &str) {
        self.core
            .update_turn(|view| view.answer = Some(text.to_owned()));
        self.recorder.answer(&self.turn_key(), text, "done");
        self.speak(text).await;
    }

    /// Whether this turn's replies are spoken (or only shown, with a soft cue).
    fn speak_replies(&self) -> bool {
        let config = self.core.config();
        let typed = lock(&self.turn)
            .as_ref()
            .is_some_and(|t| t.source == TurnSource::Typed);
        let capability_on = config
            .capabilities
            .enabled(kivo_core::Capability::SpeakResponses);
        // Over a fullscreen app or in Focus, KIVO uses sounds only (UX §2, UX-11).
        let quiet = self.core.turn_view().is_some_and(|v| v.quiet.is_some());
        // Voice-only use speaks typed replies too (Settings → Accessibility).
        capability_on
            && !quiet
            && (!typed || config.voice.speak_typed_replies || config.accessibility.voice_only)
    }

    /// Streams `text` through the voice; `SpeakDone` finishes the turn.
    async fn speak(self: &Arc<Self>, text: &str) {
        let speak_replies = self.speak_replies();
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
            text.clone_into(&mut running.reply);
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
        if lock(&self.previews).contains(&id) {
            self.speaker.speak(pcm, rate);
            return;
        }
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
    pub fn speech_done(self: &Arc<Self>, id: u64, error: Option<&str>, cancelled: bool) {
        if lock(&self.previews).remove(&id) {
            return;
        }
        let ours = lock(&self.turn)
            .as_ref()
            .is_some_and(|t| t.speaking == Some(id));
        if !ours {
            return;
        }
        if let Some(message) = error {
            tracing::warn!(message, "speaking failed");
        }
        if cancelled {
            return;
        }
        // A streamed answer: the next phrase, or wait for more (BRAIN-28).
        let (next, streaming, pending) = {
            let mut turn = lock(&self.turn);
            let Some(t) = turn.as_mut() else { return };
            let next = t.phrases.pop_front();
            if next.is_none() {
                t.speaking = None;
            }
            (next, t.streaming, t.pending.is_some())
        };
        if let Some(next) = next {
            let engine = Arc::clone(self);
            tokio::spawn(async move { engine.start_phrase(next).await });
            return;
        }
        if streaming {
            if pending {
                self.listen_for_answer();
            }
            return;
        }
        self.finish_speaking();
    }

    /// Waits for the speaker to drain, then ends the turn (or starts a follow-up). After a
    /// question the turn waits for the answer instead (by click, or by voice, CONV-26).
    fn finish_speaking(self: &Arc<Self>) {
        if lock(&self.turn)
            .as_ref()
            .is_some_and(|t| t.pending.is_some())
        {
            self.listen_for_answer();
            return;
        }
        let Some(id) = lock(&self.turn).as_ref().map(|t| t.id.clone()) else {
            return;
        };
        let answer = self.core.turn_view().and_then(|v| v.answer);
        let outcome = lock(&self.turn).as_ref().map_or("done", |t| t.outcome);
        self.finish_turn(outcome, answer.as_deref());
        tracing::debug!(turn = id, "turn complete");
    }

    /// Ends the turn: the session returns to Idle (or a follow-up window) and the Island clears.
    fn finish_turn(&self, outcome: &str, reply: Option<&str>) {
        let Some(running) = lock(&self.turn).take() else {
            return;
        };
        *lock(&self.previous) = self.core.turn_view();
        if !running.calls.is_empty() {
            *lock(&self.last_calls) = Some((running.transcript.clone(), running.calls.clone()));
        }
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
        // After a spoken answer, hands-free KIVO listens for a follow-up without the wake word
        // for the chosen time, with the Island's ring counting down (UX-45); otherwise the
        // session returns to Idle. The Island keeps the answer until then, or for a moment
        // (UX-10).
        let listener = self.listener().filter(|l| l.hands_free_on());
        let window = (spoken && follow_up_seconds > 0 && !running.guest)
            .then(|| listener.clone())
            .flatten()
            .map(|l| (l, Duration::from_secs(u64::from(follow_up_seconds))));
        match &window {
            Some((listener, length)) => {
                listener.follow_up(Some(Instant::now() + *length));
                self.core
                    .update_turn(|view| view.follow_up = Some(follow_up_seconds));
            }
            None => {
                if self.core.state().borrow().session == SessionState::FollowUp {
                    self.core.advance(SessionInput::FollowUpTimeout);
                }
            }
        }
        let core = Arc::clone(&self.core);
        let turn_id = running.id.clone();
        // Settings → Island "Hide after an answer" (UX-10); 0 keeps it until dismissed, but a
        // follow-up window still ends on time.
        let hide_after = self.core.config().overlay.hide_after_seconds;
        let stay = hide_after == 0 && window.is_none();
        let collapse = if hide_after == 0 {
            COLLAPSE_AFTER
        } else {
            Duration::from_secs(u64::from(hide_after))
        };
        let wait = window
            .as_ref()
            .map_or(collapse, |(_, length)| (*length).max(collapse));
        let listener = window.map(|(l, _)| l);
        if stay {
            return;
        }
        tokio::spawn(async move {
            tokio::time::sleep(wait).await;
            let same_turn = core.turn_view().is_some_and(|v| v.id == turn_id);
            if same_turn {
                core.clear_turn();
                if core.state().borrow().session == SessionState::FollowUp {
                    core.advance(SessionInput::FollowUpTimeout);
                }
                if let Some(listener) = listener {
                    listener.follow_up(None);
                }
            }
        });
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
        self.cancel_turn(reason, true);
    }

    /// Cancels the turn; `stop_listening: false` keeps an utterance the listener has just started
    /// (a wake word interrupting KIVO).
    fn cancel_turn(&self, reason: CancelReason, stop_listening: bool) {
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
        if stop_listening && let Some(listener) = self.listener() {
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

    /// The "thinking" cue (VOICE §6, off by default): a quiet tick when KIVO is still working on
    /// a request a second after the user stopped talking.
    fn thinking_cue_later(self: &Arc<Self>) {
        if !self.core.config().sounds.thinking_cue {
            return;
        }
        let Some(turn) = lock(&self.turn).as_ref().map(|t| t.id.clone()) else {
            return;
        };
        let engine = Arc::clone(self);
        tokio::spawn(async move {
            tokio::time::sleep(THINKING_CUE_AFTER).await;
            let same_turn = lock(&engine.turn).as_ref().is_some_and(|t| t.id == turn);
            let working = matches!(
                engine.core.state().borrow().session,
                SessionState::Thinking | SessionState::Acting
            );
            if same_turn && working {
                engine.speaker.cue(Cue::Thinking);
            }
        });
    }

    /// The Island appears on the monitor of the window the user is working in (UX-06).
    fn anchor_island(&self) {
        let Ok(Some(front)) = self.windows.foreground() else {
            return;
        };
        // The folder open in VS Code is a folder the user works in (CONV-10).
        if let Some(workspaces) = self.workspaces() {
            workspaces.front_window(&front.title, &front.app_id);
        }
        // Settings → Island "Which screen": the main screen always holds (0, 0) on Windows, so
        // anchoring there keeps the Island on it; the title-bar shift is for the window in front.
        if self.core.config().overlay.monitor == kivo_core::config::IslandMonitor::Main {
            self.core.update_turn(|view| {
                view.anchor = Some(kivo_ipc::protocol::ScreenPoint { x: 0, y: 0 });
                view.title_bar_bottom = None;
            });
            return;
        }
        let b = front.bounds;
        let anchor = kivo_ipc::protocol::ScreenPoint {
            x: b.x.saturating_add(i32::try_from(b.width / 2).unwrap_or(0)),
            y: b.y.saturating_add(i32::try_from(b.height / 2).unwrap_or(0)),
        };
        // UX-14: the window's title bar or tabs, so the listening Island can sit below them.
        let title_bar_bottom = self
            .windows
            .title_bar(front.id)
            .ok()
            .flatten()
            .map(|t| t.y.saturating_add(i32::try_from(t.height).unwrap_or(0)));
        self.core.update_turn(|view| {
            view.anchor = Some(anchor);
            view.title_bar_bottom = title_bar_bottom;
        });
    }

    /// How requests have been routed: the fast-path share and per-stage p95 (BRAIN-05).
    /// Turns the router's semantic stage on (or off) (BRAIN-03).
    pub fn set_semantic(&self, semantic: Option<kivo_intent::Semantic>) {
        lock(&self.router).set_semantic(semantic);
    }

    pub fn has_semantic(&self) -> bool {
        lock(&self.router).has_semantic()
    }

    pub fn router_metrics(&self) -> kivo_intent::RouterMetrics {
        lock(&self.router).metrics()
    }

    /// Stop everything (SEC-25): the turn, the voice and (from M5) tasks.
    pub fn stop_everything(&self) {
        self.cancel(CancelReason::EmergencyStop);
        // Background tasks go to Paused; only the user resumes them (SECURITY §8).
        if let Some(tasks) = self.tasks() {
            let paused = tasks.pause_all();
            if paused > 0 {
                tracing::warn!(paused, "tasks paused by the emergency stop");
            }
        }
        // An agent's prompt runs under the turn's token: cancelling it sent session/cancel.
        self.stop_controls();
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

    /// A chosen speech engine failed to load and another stands in for the session, or none
    /// could (VOICE-47). The user is told; the saved choice stays as it is.
    pub fn speech_fallback(&self, slot: InferSlot, from: &str, to: Option<&str>, error: &str) {
        let name = |id: &str| kivo_voice::engine(id).map_or_else(|| id.to_owned(), |e| e.name);
        let from_name = name(from);
        let (slot_name, message) = match (slot, to) {
            (InferSlot::Tts, _) => (
                "tts",
                text::tf("voice.fallbackTts", &[("from", &from_name)]),
            ),
            (InferSlot::Stt, Some(to)) => (
                "stt",
                text::tf(
                    "voice.fallbackStt",
                    &[("from", &from_name), ("to", &name(to))],
                ),
            ),
            (InferSlot::Stt, None) => {
                let message = text::tf("voice.fallbackNone", &[("from", &from_name)]);
                self.core.set_speech_status(SpeechStatus::Failed {
                    message: message.clone(),
                });
                ("stt", message)
            }
        };
        tracing::warn!(slot = slot_name, from, to, error, "speech engine fallback");
        self.core
            .bus
            .publish(kivo_core::Event::new(kivo_core::EventKind::System(
                kivo_core::event::SystemEvent::SpeechFallback {
                    slot: slot_name.to_owned(),
                    from: from.to_owned(),
                    to: to.map(str::to_owned),
                    message,
                },
            )));
    }

    /// The speech worker crashed (PLAN-12): it's in Activity with what KIVO did about it; when
    /// KIVO stops restarting it, speech shows as failed with a way to Diagnostics.
    pub fn worker_crashed(&self, crashes: u32, recovery: &crate::infer::Recovery) {
        use crate::infer::Recovery;
        let name = |id: &str| kivo_voice::engine(id).map_or_else(|| id.to_owned(), |e| e.name);
        let detail = match recovery {
            Recovery::Restart => text::t("recovery.restart"),
            Recovery::Cpu => text::t("recovery.cpu"),
            Recovery::Vulkan => text::t("recovery.vulkan"),
            Recovery::Fallback(id) => text::tf("recovery.fallback", &[("engine", &name(id))]),
            Recovery::Stopped => text::t("recovery.stopped"),
        };
        self.recorder
            .worker_crashed(crashes, recovery.key(), &detail);
        if *recovery == Recovery::Stopped {
            self.core.set_speech_status(SpeechStatus::Failed {
                message: detail.clone(),
            });
            if let Some(tasks) = self.tasks() {
                let notifier = Arc::clone(tasks.notifier());
                tokio::spawn(async move {
                    notifier
                        .announce(crate::notifier::Announcement {
                            source: "system".into(),
                            title: text::t("recovery.stoppedTitle"),
                            text: detail,
                            urgent: false,
                            tell_me: false,
                            task: None,
                        })
                        .await;
                });
            }
        }
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
    // In a realtime conversation the provider hears the user and handles interruptions itself
    // (BRAIN-33): the listener's own utterances are dropped; "Kivo stop" still works.
    if engine.live()
        && matches!(
            signal,
            VoiceSignal::Wake { .. }
                | VoiceSignal::BargeIn { .. }
                | VoiceSignal::FollowUpSpeech { .. }
                | VoiceSignal::SpeechStarted { .. }
                | VoiceSignal::EndOfSpeech { .. }
                | VoiceSignal::NoSpeech { .. }
        )
    {
        if let Some(listener) = engine.listener() {
            listener.cancel();
        }
        return;
    }
    match signal {
        VoiceSignal::SpeechStarted { .. } => engine.mark("t2SpeechStarted"),
        VoiceSignal::EndOfSpeech { utterance } => engine.end_of_speech(utterance).await,
        VoiceSignal::NoSpeech { utterance } => engine.nothing_heard(utterance),
        VoiceSignal::MicrophoneUnavailable => {
            engine.fail_turn(&text::t("turn.micBlocked"));
        }
        VoiceSignal::Wake {
            utterance,
            word,
            phrase,
            score,
            clip,
        } => engine.woke(utterance, &word, &phrase, score, clip).await,
        VoiceSignal::StopHeard { word } => engine.stop_heard(&word),
        VoiceSignal::BargeIn { utterance } => engine.barge_in(utterance).await,
        VoiceSignal::FollowUpSpeech { utterance } => engine.follow_up_heard(utterance).await,
        VoiceSignal::Heard { utterance, audio } => engine.heard(utterance, audio),
        VoiceSignal::Recorded { id, audio } => {
            if let Some(voice_id) = engine.voice_id() {
                voice_id.recorded(id, audio);
            }
        }
        VoiceSignal::HandsFreeFailed { message } => {
            tracing::warn!(message, "hands-free listening is off");
            engine
                .core
                .flash_error(&text::t("turn.handsFreeFailed"), COLLAPSE_AFTER);
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
            let state = serde_json::to_value(state)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default();
            engine.core.set_residency(&name, &state);
        }
        InferEvent::Fallback {
            slot,
            from,
            to,
            error,
        } => engine.speech_fallback(slot, &from, to.as_deref(), &error),
        InferEvent::Lost => engine.engine_lost(),
        InferEvent::Crashed { crashes, recovery } => engine.worker_crashed(crashes, &recovery),
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

/// Announcements outside a turn (UX-40): Windows' voice or KIVO's, and the notification earcon.
#[async_trait::async_trait]
impl crate::notifier::Voice for Engine {
    async fn say(&self, text: &str) {
        if !self
            .core
            .config()
            .capabilities
            .enabled(kivo_core::Capability::SpeakResponses)
        {
            return;
        }
        if lock(&self.turn).is_some() {
            return;
        }
        if self.infer.wait_ready(Duration::from_millis(500)).await {
            let id = self.infer.next_utterance();
            lock(&self.previews).insert(id);
            let voice = self.core.config().voice.tts_voice;
            let voice = (!voice.is_empty()).then_some(voice);
            if self.infer.speak(id, text, voice.as_deref()).await.is_err() {
                lock(&self.previews).remove(&id);
                self.say_in_process(text);
            }
        } else {
            self.say_in_process(text);
        }
    }

    fn earcon(&self) {
        self.speaker.cue(Cue::Notification);
    }
}
