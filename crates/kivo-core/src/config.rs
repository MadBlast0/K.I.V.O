//! The settings schema (ARCHITECTURE §5, UX §5). `kivo-store` reads and writes it as TOML. Every
//! section has defaults, so a partial file is valid; the defaults here are the product defaults.

use serde::{Deserialize, Serialize};

/// Bumped whenever the shape changes; `kivo-store` migrates older files forward.
pub const CONFIG_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct KivoConfig {
    pub schema_version: u32,
    pub general: General,
    pub voice: Voice,
    pub overlay: Overlay,
    /// Theme, accent, text size, motion and transparency (UX §5, DECISIONS "Appearance settings").
    pub appearance: Appearance,
    /// Screen-reader announcements, captions, larger Island text, voice-only use (UX §10).
    pub accessibility: Accessibility,
    pub sounds: Sounds,
    pub permissions: Permissions,
    /// Capability toggles changed from their defaults (CAPABILITIES §1).
    pub capabilities: crate::capability::CapabilitySettings,
    pub privacy: Privacy,
    pub memory: Memory,
    pub companion: Companion,
    pub performance: Performance,
    /// Providers and profiles (filled in by M3, BRAINS §5).
    pub brains: Brains,
    /// What a brain starts with, layer by layer (CONVERSATION §8, Settings → Context).
    pub context: Context,
    /// Per-tool overrides (M4, TOOLS_AND_CONTROL §1).
    pub tools: Tools,
    /// Connected services (M6, INTEGRATIONS_AND_PLUGINS §0).
    pub integrations: Integrations,
    /// Routines and watchers (M5, ROUTINES).
    pub automation: Automation,
}

