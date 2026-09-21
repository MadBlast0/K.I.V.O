//! Windows implementations of the `kivo-platform` traits, plus the runtime's single-instance
//! guard. Empty on other platforms, so the workspace still builds there.

#![cfg(windows)]

mod capabilities;
pub mod instance;
mod system;
pub mod tray;

pub use capabilities::detect as detect_capabilities;
pub use system::WindowsSystemInfo;
pub use tray::{TrayEvent, WindowsTray};
