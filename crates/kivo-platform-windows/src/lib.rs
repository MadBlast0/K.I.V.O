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
mod secrets;
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
pub use secrets::WindowsSecrets;
pub use speech::WindowsSpeech;
pub use system::WindowsSystemInfo;
pub use toast::{ToastAnswer, WindowsNotifications};
pub use tray::{TrayEvent, WindowsTray};

/// EcoQoS for the calling thread (VOICE-03): Windows runs it on efficient cores at low clocks.
#[derive(Default)]
pub struct WindowsThreadQos;

impl kivo_platform::ThreadQos for WindowsThreadQos {
    fn efficiency_mode(&self, on: bool) {
        use windows::Win32::System::Threading::{
            GetCurrentThread, SetThreadInformation, THREAD_POWER_THROTTLING_CURRENT_VERSION,
            THREAD_POWER_THROTTLING_EXECUTION_SPEED, THREAD_POWER_THROTTLING_STATE,
            ThreadPowerThrottling,
        };
        let state = THREAD_POWER_THROTTLING_STATE {
            Version: THREAD_POWER_THROTTLING_CURRENT_VERSION,
            ControlMask: THREAD_POWER_THROTTLING_EXECUTION_SPEED,
            StateMask: if on {
                THREAD_POWER_THROTTLING_EXECUTION_SPEED
            } else {
                0
            },
        };
        // SAFETY: the struct outlives the call and its size is passed; a failure (older Windows)
        // just leaves the thread as it was.
        let _ = unsafe {
            SetThreadInformation(
                GetCurrentThread(),
                ThreadPowerThrottling,
                (&raw const state).cast(),
                u32::try_from(size_of::<THREAD_POWER_THROTTLING_STATE>()).unwrap_or(0),
            )
        };
    }
}

/// Makes this process see physical pixels on every monitor, so window positions, sizes and
/// captures agree (without it Windows scales them on high-DPI screens). Call once at start.
pub fn dpi_aware() {
    use windows::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
    };
    // SAFETY: a process-wide setting; it fails harmlessly if already set.
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
}
