//! The wire protocol (ARCHITECTURE §3): JSON-RPC 2.0 messages in length-prefixed frames.
//!
//! A connection starts with `hello` (carrying the session token and protocol version). The
//! reply is a full `StateSnapshot`; after that the runtime pushes `event` notifications, and a
//! fresh `snapshot` notification whenever the client may have missed events.

use kivo_core::config::PermissionMode;
use kivo_core::event::{StepStatus, TurnSource};
use kivo_core::tool::ConfirmSpec;
use kivo_core::{Event, SessionState};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Bumped on breaking changes (major) and additions (minor). Clients with another major
/// version are refused with a clear error (for example after a partial update).
/// 1.1 added the permission mode and "Island hidden" to the snapshot, and `permissions.setMode`.
/// 1.2 added the live turn (transcript, steps, answer, confirmation), capabilities, Activity,
/// speech models and the requests the Island and the Control Center make.
/// 1.3 added brains: the turn's brain chip, and the Brains, Chat, Usage and preference requests.
pub const PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion { major: 1, minor: 3 };

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolVersion {
    pub major: u16,
    pub minor: u16,
}

pub mod method {
    /// Client → runtime, first message: authenticate and get the initial state.
    pub const HELLO: &str = "hello";
    /// Client → runtime: a liveness check.
    pub const PING: &str = "ping";
    /// Client → runtime: the current state snapshot.
    pub const STATE: &str = "state.get";
    /// Runtime → client notification: one event from the bus.
    pub const EVENT: &str = "event";
    /// Runtime → client notification: a full snapshot (on every state change, and after events
    /// were missed).
    pub const SNAPSHOT: &str = "snapshot";
    /// Client → runtime: stop listening (the mic is released); only from Ready or a follow-up.
    pub const SESSION_PAUSE: &str = "session.pause";
    /// Client → runtime: listen again after a pause.
    pub const SESSION_RESUME: &str = "session.resume";
    /// Client → runtime: quit KIVO (UX §1). The runtime announces `ShuttingDown`, then stops.
    pub const RUNTIME_QUIT: &str = "runtime.quit";
    /// Runtime → client notification: the microphone level, `{ "level": 0.0–1.0 }`, about 30 times
    /// a second while listening (ARCHITECTURE §3).
    pub const LEVELS: &str = "levels";
    /// Client → runtime: switch the permission mode (`{ "mode": "plan" }`). Only the user does
    /// this, from the UI or the hotkey, never a tool or voice alone (SECURITY §1.1).
    pub const PERMISSIONS_SET_MODE: &str = "permissions.setMode";
    /// Client → runtime: start listening, as the Talk button does (`{}`).
    pub const SESSION_TALK: &str = "session.talk";
    /// Client → runtime: a typed request (`{ "text": "open chrome" }`, UX §8).
    pub const SESSION_SAY: &str = "session.say";
    /// Client → runtime: stop the current turn (Esc, the Island's Stop button).
    pub const SESSION_CANCEL: &str = "session.cancel";
    /// Client → runtime: stop everything (SEC-25).
    pub const SESSION_STOP_ALL: &str = "session.stopEverything";
    /// Takes back the last change (UX-43): the Island's Undo, the toast, "Kivo, undo that".
    pub const SESSION_UNDO: &str = "session.undo";
    /// The Island was dragged (the app reports where, UX-13): remembered for that monitor.
    pub const ISLAND_MOVED: &str = "island.moved";
    /// Client → runtime: answer a confirmation
    /// (`{ "callId": …, "answer": "allow" | "deny", "always": bool }`, SEC-10).
    pub const PERMISSIONS_ANSWER: &str = "permissions.answer";
    /// Client → runtime: the capability toggles (CAPABILITIES §1).
    pub const CAPABILITIES_GET: &str = "capabilities.get";
    /// Client → runtime: turn a capability on or off (`{ "capability": …, "on": bool }`).
    pub const CAPABILITIES_SET: &str = "capabilities.set";
    /// Applies Minimal / Balanced / Power user (CAP-05); returns the list.
    pub const CAPABILITIES_PRESET: &str = "capabilities.preset";
    /// Whether KIVO's browser extension is connected, and how to install it (TOOL-24).
    pub const BROWSER_STATUS: &str = "browser.status";
    /// Client → runtime: a page of the Activity timeline (`{ "before": id?, "limit": n }`).
    pub const ACTIVITY_LIST: &str = "activity.list";
    /// Client → runtime: the speech models on this PC and what can be downloaded (DIST-12).
    pub const MODELS_LIST: &str = "models.list";
    /// Client → runtime: download a model (`{ "id": "moonshine-base-en" }`).
    pub const MODELS_INSTALL: &str = "models.install";
    /// Client → runtime: delete a downloaded model (`{ "id": … }`).
    pub const MODELS_REMOVE: &str = "models.remove";
    /// Client → runtime: pause a download (it resumes where it stopped) or cancel it (UX-61).
    pub const MODELS_PAUSE: &str = "models.pause";
    pub const MODELS_CANCEL: &str = "models.cancel";
    /// Client → runtime: use the speech engine that runs on this model, switched safely
    /// (VOICE-45; UX-61 "Set as default").
    pub const MODELS_SET_DEFAULT: &str = "models.setDefault";
    /// Client → runtime: the settings the Control Center edits.
    pub const SETTINGS_GET: &str = "settings.get";
    /// Client → runtime: change settings (merged into the current values).
    pub const SETTINGS_SET: &str = "settings.set";
    /// Client → runtime: the "always allow" grants in force (SEC-08).
    pub const PERMISSIONS_GRANTS: &str = "permissions.grants";
    /// Client → runtime: revoke one grant (`{ "id": 3 }`).
    pub const PERMISSIONS_REVOKE: &str = "permissions.revoke";
    /// Client → runtime: the audit log, newest first (`{ "limit": n }`, SECURITY §7).
    pub const AUDIT_LIST: &str = "audit.list";
    /// Client → runtime: the Control Center window was closed (`{}`). The reply says whether KIVO
    /// keeps running (`{ "keepRunning": true }`); the first time, the runtime shows the one-time
    /// notice (UX §1).
    pub const UI_WINDOW_CLOSED: &str = "ui.windowClosed";
    /// Runtime → client notification: a model download's progress.
    pub const MODEL_PROGRESS: &str = "models.progress";
    /// Wake words (VOICE §4): list, check a phrase, add or edit, delete, turn on/off or tune,
    /// hear it spoken, try it, record samples, tune from them, and a false-alarm test.
    pub const WAKE_LIST: &str = "wake.list";
    pub const WAKE_CHECK: &str = "wake.check";
    pub const WAKE_SAVE: &str = "wake.save";
    pub const WAKE_DELETE: &str = "wake.delete";
    pub const WAKE_SET: &str = "wake.set";
    pub const WAKE_HEAR: &str = "wake.hear";
    pub const WAKE_TRY: &str = "wake.try";
    pub const WAKE_SAMPLE: &str = "wake.sample";
    pub const WAKE_TUNE: &str = "wake.tune";
    pub const WAKE_FALSE_ALARMS: &str = "wake.falseAlarms";
    /// Your voice (VOICE §5): status, start (with consent), record a prompt, finish, cancel, and
    /// delete all voice data.
    pub const VOICE_ID_STATUS: &str = "voiceId.status";
    pub const VOICE_ID_START: &str = "voiceId.start";
    pub const VOICE_ID_RECORD: &str = "voiceId.record";
    pub const VOICE_ID_FINISH: &str = "voiceId.finish";
    pub const VOICE_ID_CANCEL: &str = "voiceId.cancel";
    pub const VOICE_ID_DELETE: &str = "voiceId.delete";
    /// Settings → Sounds: play a set's cues (VOICE-27).
    pub const SOUNDS_PREVIEW: &str = "sounds.preview";
    /// Choosing speech engines (VOICE §11): the registry, profiles and current choice; the
    /// recommendation; a safe switch (progress as `engineSwitch` events); a voice preview.
    pub const VOICE_ENGINES: &str = "voice.engines";
    pub const VOICE_RECOMMEND: &str = "voice.recommend";
    pub const VOICE_SWITCH: &str = "voice.switch";
    pub const VOICE_PREVIEW: &str = "voice.preview";
    /// Onboarding's microphone check (UX-33) and a recognizer's "Try sample" (UX-60).
    pub const VOICE_MIC_CHECK: &str = "voice.micCheck";
    /// Client → runtime: the microphones and speakers (UX-61).
    pub const VOICE_DEVICES: &str = "voice.devices";
    /// Client → runtime: the advanced view's details (VOICE-49).
    pub const VOICE_ADVANCED: &str = "voice.advanced";
    pub const VOICE_TRY_SAMPLE: &str = "voice.trySample";
    /// Brains (BRAINS §4–5, UX-22): what KIVO can connect, what is connected and found, and
    /// connecting (sign-in, write-only keys, local servers, CLI agents).
    pub const BRAINS_CATALOG: &str = "brains.catalog";
    pub const BRAINS_LIST: &str = "brains.list";
    pub const BRAINS_CHECK: &str = "brains.check";
    pub const BRAINS_CONNECT: &str = "brains.connect";
    pub const BRAINS_DISCONNECT: &str = "brains.disconnect";
    pub const BRAINS_SET_KEY: &str = "brains.setKey";
    pub const BRAINS_TEST_KEY: &str = "brains.testKey";
    pub const BRAINS_SIGN_IN: &str = "brains.signIn";
    pub const BRAINS_SET_DEFAULT: &str = "brains.setDefault";
    pub const BRAINS_SAVE_PROFILE: &str = "brains.saveProfile";
    pub const BRAINS_DELETE_PROFILE: &str = "brains.deleteProfile";
    pub const BRAINS_DISCOVERY: &str = "brains.discovery";
    pub const BRAINS_REFRESH: &str = "brains.refresh";
    pub const BRAINS_VIEWED: &str = "brains.viewed";
    pub const BRAINS_SET_WORKSPACE: &str = "brains.setWorkspace";
    /// Context (Settings → Context, CONV-30): the layers and their sizes.
    pub const BRAINS_CONTEXT: &str = "brains.context";
    /// Usage and cost (BRAINS §9): totals, limits, task caps, price overrides, CSV export.
    pub const USAGE_SUMMARY: &str = "usage.summary";
    pub const USAGE_SET_LIMITS: &str = "usage.setLimits";
    pub const USAGE_SET_CAPS: &str = "usage.setCaps";
    pub const USAGE_SET_PRICE: &str = "usage.setPrice";
    pub const USAGE_EXPORT: &str = "usage.export";
    /// Chat (UX-21, CONVERSATION §0–1): threads, messages, sending, compaction, search, and the
    /// "that's not what I meant" report (BRAIN-06).
    pub const CHAT_THREADS: &str = "chat.threads";
    pub const CHAT_THREAD: &str = "chat.thread";
    pub const CHAT_NEW: &str = "chat.new";
    pub const CHAT_UPDATE: &str = "chat.update";
    pub const CHAT_DELETE: &str = "chat.delete";
    pub const CHAT_SEND: &str = "chat.send";
    pub const CHAT_COMPACT: &str = "chat.compact";
    pub const CHAT_SEARCH: &str = "chat.search";
    pub const CHAT_MISROUTE: &str = "chat.misroute";
    /// The user's own words for recognition and transcript repair (VOICE-23).
    pub const VOICE_VOCABULARY: &str = "voice.vocabulary";
    pub const VOICE_ADD_WORD: &str = "voice.addWord";
    pub const VOICE_REMOVE_WORD: &str = "voice.removeWord";
    /// Stated preferences ("call me Sam", MEM-03).
    pub const MEMORY_PREFERENCES: &str = "memory.preferences";
    pub const MEMORY_SET_PREFERENCE: &str = "memory.setPreference";
    pub const MEMORY_DELETE_PREFERENCE: &str = "memory.deletePreference";

