//! Capabilities: what KIVO may do at all (CAPABILITIES §1–2). This sits above the permission
//! engine: a disabled capability's tools are not registered anywhere (not for brains, the fast
//! path, routines, MCP or remote clients), and a toggle never turns on by itself.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    MicListening,
    PushToTalk,
    SpeakResponses,
    AppsAndWindows,
    SystemControls,
    PowerActions,
    FilesRead,
    FilesModify,
    Clipboard,
    BrowserOpenLinks,
    BrowserPages,
    BrowserAutonomous,
    UiAutomation,
    ScreenAwareness,
    ComputerUse,
    Shell,
    BackgroundTasks,
    Routines,
    Memory,
    CloudBrains,
    RealtimeVoice,
    CliAgents,
    McpServers,
    Integrations,
    SpeakerRecognition,
    RemoteAccess,
    Notifications,
}

/// The cost/privacy badges (CAPABILITIES §1).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Badge {
    Local,
    Cloud,
    Costly,
    Sensitive,
}

impl Capability {
    pub const ALL: [Capability; 27] = [
        Self::MicListening,
        Self::PushToTalk,
        Self::SpeakResponses,
        Self::AppsAndWindows,
        Self::SystemControls,
        Self::PowerActions,
        Self::FilesRead,
        Self::FilesModify,
        Self::Clipboard,
        Self::BrowserOpenLinks,
        Self::BrowserPages,
        Self::BrowserAutonomous,
        Self::UiAutomation,
        Self::ScreenAwareness,
        Self::ComputerUse,
        Self::Shell,
        Self::BackgroundTasks,
        Self::Routines,
        Self::Memory,
        Self::CloudBrains,
        Self::RealtimeVoice,
        Self::CliAgents,
        Self::McpServers,
        Self::Integrations,
        Self::SpeakerRecognition,
        Self::RemoteAccess,
        Self::Notifications,
    ];

    /// The shipped default (CAPABILITIES §1). "Off until …" capabilities start off; setup turns
    /// them on with the user's consent.
    pub fn default_enabled(self) -> bool {
        !matches!(
            self,
            Self::MicListening
                | Self::Clipboard
                | Self::BrowserPages
                | Self::BrowserAutonomous
                | Self::ScreenAwareness
                | Self::ComputerUse
                | Self::Shell
                | Self::RealtimeVoice
                | Self::CliAgents
                | Self::McpServers
                | Self::Integrations
                | Self::SpeakerRecognition
                | Self::RemoteAccess
        )
    }

    /// The user-facing name, as in "Screen awareness is off", in the current language.
    pub fn label(self) -> String {
        crate::text::t(&format!("capability.{}", crate::text::key_of(&self)))
    }

    pub fn badges(self) -> &'static [Badge] {
        use Badge::{Cloud, Costly, Local, Sensitive};
        match self {
            Self::MicListening
            | Self::PushToTalk
            | Self::AppsAndWindows
            | Self::SystemControls
            | Self::BrowserOpenLinks
            | Self::UiAutomation
            | Self::BackgroundTasks
            | Self::Routines
            | Self::Notifications => &[Local],
            Self::SpeakResponses => &[Local, Cloud],
            Self::PowerActions
            | Self::FilesRead
            | Self::FilesModify
            | Self::Clipboard
            | Self::BrowserPages
            | Self::ScreenAwareness
            | Self::Shell
            | Self::Memory
            | Self::SpeakerRecognition
            | Self::RemoteAccess => &[Sensitive],
            Self::BrowserAutonomous => &[Cloud],
            Self::ComputerUse => &[Costly, Cloud, Sensitive],
            Self::CloudBrains | Self::RealtimeVoice | Self::CliAgents => &[Cloud, Costly],
            Self::Integrations => &[Cloud, Sensitive],
            Self::McpServers => &[],
        }
    }
}

/// The user's toggles: only the ones changed from the default are stored.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CapabilitySettings(pub BTreeMap<Capability, bool>);

impl CapabilitySettings {
    pub fn enabled(&self, capability: Capability) -> bool {
        self.0
            .get(&capability)
            .copied()
            .unwrap_or_else(|| capability.default_enabled())
    }

    /// Sets a toggle; returns true when it changed. Values equal to the default are not stored.
    pub fn set(&mut self, capability: Capability, on: bool) -> bool {
        let changed = self.enabled(capability) != on;
        if on == capability.default_enabled() {
            self.0.remove(&capability);
        } else {
            self.0.insert(capability, on);
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_the_catalogue() {
        let s = CapabilitySettings::default();
        assert!(s.enabled(Capability::PushToTalk));
        assert!(
            s.enabled(Capability::PowerActions),
            "on, but every use confirms"
        );
        assert!(
            !s.enabled(Capability::ComputerUse),
            "off by default (DECISIONS)"
        );
        assert!(!s.enabled(Capability::ScreenAwareness));
        assert_eq!(Capability::ALL.len(), 27);
        for c in Capability::ALL {
            assert!(
                !c.label().starts_with("capability."),
                "{c:?} has a name in the catalog"
            );
        }
        assert_eq!(Capability::AppsAndWindows.label(), "Apps & windows");
    }

    #[test]
    fn only_changes_from_the_default_are_stored() {
        let mut s = CapabilitySettings::default();
        assert!(s.set(Capability::SystemControls, false));
        assert!(!s.set(Capability::SystemControls, false), "no change");
        assert!(!s.enabled(Capability::SystemControls));
        assert!(s.set(Capability::SystemControls, true));
        assert!(s.0.is_empty());
    }
}
