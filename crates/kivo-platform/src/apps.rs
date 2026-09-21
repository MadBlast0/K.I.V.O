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
}

pub trait Apps: Send + Sync {
    fn installed(&self) -> PlatformResult<Vec<AppEntry>>;
    fn launch(&self, app: &AppEntry, args: &[String]) -> PlatformResult<()>;
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
    fn minimize(&self, id: WindowId) -> PlatformResult<()>;
    fn maximize(&self, id: WindowId) -> PlatformResult<()>;
    fn close(&self, id: WindowId) -> PlatformResult<()>;
}