    // M5: tasks, routines, agents, workspaces and instructions.
    /// Tasks for the Tasks page and Home (UX-24): `{ finished: bool }` → `TaskView[]`.
    pub const TASKS_LIST: &str = "tasks.list";
    pub const TASKS_GET: &str = "tasks.get";
    pub const TASKS_CANCEL: &str = "tasks.cancel";
    pub const TASKS_PAUSE: &str = "tasks.pause";
    pub const TASKS_RESUME: &str = "tasks.resume";
    /// The user's answer to a task's question: `{ id, choice }`.
    pub const TASKS_ANSWER: &str = "tasks.answer";
    pub const TASKS_DELETE: &str = "tasks.delete";
    pub const TASKS_CLEAR: &str = "tasks.clearFinished";
    /// Routines (ROUTINES §4): list, save (with the grants shown), delete, run now, enable.
    pub const ROUTINES_LIST: &str = "routines.list";
    pub const ROUTINES_SAVE: &str = "routines.save";
    pub const ROUTINES_DELETE: &str = "routines.delete";
    pub const ROUTINES_RUN: &str = "routines.run";
    pub const ROUTINES_ENABLE: &str = "routines.enable";
    /// Phrase and hotkey collisions and the permissions a draft needs (ROUT-03, ROUT-07).
    pub const ROUTINES_CHECK: &str = "routines.check";
    /// The tools a step can use, with their JSON Schemas for the builder's forms (ROUT-09).
    pub const ROUTINES_TOOLS: &str = "routines.tools";
    /// The Agents page (UX-25): CLI agents, desktop AI apps, sessions.
    pub const AGENTS_OVERVIEW: &str = "agents.overview";
    /// Hands an agent session to a visible terminal (CONV-13).
    pub const AGENTS_OPEN_TERMINAL: &str = "agents.openInTerminal";
    /// Starts a CLI agent in a visible terminal: `{ agent, folder, mode }` (CONV-14), as a request
    /// of its own through the permission engine.
    pub const AGENTS_START: &str = "agents.start";
    /// Workspaces and instructions (CONV-09/10/11).
    pub const WORKSPACES_LIST: &str = "workspaces.list";
    pub const WORKSPACES_REMEMBER: &str = "workspaces.remember";
    pub const WORKSPACES_FORGET: &str = "workspaces.forget";
    pub const WORKSPACES_EXPORT_AGENTS: &str = "workspaces.exportAgentsMd";
    pub const INSTRUCTIONS_GET: &str = "instructions.get";
    pub const INSTRUCTIONS_SET: &str = "instructions.set";
    /// Bypass permissions, with its opt-in and expiry (SEC-03).
    pub const PERMISSIONS_BYPASS: &str = "permissions.bypass";
    /// The Draft card's Edit (CONV-15): `{ callId, text }`; Send and Cancel answer the card's
    /// decision (`permissions.answer`).
    pub const DRAFT_ANSWER: &str = "draft.answer";
    /// "What can I say?" (UX-44) for the app in front.
    pub const SESSION_HELP: &str = "session.help";
    /// "What did I miss?" (UX-40).
    pub const SESSION_MISSED: &str = "session.missed";
    /// The selection shortcut (UX-42): `{ action: "explain" | "rewrite" | "translate" }`.
    pub const SELECTION_ACTION: &str = "selection.action";
    /// Offers the Island makes outside a turn: `{ id, accept }` ("Remember … as a workspace?").
    pub const OFFER_ANSWER: &str = "offer.answer";
}

