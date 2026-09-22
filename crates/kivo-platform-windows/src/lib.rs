//! Windows implementations of the `kivo-platform` traits, plus the runtime's single-instance
//! guard. Empty on other platforms, so the workspace still builds there.

#![cfg(windows)]

mod audio;
mod autostart;
mod capabilities;
mod capture;
mod com;
mod control;
pub mod crash;
mod desktop;
pub mod hotkeys;
pub mod instance;
mod speech;
mod system;
pub mod toast;
pub mod tray;

pub use audio::WindowsAudio;
pub use autostart::WindowsAutostart;
pub use capabilities::detect as detect_capabilities;
pub use capture::WindowsScreen;
pub use control::WindowsControl;
pub use desktop::{WindowsApps, WindowsWindows};
pub use hotkeys::WindowsHotkeys;
pub use speech::WindowsSpeech;
pub use system::WindowsSystemInfo;
pub use toast::{ToastAnswer, WindowsNotifications};
pub use tray::{TrayEvent, WindowsTray};

/// Makes this process see physical pixels on every monitor, so window positions, sizes and
/// captures agree (without it Windows scales them on high-DPI screens). Call once at start.
pub fn dpi_aware() {
    use windows::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
    };
    // SAFETY: a process-wide setting; it fails harmlessly if already set.
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
}
