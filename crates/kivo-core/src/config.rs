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
            sounds: Sounds::default(),
            permissions: Permissions::default(),
            capabilities: crate::capability::CapabilitySettings::default(),
            privacy: Privacy::default(),
            memory: Memory::default(),
            companion: Companion::default(),
            performance: Performance::default(),
            brains: Brains::default(),
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
    /// The one-time "KIVO is still running" notice after the first close has been shown (UX §1).
    pub first_close_seen: bool,
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
            first_close_seen: false,
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
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            push_to_talk: vec!["Ctrl".into(), "Space".into()],
            toggle_mode: false,
            auto_end_on_silence: true,
            type_to_kivo: vec!["Ctrl".into(), "Shift".into(), "Space".into()],
            input_device: None,
            output_device: None,
            speaker_mode: SpeakerMode::Off,
            follow_up_seconds: 8,
            stt_engine: String::new(),
            tts_engine: "system".into(),
            tts_voice: String::new(),
            speak_typed_replies: false,
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
    pub wake_glow: bool,
    pub in_fullscreen: FullscreenBehavior,
    /// Motion: follow Windows "Animation effects", or always reduced.
    pub reduce_motion: bool,
}

impl Default for Overlay {
    fn default() -> Self {
        Self {
            style: OverlayStyle::PillAndCard,
            position: OverlayPosition::TopCenter,
            wake_glow: false,
            in_fullscreen: FullscreenBehavior::Hide,
            reduce_motion: false,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverlayPosition {
    TopCenter,
    BottomCenter,
    RememberDrag,
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
}

impl Default for Sounds {
    fn default() -> Self {
        Self {
            enabled: true,
            set: SoundSet::Soft,
            volume: 70,
            thinking_cue: false,
        }
    }
}

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
}

impl Default for Permissions {
    fn default() -> Self {
        Self {
            mode: PermissionMode::Auto,
            emergency_stop: vec!["Ctrl".into(), "Alt".into(), "Shift".into(), "Esc".into()],
            blocked_apps: Vec::new(),
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
}

impl Default for Privacy {
    fn default() -> Self {
        Self {
            mode: PrivacyMode::Cloud,
            retention_days: 30,
            debug_transcripts: false,
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
}

impl Default for Memory {
    fn default() -> Self {
        Self {
            capture: CaptureMode::Suggest,
            workspace_notes: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CaptureMode {
    OnlyWhenAsked,
    Suggest,
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
}

impl Default for Performance {
    fn default() -> Self {
        Self {
            profile: PerformanceProfile::Auto,
            stt_warm_minutes: 10,
            tts_warm_minutes: 10,
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

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Brains {}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Tools {}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Integrations {}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Automation {}

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