/// The name the desktop app gives in `hello`; the runtime supervises the client with this name.
pub const APP_CLIENT: &str = "kivo-app";

/// `hello` parameters.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hello {
    pub protocol_version: ProtocolVersion,
    /// Who is connecting, e.g. "kivo-app 0.1.0".
    pub client: String,
    /// The contents of the session token file.
    pub token: String,
}

/// `hello` result.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Welcome {
    pub protocol_version: ProtocolVersion,
    pub runtime_version: String,
    pub snapshot: StateSnapshot,
}

/// Everything a UI needs to render from scratch (ARCHITECTURE §3: full snapshot, then deltas).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateSnapshot {
    pub session: SessionState,
    /// How much KIVO may do without asking (SECURITY §1.1).
    pub mode: PermissionMode,
    /// "Hide Island for 1 hour" (UX §1): the Island shows nothing until this is cleared.
    pub island_hidden: bool,
    /// What KIVO is working on now: what it heard, what it is doing and what it will say
    /// (UX §2). `None` between turns.
    pub turn: Option<TurnView>,
    /// Whether speech recognition is ready, downloading or missing.
    pub speech: SpeechStatus,
    /// Another app owns the push-to-talk keys, so KIVO can't use them (VOICE-41). The Control
    /// Center offers a rebind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotkey_conflict: Option<String>,
    /// Sensitive capabilities in use right now (CAP-06): `screen` (eye), `input` (hand),
    /// `shell` (terminal). The tray and the Island show an indicator for each.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub in_use: Vec<String>,
    /// Where the Island goes (UX-13): the setting and the spots it was dragged to.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub island: IslandPlacement,
    /// Ongoing status the collapsed Island shows (UX-15): a timer, a download, an agent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<LiveActivity>>", optional))]
    pub activities: Vec<LiveActivity>,
    /// Tasks running or waiting now: the tray tooltip and Home count them (UX-56, UX-19).
    #[serde(default, skip_serializing_if = "is_zero")]
    #[cfg_attr(feature = "ts", ts(as = "Option<u32>", optional))]
    pub tasks_active: u32,
    /// Bypass permissions is on until then (epoch ms; `u64::MAX`: until turned off), SEC-03.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional, type = "number"))]
    pub bypass_until: Option<u64>,
    /// Something the Island offers outside a turn ("Remember kivo as a workspace?").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub offer: Option<Offer>,
    /// Text was selected in the app in front when the text box opened: the Island offers
    /// Explain · Rewrite · Translate (UX-42). The text itself stays in the runtime.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(as = "Option<bool>", optional))]
    pub has_selection: bool,
    /// Increases with every state change, so a client can tell whether its view is current.
    pub revision: u64,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// Ongoing status in the collapsed Island (UX-15).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveActivity {
    pub id: String,
    /// `timer`, `download`, `agent`, `media` or `task`.
    pub kind: String,
    pub title: String,
    pub detail: Option<String>,
    /// 0–1, when known.
    pub progress: Option<f32>,
    /// When it ends (epoch ms), for a timer's countdown.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub until: Option<u64>,
    pub task_id: Option<String>,
}

