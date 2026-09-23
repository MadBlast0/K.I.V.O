//! Mica behind the Control Center on Windows 11 (UX-32, DECISIONS "Appearance settings"):
//! Settings → Appearance "Transparency effects". Windows 10 has no Mica, so the window stays
//! solid there; the page is told which, so it paints its own background when Mica isn't behind
//! it.

use tauri::window::{Effect, EffectsBuilder};

/// Windows 11 starts at build 22000.
const WINDOWS_11: u32 = 22_000;

/// The real build number (`RtlGetVersion` isn't subject to the compatibility shims).
#[cfg(windows)]
fn os_build() -> u32 {
    use windows::Wdk::System::SystemServices::RtlGetVersion;
    use windows::Win32::System::SystemInformation::OSVERSIONINFOW;
    let mut info = OSVERSIONINFOW {
        dwOSVersionInfoSize: u32::try_from(size_of::<OSVERSIONINFOW>()).unwrap_or(0),
        ..Default::default()
    };
    // SAFETY: `info` is a properly sized, writable OSVERSIONINFOW with its size field set.
    let status = unsafe { RtlGetVersion(&raw mut info) };
    if status.is_ok() {
        info.dwBuildNumber
    } else {
        0
    }
}

#[cfg(not(windows))]
fn os_build() -> u32 {
    0
}

/// Whether Mica can be used: transparency is on and this is Windows 11.
pub fn mica_wanted(transparency: bool, build: u32) -> bool {
    transparency && build >= WINDOWS_11
}

/// Turns Mica on or off behind the calling window; returns whether it is on now.
#[tauri::command]
pub fn window_effects(window: tauri::WebviewWindow, transparency: bool) -> bool {
    if mica_wanted(transparency, os_build()) {
        let effects = EffectsBuilder::new().effect(Effect::Mica).build();
        window.set_effects(effects).is_ok()
    } else {
        let _ = window.set_effects(None);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mica_only_on_windows_11_and_only_when_wanted() {
        assert!(mica_wanted(true, 26_200));
        assert!(!mica_wanted(false, 26_200));
        assert!(!mica_wanted(true, 19_045), "Windows 10 stays solid");
    }
}