impl Default for KivoConfig {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            general: General::default(),
            voice: Voice::default(),
            overlay: Overlay::default(),
            appearance: Appearance::default(),
            accessibility: Accessibility::default(),
            sounds: Sounds::default(),
            permissions: Permissions::default(),
            capabilities: crate::capability::CapabilitySettings::default(),
            privacy: Privacy::default(),
            memory: Memory::default(),
            companion: Companion::default(),
            performance: Performance::default(),
            brains: Brains::default(),
            context: Context::default(),
            tools: Tools::default(),
            integrations: Integrations::default(),
            automation: Automation::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct General {
    /// "Open KIVO when Windows starts" (DECISIONS: startup wording).
    pub start_with_windows: bool,
    /// Closing the window keeps KIVO running in the tray (UX §1).
    pub keep_running_on_close: bool,
    pub tray_icon: bool,
    /// BCP-47 primary language; secondary languages follow in `languages`.
    pub language: String,
    pub languages: Vec<String>,
    pub low_memory_mode: bool,
    /// Dates, times and numbers as Windows formats them, or as the app's language does.
    pub format: FormatLocale,
    /// The one-time "KIVO is still running" notice after the first close has been shown (UX §1).
    pub first_close_seen: bool,
    /// Setup has been finished or skipped; until then the Control Center opens on it (UX-33).
    pub onboarded: bool,
}

impl Default for General {
    fn default() -> Self {
        Self {
            start_with_windows: false,
            keep_running_on_close: true,
            tray_icon: true,
            language: "en".into(),
            languages: Vec::new(),
            low_memory_mode: false,
            format: FormatLocale::Windows,
            first_close_seen: false,
            onboarded: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Voice {
    /// Keys, e.g. ["Ctrl", "Space"] (VOICE-41).
    pub push_to_talk: Vec<String>,
    /// Press once to start and once to stop, instead of holding.
    pub toggle_mode: bool,
    pub auto_end_on_silence: bool,
    /// Keys that open the card in text mode (UX §8).
    pub type_to_kivo: Vec<String>,
    /// Pauses or resumes listening (Settings → Shortcuts); empty for none.
    pub pause_shortcut: Vec<String>,
    /// Audio device ids; `None` follows the Windows default.
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub speaker_mode: SpeakerMode,
    /// Seconds to listen for a follow-up without the wake word; 0 turns it off (UX §8.1).
    pub follow_up_seconds: u8,
    /// Speech-to-text engine id; empty picks the best installed one for the language (VOICE-36).
    pub stt_engine: String,
    /// Text-to-speech engine and voice ids; an empty voice uses the engine's default.
    pub tts_engine: String,
    pub tts_voice: String,
    /// Speak answers to typed requests too (UX §8: off, text in, text out).
    pub speak_typed_replies: bool,
    /// Speaking speed in percent, 50–200 (UX-61).
    pub tts_speed: u16,
    /// Keep another installed recognizer ready in case the chosen one fails (VOICE-47, VOICE-49).
    pub stt_fallback: bool,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            push_to_talk: vec!["Ctrl".into(), "Space".into()],
            toggle_mode: false,
            auto_end_on_silence: true,
            type_to_kivo: vec!["Ctrl".into(), "Shift".into(), "Space".into()],
            pause_shortcut: Vec::new(),
            input_device: None,
            output_device: None,
            speaker_mode: SpeakerMode::Off,
            follow_up_seconds: 8,
            stt_engine: String::new(),
            tts_engine: "system".into(),
            tts_voice: String::new(),
            speak_typed_replies: false,
            tts_speed: 100,
            stt_fallback: true,
        }
    }
}

/// VOICE §5. Becomes `PreferOwner` once the user enrolls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpeakerMode {
    Off,
    PreferOwner,
    OwnerOnly,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Overlay {
    pub style: OverlayStyle,
    pub position: OverlayPosition,
    /// Where the Island was dragged to, per monitor (UX-13), used with Remember drag.
    pub spots: Vec<IslandSpot>,
    pub wake_glow: bool,
    pub in_fullscreen: FullscreenBehavior,
    /// Replaced by `appearance.motion` (M7); kept so settings files that have it still load.
    pub reduce_motion: bool,
    /// Compact, Standard or Large (Settings → Island).
    pub size: IslandSize,
    /// The monitor the Island shows on: where the user is working, or always the main one.
    pub monitor: IslandMonitor,
    /// The user's words as they speak.
    pub show_transcript: bool,
    /// Undo after a change (UX-43).
    pub show_undo: bool,
    /// "Say approve or cancel" under questions.
    pub voice_hints: bool,
    /// Seconds before the Island hides after an answer; 0 keeps it until dismissed (UX-10).
    pub hide_after_seconds: u8,
}

impl Default for Overlay {
    fn default() -> Self {
        Self {
            style: OverlayStyle::PillAndCard,
            position: OverlayPosition::TopCenter,
            spots: Vec::new(),
            wake_glow: false,
            in_fullscreen: FullscreenBehavior::Hide,
            reduce_motion: false,
            size: IslandSize::Standard,
            monitor: IslandMonitor::Active,
            show_transcript: true,
            show_undo: true,
            voice_hints: true,
            hide_after_seconds: 4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverlayStyle {
    PillAndCard,
    PillOnly,
    CardOnly,
    Off,
}

/// The Island's place on one monitor: its top-left, from the monitor's top-left, in physical
/// pixels (UX-13).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct IslandSpot {
    /// The monitor as Windows names it (`\\.\DISPLAY2`).
    pub monitor: String,
    pub x: i32,
    pub y: i32,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverlayPosition {
    TopCenter,
    BottomCenter,
    RememberDrag,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IslandSize {
    Compact,
    Standard,
    Large,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IslandMonitor {
    /// The monitor with the window in front.
    Active,
    /// The primary monitor.
    Main,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FormatLocale {
    /// As Windows' regional settings format them.
    Windows,
    /// As the app's language does.
    App,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Appearance {
    pub theme: Theme,
    /// A preset (`blue`, `violet`, `teal`, `green`, `amber`, `coral`, `graphite`) or `#RRGGBB`.
    pub accent: String,
    pub text_size: TextSize,
    pub motion: MotionPref,
    /// Mica behind the Control Center on Windows 11 (UX-32); solid when off or on Windows 10.
    pub transparency: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: Theme::Light,
            accent: "blue".into(),
            text_size: TextSize::Normal,
            motion: MotionPref::System,
            transparency: true,
        }
    }
}

/// The accent presets (DS-03); a custom accent is `#RRGGBB`.
pub const ACCENTS: &[&str] = &[
    "blue", "violet", "teal", "green", "amber", "coral", "graphite",
];

impl Appearance {
    /// A preset name or a `#RRGGBB` colour.
    pub fn accent_ok(&self) -> bool {
        ACCENTS.contains(&self.accent.as_str())
            || (self.accent.len() == 7
                && self.accent.starts_with('#')
                && self.accent[1..].chars().all(|c| c.is_ascii_hexdigit()))
    }
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    Light,
    Dark,
    System,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextSize {
    Normal,
    Large,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MotionPref {
    /// Follow Windows "Animation effects".
    System,
    Full,
    Reduced,
}

/// Settings → Context (CONV-31, CONVERSATION §8): which layers a brain starts with, the live fields
/// it sees, and when the conversation is compacted. Defaults just work.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Context {
    /// Layer 2: "About me" and stated preferences.
    pub about_me: bool,
    /// Layer 3: the workspace's notes and the project's agent files.
    pub workspace: bool,
    /// Layer 4: which live fields are sent (`local time`, `language`, `permission mode`,
    /// `active app`, `window title`).
    pub live_fields: Vec<String>,
    /// Layer 5: relevant memories.
    pub memories: bool,
    /// Layer 6: the skills index.
    pub skills: bool,
    /// Summarize older turns on its own (off: they drop out of the request; "Compact now" still
    /// works).
    pub auto_compact: bool,
    /// Compact when the conversation fills this share of the budget, in percent (50–100).
    pub compact_at: u8,
    /// "Start each conversation fresh": a new voice session never joins an earlier thread, so
    /// no summary carries over.
    pub fresh_start: bool,
}

/// The live fields a brain can be sent (CONVERSATION §8, layer 4).
pub const LIVE_FIELDS: &[&str] = &[
    "local time",
    "language",
    "permission mode",
    "active app",
    "window title",
];

impl Default for Context {
    fn default() -> Self {
        Self {
            about_me: true,
            workspace: true,
            live_fields: LIVE_FIELDS.iter().map(|f| (*f).to_owned()).collect(),
            memories: true,
            skills: true,
            auto_compact: true,
            compact_at: 100,
            fresh_start: false,
        }
    }
}

impl Context {
    /// Whether the live field `name` is sent.
    pub fn live(&self, name: &str) -> bool {
        self.live_fields.iter().any(|f| f == name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Accessibility {
    /// Narrator hears what KIVO is doing (UX-53).
    pub announcements: bool,
    /// What KIVO says is shown as text too.
    pub captions: bool,
    pub large_island_text: bool,
    /// KIVO says everything out loud, typed replies and the words for each button included.
    pub voice_only: bool,
    /// Warn when the Island is hidden and every sound is off (UX-54).
    pub warn_silent: bool,
}

impl Default for Accessibility {
    fn default() -> Self {
        Self {
            announcements: true,
            captions: true,
            large_island_text: false,
            voice_only: false,
            warn_silent: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FullscreenBehavior {
    Hide,
    TinyPill,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Sounds {
    pub enabled: bool,
    pub set: SoundSet,
    /// Relative to the system volume, 0–100.
    pub volume: u8,
    /// The "thinking" cue is off by default (VOICE §6).
    pub thinking_cue: bool,
    /// Cues switched off one by one in Settings → Sounds (VOICE-27).
    pub off: Vec<SoundCue>,
}

impl Default for Sounds {
    fn default() -> Self {
        Self {
            enabled: true,
            set: SoundSet::Soft,
            volume: 70,
            thinking_cue: false,
            off: Vec::new(),
        }
    }
}

/// Every sound KIVO makes (VOICE §6), in every set.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SoundCue {
    ListenStart,
    ListenStop,
    Done,
    Error,
    Thinking,
    Hangup,
    /// "This needs your answer" (CONVERSATION §7).
    Question,
    /// A decision was approved.
    Approved,
    Cancelled,
    /// A proactive message.
    Notification,
}

impl SoundCue {
    pub const ALL: [Self; 10] = [
        Self::ListenStart,
        Self::ListenStop,
        Self::Done,
        Self::Error,
        Self::Thinking,
        Self::Hangup,
        Self::Question,
        Self::Approved,
        Self::Cancelled,
        Self::Notification,
    ];
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SoundSet {
    Soft,
    Glass,
    Pulse,
    Wood,
    Minimal,
    Custom,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Permissions {
    pub mode: PermissionMode,
    pub emergency_stop: Vec<String>,
    /// Apps KIVO must never act on, by app id or name (SECURITY §1.1 hard limit).
    pub blocked_apps: Vec<String>,
    /// Moves to the next permission mode (Settings → Shortcuts); empty for none.
    pub mode_shortcut: Vec<String>,
}

impl Default for Permissions {
    fn default() -> Self {
        Self {
            mode: PermissionMode::Auto,
            emergency_stop: vec!["Ctrl".into(), "Alt".into(), "Shift".into(), "Esc".into()],
            blocked_apps: Vec::new(),
            mode_shortcut: vec!["Ctrl".into(), "Shift".into(), "M".into()],
        }
    }
}

/// SECURITY §1.1. Bypass auto-expires; loading the settings always turns it back into Auto.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PermissionMode {
    Ask,
    AcceptEdits,
    Plan,
    Auto,
    Bypass,
}

impl PermissionMode {
    /// The next mode for the Ctrl+Shift+M hotkey (SECURITY §1.1). Bypass is never reached this
    /// way: it needs its own opt-in (SEC-03).
    pub fn next(self) -> Self {
        match self {
            Self::Ask => Self::AcceptEdits,
            Self::AcceptEdits => Self::Plan,
            Self::Plan => Self::Auto,
            Self::Auto | Self::Bypass => Self::Ask,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Privacy {
    pub mode: PrivacyMode,
    /// Days to keep conversations; 0 means don't keep them.
    pub retention_days: u32,
    /// Log transcript text (off: transcripts never reach the log files).
    pub debug_transcripts: bool,
    /// Folders whose files only a brain on this PC may see (SECURITY §6, classified by source):
    /// KIVO reads them, and their content is `sensitive`. (Folders never touched at all are
    /// `tools.private-folders`.)
    pub sensitive_folders: Vec<String>,
    /// Words the user labels ("Project Falcon" → sensitive): text with them gets that class.
    pub labels: Vec<PrivacyLabel>,
    /// What Custom mode lets off the device, one switch each.
    pub custom: CustomPrivacy,
}

impl Default for Privacy {
    fn default() -> Self {
        Self {
            mode: PrivacyMode::Cloud,
            retention_days: 30,
            debug_transcripts: false,
            sensitive_folders: Vec::new(),
            labels: Vec::new(),
            custom: CustomPrivacy::default(),
        }
    }
}

/// A user label (SECURITY §6): text containing `text` (any case) is at least `class`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PrivacyLabel {
    pub text: String,
    pub class: LabelClass,
}

/// The classes a user label can give.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LabelClass {
    Personal,
    Sensitive,
    HighlySensitive,
}

/// Custom privacy (SEC-21): each kind of cloud processing on its own switch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct CustomPrivacy {
    /// Cloud brains answer questions.
    pub cloud_brains: bool,
    /// Personal details (contacts, addresses) may go to them; off, those requests stay local.
    pub personal_to_cloud: bool,
    /// Cloud speech engines hear and speak.
    pub cloud_speech: bool,
    /// Cloud brains may be shown screenshots.
    pub cloud_vision: bool,
}

impl Default for CustomPrivacy {
    fn default() -> Self {
        Self {
            cloud_brains: true,
            personal_to_cloud: false,
            cloud_speech: false,
            cloud_vision: false,
        }
    }
}

/// SECURITY §6.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PrivacyMode {
    Cloud,
    Local,
    StrictPrivate,
    Custom,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Memory {
    pub capture: CaptureMode,
    /// Automatic notes for workspaces KIVO works in (CONVERSATION §6; on by default).
    pub workspace_notes: bool,
    /// How much a session-log entry says.
    pub detail: NoteDetail,
    /// The tidy job merges near-duplicate notes.
    pub merge_duplicates: bool,
    /// The tidy job condenses workspace logs older than 14 days.
    pub condense_logs: bool,
    /// Sensitive notes may go to cloud AI when each one allows it (MEM-07). Off: never.
    pub sensitive_to_cloud: bool,
    /// The most memories put into one request (MEM-08).
    pub max_items: u32,
}

impl Default for Memory {
    fn default() -> Self {
        Self {
            capture: CaptureMode::Suggest,
            workspace_notes: true,
            detail: NoteDetail::Standard,
            merge_duplicates: true,
            condense_logs: true,
            sensitive_to_cloud: false,
            max_items: 5,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CaptureMode {
    OnlyWhenAsked,
    Suggest,
}

/// Memory → Note detail (CONVERSATION §6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NoteDetail {
    Brief,
    Standard,
    Detailed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Companion {
    pub style: CompanionStyle,
}

impl Default for Companion {
    fn default() -> Self {
        Self {
            style: CompanionStyle::Pill,
        }
    }
}

/// UX §6.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CompanionStyle {
    Pill,
    Orb,
    Character,
    Hidden,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Performance {
    pub profile: PerformanceProfile,
    /// Minutes to keep speech models loaded after use (VOICE §8).
    pub stt_warm_minutes: u16,
    pub tts_warm_minutes: u16,
    /// Processor threads speech models may use; 0 lets KIVO choose (VOICE-49).
    pub speech_threads: u8,
}

impl Default for Performance {
    fn default() -> Self {
        Self {
            profile: PerformanceProfile::Auto,
            stt_warm_minutes: 10,
            tts_warm_minutes: 10,
            speech_threads: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PerformanceProfile {
    Auto,
    Battery,
    Balanced,
    Performance,
    Gaming,
}

/// Brains (BRAINS §4–5, §9–10; CONVERSATION §1). Keys are never here: a connection names its key
/// by handle (`secret://kivo/<provider>/api-key`) and the value stays in Credential Manager
/// (SEC-17). User-made profiles, limits and price overrides live in the store.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Brains {
    /// The profile used when nothing else decides (BRAINS §5).
    pub default_profile: String,
    /// Calm, Friendly, Witty or Custom (BRAINS §10).
    pub persona: String,
    /// The user's own style, for the Custom persona.
    pub custom_persona: String,
    /// The brains the user connected. Detection only suggests; nothing is added by itself
    /// (DISC-03).
    pub connections: Vec<BrainConnection>,
    /// Keep the model's hidden reasoning in the turn log for debugging (BRAIN-09; off).
    pub keep_reasoning: bool,
    /// Refresh the price table weekly from LiteLLM (BRAIN-35).
    pub refresh_prices: bool,
    /// Show a live "≈ $0.12" in the card (BRAINS §9).
    pub show_cost: bool,
    /// A voice session ends after this many minutes of silence (CONV-01).
    pub voice_session_minutes: u16,
    /// Voice sessions within this many minutes join the same thread (CONV-01).
    pub thread_join_minutes: u16,
}

impl Default for Brains {
    fn default() -> Self {
        Self {
            default_profile: "default".into(),
            persona: "calm".into(),
            custom_persona: String::new(),
            connections: Vec::new(),
            keep_reasoning: false,
            refresh_prices: true,
            show_cost: false,
            voice_session_minutes: 2,
            thread_join_minutes: 30,
        }
    }
}

/// One connected brain.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct BrainConnection {
    /// A catalog id (`anthropic`, `ollama`, `claude-code`, …) or `custom-<name>`.
    pub id: String,
    /// The name shown, for custom connections.
    pub name: String,
    /// Overrides the catalog address (a local server on another port, a custom service).
    pub base_url: String,
    /// A custom service that runs on this PC (private) rather than in the cloud.
    pub local: bool,
    /// `secret://kivo/<provider>/<name>`, or empty when no key is needed.
    pub key: String,
    pub enabled: bool,
}

/// The per-capability controls (CAPABILITIES §1 "Controls when on", TOOLS_AND_CONTROL).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Tools {
    /// The last preset applied (CAP-05); `custom` once a toggle differs from it.
    pub preset: Preset,
    /// Per-app lists for UI Automation, screen awareness and computer use (CAP-07).
    pub ui_automation_apps: AppScope,
    pub screen_apps: AppScope,
    pub computer_use_apps: AppScope,
    /// Screen awareness may send a screenshot to a cloud vision brain (off: local OCR only).
    pub cloud_vision: bool,
    pub clipboard_read: bool,
    pub clipboard_write: bool,
    /// Shell: only read-only commands (Low risk) run; anything else is refused.
    pub shell_read_only: bool,
    /// Files: the folders file tools may use (empty: the user's folders).
    pub allowed_folders: Vec<String>,
    /// Folders never read or changed, and never searched.
    pub private_folders: Vec<String>,
    /// Browser pages: sites the extension tools may read and act on (empty: any) and never.
    pub allowed_sites: Vec<String>,
    pub blocked_sites: Vec<String>,
    /// Agents (by id: `claude-code`, `codex`) allowed to use KIVO's MCP server, and so the user's
    /// memory (CONV-24). None until the user allows one.
    pub share_with_agents: Vec<String>,
    /// Sensitive memories (personal notes, conversations) may go to those agents too.
    pub share_sensitive: bool,
    /// Local connectors the user switched off (Extensions → Connectors): KIVO doesn't use that
    /// app's commands, links or CLI (INT-03).
    pub connectors_off: Vec<String>,
}

impl Default for Tools {
    fn default() -> Self {
        Self {
            preset: Preset::Balanced,
            ui_automation_apps: AppScope::default(),
            screen_apps: AppScope::default(),
            computer_use_apps: AppScope::default(),
            cloud_vision: false,
            clipboard_read: true,
            clipboard_write: true,
            shell_read_only: true,
            allowed_folders: Vec::new(),
            private_folders: Vec::new(),
            allowed_sites: Vec::new(),
            blocked_sites: Vec::new(),
            share_with_agents: Vec::new(),
            share_sensitive: false,
            connectors_off: Vec::new(),
        }
    }
}

/// Capability presets (CAPABILITIES §2).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Preset {
    Minimal,
    Balanced,
    PowerUser,
    Custom,
}

/// Which apps a capability may act on. `allow` empty means any app not blocked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct AppScope {
    pub allow: Vec<String>,
    pub block: Vec<String>,
}

impl Default for AppScope {
    fn default() -> Self {
        Self {
            allow: Vec::new(),
            block: DEFAULT_BLOCKED_APPS
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
        }
    }
}

/// Never touched by default (CAP-07): password managers, banking apps and Windows Security.
/// Entries match an app's program name (`KeePassXC`), its id, or a `*pattern*`.
pub const DEFAULT_BLOCKED_APPS: &[&str] = &[
    "1Password",
    "Bitwarden",
    "KeePass",
    "KeePassXC",
    "LastPass",
    "Dashlane",
    "NordPass",
    "RoboForm",
    "Enpass",
    "Keeper",
    "ProtonPass",
    "*bank*",
    "*banking*",
    "SecHealthUI",
    "SecurityHealthHost",
    "SecurityHealthSystray",
];

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Integrations {}

/// Tasks, watchers and what KIVO says without being asked (UX §7, UX-15, UX-40).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Automation {
    /// Toasts only in these hours, `"22:00-07:00"` (local time); off when empty.
    pub quiet_hours: String,
    /// How each source is told: `tasks`, `reminders`, `watchers`, `routines`, `agents`. A source
    /// that isn't listed speaks when it's marked "tell me", else toasts.
    pub sources: std::collections::BTreeMap<String, AnnounceMode>,
    /// Read out what was missed when the user comes back.
    pub catch_up_on_return: bool,
    /// Ongoing status in the collapsed Island (UX-15); all off in fullscreen.
    pub live_activities: LiveActivities,
    /// "Say it out loud": always, only when the user is free (not in a call, a fullscreen app
    /// or Focus), or never (Settings → Notifications).
    pub speak: SpeakMode,
    /// Windows notifications; off, notices wait in the Control Center.
    pub toasts: bool,
    /// Notifications play the sound set's notification sound.
    pub notification_sound: bool,
    /// Windows Focus counts as quiet time.
    pub follow_focus: bool,
}

impl Default for Automation {
    fn default() -> Self {
        Self {
            quiet_hours: String::new(),
            sources: std::collections::BTreeMap::new(),
            catch_up_on_return: true,
            live_activities: LiveActivities::default(),
            speak: SpeakMode::WhenFree,
            toasts: true,
            notification_sound: true,
            follow_focus: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpeakMode {
    Always,
    WhenFree,
    Never,
}

impl Automation {
    /// The quiet hours as minutes after midnight, when set and well formed.
    pub fn quiet(&self) -> Option<(u32, u32)> {
        let (a, b) = self.quiet_hours.split_once('-')?;
        let minutes = |t: &str| {
            let (h, m) = t.trim().split_once(':')?;
            let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
            (h < 24 && m < 60).then_some(h * 60 + m)
        };
        Some((minutes(a)?, minutes(b)?))
    }
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AnnounceMode {
    Speak,
    Toast,
    Silent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct LiveActivities {
    pub media: bool,
    pub timer: bool,
    pub download: bool,
    pub agent: bool,
}

impl Default for LiveActivities {
    fn default() -> Self {
        Self {
            media: true,
            timer: true,
            download: true,
            agent: true,
        }
    }
}

/// A setting that is present but not allowed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{field}: {problem}")]
pub struct InvalidSetting {
    pub field: &'static str,
    pub problem: &'static str,
}

impl KivoConfig {
    /// Checks the rules the type system can't: ranges and non-empty shortcuts.
    pub fn validate(&self) -> Result<(), Vec<InvalidSetting>> {
        let mut problems = Vec::new();
        let mut check = |ok: bool, field, problem| {
            if !ok {
                problems.push(InvalidSetting { field, problem });
            }
        };
        check(self.sounds.volume <= 100, "sounds.volume", "must be 0–100");
        check(
            !self.voice.push_to_talk.is_empty(),
            "voice.push-to-talk",
            "needs at least one key",
        );
        check(
            !self.voice.type_to_kivo.is_empty(),
            "voice.type-to-kivo",
            "needs at least one key",
        );
        check(
            !self.permissions.emergency_stop.is_empty(),
            "permissions.emergency-stop",
            "needs at least one key",
        );
        check(
            self.voice.follow_up_seconds <= 60,
            "voice.follow-up-seconds",
            "must be 0–60",
        );
        check(
            !self.general.language.is_empty(),
            "general.language",
            "can't be empty",
        );
        check(
            self.appearance.accent_ok(),
            "appearance.accent",
            "must be a preset or #RRGGBB",
        );
        check(
            self.overlay.hide_after_seconds <= 60,
            "overlay.hide-after-seconds",
            "must be 0–60",
        );
        check(
            (50..=100).contains(&self.context.compact_at),
            "context.compact-at",
            "must be 50–100",
        );
        check(
            self.context
                .live_fields
                .iter()
                .all(|f| LIVE_FIELDS.contains(&f.as_str())),
            "context.live-fields",
            "unknown live field",
        );
        check(
            (1..=20).contains(&self.memory.max_items),
            "memory.max-items",
            "must be 1–20",
        );
        check(
            self.performance.speech_threads <= 16,
            "performance.speech-threads",
            "must be 0–16",
        );
        check(
            self.automation.quiet_hours.is_empty() || self.automation.quiet().is_some(),
            "automation.quiet-hours",
            "must look like 22:00-07:00",
        );
        check(
            (50..=200).contains(&self.voice.tts_speed),
            "voice.tts-speed",
            "must be 50–200",
        );
        check(
            self.brains.voice_session_minutes >= 1 && self.brains.voice_session_minutes <= 60,
            "brains.voice-session-minutes",
            "must be 1–60",
        );
        check(
            self.brains.thread_join_minutes <= 24 * 60,
            "brains.thread-join-minutes",
            "must be at most a day",
        );
        check(
            self.brains.connections.iter().all(|c| {
                !c.id.is_empty() && (c.key.is_empty() || c.key.starts_with("secret://kivo/"))
            }),
            "brains.connections",
            "each needs an id, and keys only as secret:// handles",
        );
        if problems.is_empty() {
            Ok(())
        } else {
            Err(problems)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_product_defaults() {
        let c = KivoConfig::default();
        assert_eq!(c.schema_version, CONFIG_SCHEMA_VERSION);
        assert!(!c.general.start_with_windows && c.general.keep_running_on_close);
        assert_eq!(c.voice.push_to_talk, ["Ctrl", "Space"]);
        assert_eq!(c.voice.follow_up_seconds, 8);
        assert_eq!(c.overlay.position, OverlayPosition::TopCenter);
        assert!(!c.overlay.wake_glow);
        assert_eq!(c.permissions.mode, PermissionMode::Auto);
        assert_eq!(
            c.permissions.emergency_stop,
            ["Ctrl", "Alt", "Shift", "Esc"]
        );
        assert_eq!(c.privacy.retention_days, 30);
        assert!(!c.privacy.debug_transcripts);
        assert_eq!(c.memory.capture, CaptureMode::Suggest);
        assert_eq!(c.companion.style, CompanionStyle::Pill);
        assert!(c.validate().is_ok());
    }

    #[test]
    fn validation_reports_every_problem() {
        let mut c = KivoConfig::default();
        c.sounds.volume = 150;
        c.voice.push_to_talk.clear();
        let problems = c.validate().unwrap_err();
        let fields: Vec<_> = problems.iter().map(|p| p.field).collect();
        assert_eq!(fields, ["sounds.volume", "voice.push-to-talk"]);
    }

    #[test]
    fn the_mode_hotkey_cycles_the_four_everyday_modes() {
        let mut mode = PermissionMode::Auto;
        let mut seen = Vec::new();
        for _ in 0..4 {
            mode = mode.next();
            seen.push(mode);
        }
        assert_eq!(
            seen,
            [
                PermissionMode::Ask,
                PermissionMode::AcceptEdits,
                PermissionMode::Plan,
                PermissionMode::Auto
            ]
        );
        assert_eq!(PermissionMode::Bypass.next(), PermissionMode::Ask);
    }
}