/// A question the Island asks outside a turn.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    pub id: String,
    /// `workspace`.
    pub kind: String,
    pub text: String,
    pub accept: String,
    pub decline: String,
}

/// The Island's placement setting and remembered spots (UX-13).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IslandPlacement {
    pub position: kivo_core::config::OverlayPosition,
    pub spots: Vec<kivo_core::config::IslandSpot>,
}

impl Default for IslandPlacement {
    fn default() -> Self {
        Self {
            position: kivo_core::config::OverlayPosition::TopCenter,
            spots: Vec::new(),
        }
    }
}

/// The turn the Island is showing (UX §2 card contents).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnView {
    pub id: String,
    pub source: TurnSource,
    /// What KIVO has heard so far, or the typed request.
    pub transcript: String,
    /// The transcript won't change any more.
    pub transcript_final: bool,
    /// What KIVO is doing, in order.
    pub steps: Vec<StepView>,
    /// What KIVO says back.
    pub answer: Option<String>,
    /// Why it couldn't be done, in plain words.
    pub error: Option<String>,
    /// A decision waiting for the user (SEC-10).
    pub confirm: Option<ConfirmSpec>,
    /// The app the action is aimed at, for the Island's leading icon (UX §8.1).
    pub target_app: Option<String>,
    /// That app's icon as a `data:image/png` URL, when Windows has one (UX-46).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_icon: Option<String>,
    /// A capability this request needed that is off (CAP-02): the Island offers to turn it on.
    pub capability_off: Option<kivo_core::Capability>,
    /// A fullscreen app or Focus is on: the Island hides or shrinks to a dot, and KIVO only
    /// uses sounds (UX §2, UX-11).
    pub quiet: Option<QuietIsland>,
    /// The centre of the window in front when the request began (physical pixels): the Island
    /// appears on that window's monitor (UX-06).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub anchor: Option<ScreenPoint>,
    /// The bottom edge (physical y) of the title bar or tab strip of the window in front, when it
    /// sits at the top of its monitor: while only listening, the Island moves below it (UX-14).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_bar_bottom: Option<i32>,
    /// Someone other than the enrolled owner is talking: a guest turn (UX-08, VOICE-22).
    #[serde(default)]
    pub guest: bool,
    /// Listening for a follow-up without the wake word for this many seconds from when it
    /// appears (UX-45: the Island's ring counts down).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub follow_up: Option<u8>,
    /// KIVO is listening for a spoken answer to the decision (CONV-26): the mic ring and the
    /// voice hints show.
    #[serde(default)]
    pub answering: bool,
    /// The user said "wait": the card stays, "Waiting for you", with no timeout (UX-08).
    #[serde(default)]
    pub waiting: bool,
    /// The brain answering and why (PLAN-17): the card's chip, with the reason on hover.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub brain: Option<BrainChip>,
    /// A line the card shows about what KIVO did with the user's data ("Sent a screenshot of
    /// VS Code to Claude", CAP-08).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The last change can be taken back (UX-43): the Island shows Undo with a ring until
    /// `until` (milliseconds since the Unix epoch, about 8 s).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub undo: Option<UndoOffer>,
    /// A prompt KIVO will type into another AI, waiting for "send" (CONV-15).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub draft: Option<DraftView>,
    /// "What can I say?" (UX-44): examples for the app in front, then general ones.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<String>>", optional))]
    pub help: Vec<String>,
    /// The task this turn started, for "Open in Tasks".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub task_id: Option<String>,
}

