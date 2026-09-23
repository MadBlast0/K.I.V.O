//! The M4 native controls (TOOLS_AND_CONTROL §3, §7, SECURITY §2): the clipboard, file
//! operations, running commands inside a Job Object, monitors and snapping, brightness and battery,
//! and user verification (Windows Hello).

use crate::error::PlatformResult;
use crate::types::{Rect, WindowId};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Text on the clipboard. What is read is `Untrusted` content (SECURITY §4).
pub trait Clipboard: Send + Sync {
    fn read_text(&self) -> PlatformResult<Option<String>>;
    fn write_text(&self, text: &str) -> PlatformResult<()>;
}

/// A file found by search.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHit {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    /// Seconds since the Unix epoch.
    pub modified: Option<u64>,
    pub size: Option<u64>,
}

/// The OS-specific file operations; plain create/rename/move/copy use the standard library.
pub trait FileOps: Send + Sync {
    /// The OS search index (Windows Search `SystemIndex`) for names containing `query` under
    /// `roots`. `Unsupported` when the index isn't available (callers walk the folders instead).
    fn search_index(
        &self,
        query: &str,
        roots: &[PathBuf],
        limit: usize,
    ) -> PlatformResult<Vec<FileHit>>;
    /// Moves `paths` to the Recycle Bin. Never deletes permanently.
    fn recycle(&self, paths: &[PathBuf]) -> PlatformResult<()>;
    /// Opens a file with its default app.
    fn open(&self, path: &Path) -> PlatformResult<()>;
    /// Shows the file selected in the file manager.
    fn reveal(&self, path: &Path) -> PlatformResult<()>;
    /// Folders that make an operation High risk (SECURITY §3): the OS, program files, the user
    /// profile root itself and app configuration.
    fn protected_roots(&self) -> Vec<PathBuf>;
    /// The user's everyday folders (Documents, Desktop, Downloads, …) for search.
    fn user_folders(&self) -> Vec<PathBuf>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShellKind {
    Pwsh,
    Cmd,
}

/// A command to run (TOOLS_AND_CONTROL §7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandSpec {
    pub command: String,
    pub shell: ShellKind,
    pub cwd: PathBuf,
    pub timeout: Duration,
    /// Per-call environment (secrets are injected here by handle, never in the command text).
    pub env: Vec<(String, String)>,
    /// Job limits.
    pub max_memory_mb: u64,
    /// Percent of total CPU, 1–100.
    pub max_cpu_percent: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandOutput {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    /// Output beyond the capture limit was dropped.
    pub truncated: bool,
    pub timed_out: bool,
    pub cancelled: bool,
}

/// Runs commands inside a Job Object (kill-on-close, memory and CPU limits), never elevated.
pub trait CommandRunner: Send + Sync {
    /// Runs until exit, timeout or `cancelled()` returns true (checked often).
    fn run(
        &self,
        spec: &CommandSpec,
        cancelled: &(dyn Fn() -> bool + Sync),
    ) -> PlatformResult<CommandOutput>;
    /// Kills every running command's whole process tree (the emergency stop, SEC-27). Returns
    /// how many jobs were killed.
    fn kill_all(&self) -> usize;
}

/// A monitor, in physical pixels.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Monitor {
    /// 1-based, left to right (what the user calls "monitor 2").
    pub index: u32,
    pub name: String,
    pub bounds: Rect,
    /// The area without the taskbar.
    pub work_area: Rect,
    pub primary: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Snap {
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Snap {
    /// Where a window snapped this way goes inside `area`.
    pub fn place(self, area: Rect) -> Rect {
        let half_w = area.width / 2;
        let half_h = area.height / 2;
        let right = area
            .x
            .saturating_add(i32::try_from(half_w).unwrap_or(i32::MAX));
        let lower = area
            .y
            .saturating_add(i32::try_from(half_h).unwrap_or(i32::MAX));
        let rect = |x, y, width, height| Rect {
            x,
            y,
            width,
            height,
        };
        match self {
            Self::Left => rect(area.x, area.y, half_w, area.height),
            Self::Right => rect(right, area.y, area.width - half_w, area.height),
            Self::Top => rect(area.x, area.y, area.width, half_h),
            Self::Bottom => rect(area.x, lower, area.width, area.height - half_h),
            Self::TopLeft => rect(area.x, area.y, half_w, half_h),
            Self::TopRight => rect(right, area.y, area.width - half_w, half_h),
            Self::BottomLeft => rect(area.x, lower, half_w, area.height - half_h),
            Self::BottomRight => rect(right, lower, area.width - half_w, area.height - half_h),
        }
    }
}

/// Monitors and window placement (TOOL-08).
pub trait Displays: Send + Sync {
    fn monitors(&self) -> PlatformResult<Vec<Monitor>>;
    /// The window's normal (restored) bounds.
    fn window_bounds(&self, id: WindowId) -> PlatformResult<Rect>;
    /// Restores the window if needed and places it at `bounds`.
    fn place_window(&self, id: WindowId, bounds: Rect) -> PlatformResult<()>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Battery {
    pub percent: u8,
    pub charging: bool,
    pub on_battery: bool,
    /// Seconds left on battery, when Windows knows.
    pub seconds_left: Option<u32>,
}

/// Brightness, battery and Focus (TOOL-13).
pub trait Power: Send + Sync {
    /// The built-in display's brightness, 0–100; `Unsupported` on desktops without one.
    fn brightness(&self) -> PlatformResult<u8>;
    fn set_brightness(&self, percent: u8) -> PlatformResult<()>;
    /// `None` on a machine without a battery.
    fn battery(&self) -> PlatformResult<Option<Battery>>;
    /// Windows Focus / Do not disturb is on.
    fn focus_on(&self) -> PlatformResult<bool>;
}

/// Confirming it's really the user (Windows Hello), for High-risk actions (SEC-11, CONV-29).
pub trait UserVerifier: Send + Sync {
    /// Whether a verifier (face, fingerprint or PIN) is set up.
    fn available(&self) -> bool;
    /// Shows the system prompt with `message`; `Ok(true)` only when verified.
    fn verify(&self, message: &str) -> PlatformResult<bool>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapping_splits_the_work_area() {
        let area = Rect {
            x: -1920,
            y: 0,
            width: 1920,
            height: 1041,
        };
        assert_eq!(
            Snap::Left.place(area),
            Rect {
                x: -1920,
                y: 0,
                width: 960,
                height: 1041
            }
        );
        assert_eq!(
            Snap::BottomRight.place(area),
            Rect {
                x: -960,
                y: 520,
                width: 960,
                height: 521
            }
        );
        let right = Snap::Right.place(area);
        assert_eq!(right.x + i32::try_from(right.width).unwrap(), 0);
    }
}
