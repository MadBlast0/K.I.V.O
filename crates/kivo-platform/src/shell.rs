//! Global hotkeys, the tray icon and notifications.

use crate::error::PlatformResult;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A key combination, e.g. Ctrl+Space. Keys use their display names ("Ctrl", "Alt", "Shift",
/// "Win", "Space", "Esc", "K").
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Chord(pub Vec<String>);

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.join("+"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HotkeyId(pub u32);

/// Global shortcuts. Presses and releases are delivered to the channel given at creation.
pub trait Hotkeys: Send + Sync {
    /// Fails with `PlatformError::Conflict` when another app owns the combination.
    fn register(&self, id: HotkeyId, chord: &Chord) -> PlatformResult<()>;
    fn unregister(&self, id: HotkeyId) -> PlatformResult<()>;
}

/// The tray icon's look (UX §1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrayIcon {
    Normal,
    Listening,
    Paused,
    Error,
    Updating,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum TrayMenuItem {
    Item {
        id: String,
        label: String,
        enabled: bool,
    },
    Check {
        id: String,
        label: String,
        checked: bool,
    },
    Submenu {
        label: String,
        items: Vec<TrayMenuItem>,
    },
    Separator,
}

/// Selections are delivered to the channel given at creation, by item id.
pub trait Tray: Send + Sync {
    fn set_icon(&self, icon: TrayIcon) -> PlatformResult<()>;
    fn set_tooltip(&self, text: &str) -> PlatformResult<()>;
    fn set_menu(&self, items: &[TrayMenuItem]) -> PlatformResult<()>;
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationAction {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub title: String,
    pub body: String,
    /// Buttons; a click is delivered to the channel given at creation (UX-57).
    pub actions: Vec<NotificationAction>,
    /// Show an inline reply field (UX-58).
    pub reply: bool,
}

pub trait Notifications: Send + Sync {
    fn show(&self, notification: &Notification) -> PlatformResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_display_like_the_ui_shows_them() {
        assert_eq!(
            Chord(vec![
                "Ctrl".into(),
                "Alt".into(),
                "Shift".into(),
                "Esc".into()
            ])
            .to_string(),
            "Ctrl+Alt+Shift+Esc"
        );
    }
}