/// The Draft card (CONV-15): what KIVO will type, where, before it presses send.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftView {
    /// "Claude · terminal · K.I.V.O".
    pub target: String,
    pub text: String,
}

/// An Undo the Island offers (UX-43).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoOffer {
    /// What would be undone ("Move files to Archive").
    pub title: String,
    /// When the Island stops offering it (epoch ms); voice and the toast still work after.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub until: u64,
}

/// The card header's brain chip (UX-09): "Coding · Claude Code", the routing reason, and a cost
/// estimate when the user shows costs (BRAINS §9).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrainChip {
    /// The brain's name ("Claude Code").
    pub name: String,
    /// The profile's name ("Coding").
    pub profile: String,
    /// "Coding · Claude Code — because this looked like a coding task".
    pub reason: String,
    /// It runs on this PC.
    pub local: bool,
    /// Estimated dollars so far; `None` when hidden or unknown (always an estimate).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub cost: Option<f64>,
    /// A spending-limit warning ("80% of your daily limit").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub warning: Option<String>,
    /// Context used by this request, in tokens, and the budget (the context meter, UX-21).
    #[serde(default)]
    pub context_used: u32,
    #[serde(default)]
    pub context_budget: u32,
}

/// A point on the desktop, in physical pixels.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenPoint {
    pub x: i32,
    pub y: i32,
}

/// How the Island behaves over a fullscreen app or during Focus (Settings → Island).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum QuietIsland {
    Hidden,
    Tiny,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepView {
    /// The tool call's id.
    pub id: String,
    /// What it does, in the user's words ("Open Google Chrome").
    pub title: String,
    pub status: StepStatus,
    /// A short result or reason ("Volume 30%", "Chrome isn't installed").
    pub detail: Option<String>,
}

/// Whether KIVO can hear: the speech model is a download (DIST-12).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum SpeechStatus {
    Ready,
    /// The speech model is downloading.
    Downloading {
        percent: u8,
    },
    /// It isn't downloaded yet.
    #[default]
    Missing,
    /// The engine failed; the message is safe to show.
    Failed {
        message: String,
    },
}

