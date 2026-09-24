//! Windows implementations of the `kivo-platform` traits, plus the runtime's single-instance
//! guard. Empty on other platforms, so the workspace still builds there.

#![cfg(windows)]

mod audio;
mod autostart;
mod capabilities;
mod capture;
mod clipboard;
mod com;
mod control;
pub mod crash;
mod desktop;
mod devices;
mod displays;
pub mod environment;
mod files;
mod hello;
pub mod hotkeys;
pub mod icons;
mod input;
pub mod instance;
mod jobs;
pub mod native_messaging;
mod ocr;
mod power;
mod presence;
mod processes;
mod secrets;
mod speech;
mod system;
mod terminals;
pub mod toast;
pub mod tray;
mod trust;
mod uia;

pub use audio::WindowsAudio;
pub use autostart::WindowsAutostart;
pub use capabilities::detect as detect_capabilities;
pub use capture::WindowsScreen;
pub use clipboard::WindowsClipboard;
pub use control::{WindowsControl, open_document, open_uri};
pub use desktop::{WindowsApps, WindowsWindows};
pub use displays::WindowsDisplays;
pub use files::WindowsFiles;
pub use hello::WindowsHello;
pub use hotkeys::WindowsHotkeys;
pub use input::WindowsInput;
pub use jobs::WindowsCommands;
pub use ocr::WindowsOcr;
pub use power::WindowsPower;
pub use processes::WindowsProcesses;
pub use secrets::WindowsSecrets;
pub use speech::WindowsSpeech;
pub use system::{WindowsSystemInfo, utc_offset_minutes};
pub use terminals::WindowsTerminals;
pub use toast::{ToastAnswer, WindowsNotifications};
pub use tray::{TrayEvent, WindowsTray};
pub use trust::WindowsCodeTrust;
pub use uia::WindowsUiAutomation;

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
