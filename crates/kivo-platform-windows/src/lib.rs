//! Windows implementations of the `kivo-platform` traits, plus the runtime's single-instance
//! guard. Empty on other platforms, so the workspace still builds there.

#![cfg(windows)]

mod audio;
mod capabilities;
mod com;
pub mod hotkeys;
pub mod instance;
mod speech;
mod system;
pub mod tray;

pub use audio::WindowsAudio;
pub use capabilities::detect as detect_capabilities;
pub use hotkeys::WindowsHotkeys;
pub use speech::WindowsSpeech;
pub use system::WindowsSystemInfo;
pub use tray::{TrayEvent, WindowsTray};