/// One entry of the Activity timeline (UX-20).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityItem {
    pub id: i64,
    /// Unix milliseconds.
    pub ts: i64,
    pub turn_id: Option<String>,
    /// `transcript`, `tool`, `reply`, `setting`, `stop`.
    pub kind: String,
    pub title: String,
    pub detail: Option<String>,
    /// `done`, `failed`, `cancelled`, `denied`, `unhandled`.
    pub status: String,
}

/// One row of the audit log (SECURITY §7, Activity → Audit).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditItem {
    pub ts: i64,
    pub turn_id: Option<String>,
    pub tool: String,
    pub args_summary: String,
    pub risk: String,
    pub decision: String,
    pub confirmed_by: Option<String>,
    pub result: Option<String>,
    pub error: Option<String>,
}

/// An "always allow" grant (SEC-08).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrantItem {
    pub id: i64,
    pub tool: String,
    pub scope: Option<String>,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    /// A `*` pattern the call's arguments must match (a folder, a command prefix).
    pub pattern: Option<String>,
    /// Lasts only until KIVO restarts.
    pub session_only: bool,
}

/// A speech model KIVO can install (Voice → Models on this PC, DIST-13).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelItem {
    pub id: String,
    pub name: String,
    /// `stt`, `tts`, `vad`, `wake`, `embedding`.
    pub kind: String,
    pub license: String,
    pub attribution: String,
    pub source: String,
    pub languages: Vec<String>,
    /// Download size in bytes.
    pub size: u64,
    pub installed: bool,
    pub disk_bytes: u64,
    /// 0–100 while downloading.
    pub downloading: Option<u8>,
    /// Loaded or not right now (PLAN-02): `unloaded`, `warming`, `warm`, `active`, `idle`,
    /// `unloading`; `None` until the worker reports it.
    #[serde(default)]
    pub residency: Option<String>,
    /// `notInstalled`, `downloading`, `installing`, `paused`, `ready`, `updateAvailable` or
    /// `error` (UX-61).
    #[serde(default)]
    pub state: String,
    /// Why the last download failed, in plain words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub error: Option<String>,
    /// The speech engine KIVO uses now runs on this model.
    #[serde(default)]
    pub in_use: bool,
}

/// A speech engine in the registry, as the Voice page and onboarding show it (VOICE-42).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechEngineItem {
    pub id: String,
    pub name: String,
    /// `stt` or `tts`.
    pub slot: String,
    /// `recommended`, `lightweight`, `highAccuracy`, `multilingual`, `natural`, `expressive`.
    pub profiles: Vec<String>,
    /// `local`, `cloud` or `hybrid`.
    pub privacy: String,
    pub license: String,
    pub commercial_use: bool,
    /// BCP-47 primary tags; `*` for any.
    pub languages: Vec<String>,
    pub streaming: bool,
    /// `cpu`, `directMl`, `cuda`, `npu`.
    pub devices: Vec<String>,
    pub download_mb: u32,
    pub ram_mb: u32,
    /// The model it downloads, if any.
    pub model: Option<String>,
    /// Ready to use: its model is on this PC, or it needs none.
    pub ready: bool,
    /// It handles the primary language (VOICE-48).
    pub fits_language: bool,
    pub voices: Vec<VoiceItem>,
    /// KIVO's measurement on this PC; `None` reads "Not benchmarked by KIVO".
    pub measured: Option<MeasuredItem>,
}

/// A voice of a TTS engine.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceItem {
    pub id: String,
    pub name: String,
    /// `female`, `male` or empty.
    pub style: String,
    pub languages: Vec<String>,
}

/// KIVO's own benchmark of an engine on this PC.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasuredItem {
    pub real_time_factor: Option<f64>,
    pub latency_ms: Option<f64>,
    pub word_error_rate: Option<f64>,
    /// How well it heard the owner's enrollment recordings (VOICE-23).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub voice_word_error_rate: Option<f64>,
    /// Unix milliseconds.
    pub measured_at: i64,
}

/// One curated profile and the engine behind it for the primary language (VOICE-43).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileItem {
    pub slot: String,
    pub profile: String,
    /// `None`: "Not available yet", or none for this language (`otherLanguagesOnly`).
    pub engine: Option<String>,
    pub other_languages_only: bool,
}

/// Everything the speech choosers need (`voice.engines`).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechChoices {
    /// The primary language.
    pub language: String,
    pub engines: Vec<SpeechEngineItem>,
    pub profiles: Vec<ProfileItem>,
    /// The recognizer KIVO listens with, and the voice engine and voice it speaks with.
    pub stt: String,
    pub tts: String,
    pub tts_voice: String,
}

/// The recommendation for this PC (`voice.recommend`, VOICE-44).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendationItem {
    /// `low`, `mid` or `high`.
    pub tier: String,
    pub stt_engine: Option<String>,
    pub stt_fallback: Option<String>,
    pub tts_engine: String,
    pub tts_fallback: Option<String>,
    pub threads: u32,
    pub reason: String,
}

