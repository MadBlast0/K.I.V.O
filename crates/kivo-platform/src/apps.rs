//! Installed apps and top-level windows (TOOLS_AND_CONTROL §3).

use crate::error::PlatformResult;
use crate::types::{Rect, WindowId};
use serde::{Deserialize, Serialize};

/// An app the user can launch (Start-menu shortcut, packaged app, …).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppEntry {
    /// Stable identity: an AUMID for packaged apps, the target path otherwise.
    pub id: String,
    pub name: String,
    /// Other names the user might say ("VS Code" for "Visual Studio Code").
    pub aliases: Vec<String>,
    /// The program file, when known (matches running windows to their app).
    pub exe: Option<String>,
}

pub trait Apps: Send + Sync {
    fn installed(&self) -> PlatformResult<Vec<AppEntry>>;
    fn launch(&self, app: &AppEntry, args: &[String]) -> PlatformResult<()>;
    /// Asks every window of `app` to close (the app may still ask to save). Returns how many
    /// windows were asked; `NotFound` when the app isn't running.
    fn close(&self, app: &AppEntry) -> PlatformResult<usize>;
    /// Whether `app` has a window open.
    fn running(&self, app: &AppEntry) -> PlatformResult<bool>;
    /// The app's icon (a program path, shortcut or packaged-app id), for the Island (UX-46).
    fn icon(&self, _id: &str, _size: u32) -> Option<crate::screen::Image> {
        None
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfo {
    pub id: WindowId,
    pub title: String,
    /// The owning app's id (matches `AppEntry::id` when known).
    pub app_id: String,
    pub bounds: Rect,
    pub minimized: bool,
}

pub trait Windows: Send + Sync {
    /// Visible, uncloaked top-level windows, front to back.
    fn list(&self) -> PlatformResult<Vec<WindowInfo>>;
    fn foreground(&self) -> PlatformResult<Option<WindowInfo>>;
    fn focus(&self, id: WindowId) -> PlatformResult<()>;
    /// The window's title bar or tab strip, in screen coordinates, when the OS knows it
    /// (UX-14: the Island moves below it while listening).
    fn title_bar(&self, _id: WindowId) -> PlatformResult<Option<Rect>> {
        Ok(None)
    }
    fn minimize(&self, id: WindowId) -> PlatformResult<()>;
    fn maximize(&self, id: WindowId) -> PlatformResult<()>;
    /// Back to its normal size and place (undoes minimize and maximize).
    fn restore(&self, id: WindowId) -> PlatformResult<()>;
    fn close(&self, id: WindowId) -> PlatformResult<()>;
}