/// A capability toggle (CAPABILITIES §1).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityItem {
    pub capability: kivo_core::Capability,
    pub label: String,
    pub enabled: bool,
    pub default: bool,
    pub badges: Vec<kivo_core::capability::Badge>,
    /// When a tool of this capability last ran (epoch ms), for "used 3 min ago" (CAP-04).
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub last_used: Option<i64>,
}

/// A JSON-RPC 2.0 request id (numbers only; KIVO's clients never send strings).
pub type RequestId = u64;

/// One JSON-RPC message on the wire.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Message {
    Request(Request),
    Response(Response),
    Notification(Notification),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub jsonrpc: JsonRpcV2,
    pub id: RequestId,
    pub method: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub params: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notification {
    pub jsonrpc: JsonRpcV2,
    pub method: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub params: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub jsonrpc: JsonRpcV2,
    pub id: RequestId,
    #[serde(flatten)]
    pub outcome: Outcome,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Result(Value),
    Error(RpcError),
}

/// The literal `"2.0"`; anything else is rejected while parsing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct JsonRpcV2;

impl Serialize for JsonRpcV2 {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str("2.0")
    }
}

impl<'de> Deserialize<'de> for JsonRpcV2 {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = String::deserialize(d)?;
        if v == "2.0" {
            Ok(Self)
        } else {
            Err(serde::de::Error::custom("jsonrpc must be \"2.0\""))
        }
    }
}

/// A JSON-RPC error. `message` is safe to show the user.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{message} ({code})")]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

impl RpcError {
    pub const PARSE: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL: i32 = -32603;
    /// Wrong or missing session token.
    pub const UNAUTHORIZED: i32 = -32001;
    /// Different protocol major version.
    pub const INCOMPATIBLE: i32 = -32002;
    /// The request is valid but does not fit the current state; the message says why.
    pub const REFUSED: i32 = -32010;
    /// A speech engine in `kivo-infer` failed; the message is safe to show and speak.
    pub const ENGINE: i32 = -32020;

    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn method_not_found(method: &str) -> Self {
        Self::new(Self::METHOD_NOT_FOUND, format!("unknown method {method}"))
    }

    pub fn invalid_params(detail: impl std::fmt::Display) -> Self {
        Self::new(
            Self::INVALID_PARAMS,
            format!("invalid parameters: {detail}"),
        )
    }
}

impl Request {
    pub fn new(id: RequestId, method: &str, params: Value) -> Self {
        Self {
            jsonrpc: JsonRpcV2,
            id,
            method: method.to_owned(),
            params,
        }
    }
}

impl Notification {
    pub fn new(method: &str, params: Value) -> Self {
        Self {
            jsonrpc: JsonRpcV2,
            method: method.to_owned(),
            params,
        }
    }

    /// An `event` notification carrying one bus event.
    pub fn event(event: &Event) -> Result<Self, serde_json::Error> {
        Ok(Self::new(method::EVENT, serde_json::to_value(event)?))
    }
}

impl Response {
    pub fn ok(id: RequestId, result: Value) -> Self {
        Self {
            jsonrpc: JsonRpcV2,
            id,
            outcome: Outcome::Result(result),
        }
    }

    pub fn err(id: RequestId, error: RpcError) -> Self {
        Self {
            jsonrpc: JsonRpcV2,
            id,
            outcome: Outcome::Error(error),
        }
    }
}

/// A task as the Tasks page, Home and the tray show it (UX-24).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskView {
    pub id: String,
    pub title: String,
    pub kind: kivo_core::task::TaskKind,
    /// Who it's for or who made it: "you", a routine's name.
    pub owner: String,
    pub status: kivo_core::task::TaskStatus,
    pub steps: Vec<TaskStepView>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub updated_at: i64,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub finished_at: Option<i64>,
    pub result: Option<String>,
    pub error: Option<String>,
    /// Kept instead of the steps once the task is 90 days old (MEM-02).
    pub summary: Option<String>,
    /// A step calls a brain (it costs, ROUT-08).
    pub uses_ai: bool,
    /// What the task waits for the user to decide.
    pub question: Option<TaskQuestion>,
    pub routine_id: Option<String>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStepView {
    pub id: String,
    pub title: String,
    /// `pending`, `running`, `waiting`, `needsYou`, `done`, `failed` or `skipped`.
    pub status: String,
    pub detail: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub started_at: Option<i64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub finished_at: Option<i64>,
    pub attempts: u32,
}

/// A decision a task waits for: a failed step with the "ask" policy, or an agent's request.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskQuestion {
    pub step: String,
    pub text: String,
    /// `retry`, `skip`, `stop`, `allow`, `always`, `deny`.
    pub choices: Vec<String>,
}

/// A routine for the Routines page (UX-26).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineView {
    pub routine: kivo_core::routine::Routine,
    pub contains_ai: bool,
    pub custom_command: bool,
    /// Every step is covered by what was granted when it was saved.
    pub granted: bool,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub last_run: Option<i64>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub updated_at: i64,
}

/// One permission a routine needs, as the save sheet lists it (ROUT-03).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrantLine {
    pub tool: String,
    /// "Open Slack", in plain words.
    pub title: String,
    pub risk: kivo_core::tool::Risk,
    pub capability: kivo_core::Capability,
    /// The capability is off: the step would be refused until it's turned on.
    pub capability_off: bool,
}

/// A phrase or hotkey that clashes with something (ROUT-07).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Collision {
    pub phrase: String,
    /// `command` (a built-in command), `routine`, `wakeWord` or `hotkey`.
    pub kind: String,
    /// What it clashes with ("volume up", "Work mode", "Hey Kivo").
    pub with: String,
    /// Blocks saving (an exact clash) or only warns (sounds alike).
    pub blocking: bool,
}

/// What `routines.check` finds for a draft.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineCheck {
    pub collisions: Vec<Collision>,
    pub grants: Vec<GrantLine>,
    /// Problems that stop it from saving (an empty name, an unknown tool).
    pub problems: Vec<String>,
}

/// A tool for the builder's catalog (ROUT-09).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolItem {
    pub id: String,
    pub title: String,
    pub description: String,
    /// Its arguments' JSON Schema: the builder generates the form from it.
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub params: serde_json::Value,
    pub risk: kivo_core::tool::Risk,
    pub capability: kivo_core::Capability,
}

/// A remembered workspace (CONV-10).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceItem {
    pub id: String,
    pub path: String,
    pub name: String,
    pub instructions: String,
    /// The project's own agent files found there (`CLAUDE.md`, `AGENTS.md`, `GEMINI.md`).
    pub agent_files: Vec<String>,
    pub preferred_agent: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub last_used: i64,
}

/// The Agents page (UX-25).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentsOverview {
    pub cli: Vec<AgentItem>,
    /// Claude Desktop, ChatGPT, Copilot found in the installed apps (DISC-06).
    pub desktop: Vec<DesktopAiItem>,
    pub sessions: Vec<AgentSessionItem>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentItem {
    pub id: String,
    pub name: String,
    pub installed: bool,
    pub signed_in: Option<bool>,
    pub version: Option<String>,
    /// It can be run in a visible terminal (CONV-14).
    pub terminal: bool,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopAiItem {
    pub id: String,
    pub name: String,
    /// The installed app's id (AUMID or Start-menu path) to open it.
    pub app_id: String,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionItem {
    pub id: String,
    pub agent: String,
    pub workspace: String,
    /// `acp` (KIVO runs it) or `terminal` (a visible terminal KIVO started).
    pub kind: String,
    pub running: bool,
    /// Can be continued in a terminal (the agent supports resuming).
    pub resumable: bool,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub last_used: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(v: Value) -> Result<Message, serde_json::Error> {
        serde_json::from_value(v)
    }

    #[test]
    fn parses_each_message_kind() {
        let req = parse(json!({"jsonrpc":"2.0","id":7,"method":"ping"})).unwrap();
        assert_eq!(req, Message::Request(Request::new(7, "ping", Value::Null)));
        let note = parse(json!({"jsonrpc":"2.0","method":"event","params":{"x":1}})).unwrap();
        assert!(matches!(note, Message::Notification(n) if n.method == "event"));
        let ok = parse(json!({"jsonrpc":"2.0","id":7,"result":"pong"})).unwrap();
        assert_eq!(ok, Message::Response(Response::ok(7, json!("pong"))));
        let err = parse(
            json!({"jsonrpc":"2.0","id":7,"error":{"code":-32601,"message":"unknown method x"}}),
        )
        .unwrap();
        assert_eq!(
            err,
            Message::Response(Response::err(7, RpcError::method_not_found("x")))
        );
    }

    #[test]
    fn rejects_malformed_messages() {
        for bad in [
            json!({"jsonrpc":"1.0","id":1,"method":"ping"}),
            json!({"id":1,"method":"ping"}),
            json!({"jsonrpc":"2.0","id":"a","method":"ping"}),
            json!({"jsonrpc":"2.0","id":1,"method":"ping","extra":true}),
            json!({"jsonrpc":"2.0","id":1}),
        ] {
            assert!(parse(bad.clone()).is_err(), "{bad}");
        }
    }

    #[test]
    fn serializes_responses_in_the_standard_shape() {
        let v = serde_json::to_value(Response::err(
            3,
            RpcError::new(RpcError::UNAUTHORIZED, "wrong token"),
        ))
        .unwrap();
        assert_eq!(
            v,
            json!({"jsonrpc":"2.0","id":3,"error":{"code":-32001,"message":"wrong token"}})
        );
    }
}
