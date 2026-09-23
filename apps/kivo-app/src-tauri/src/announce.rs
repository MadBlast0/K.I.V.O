//! Screen-reader announcements (UX §10, UX-53): the Island's state changes, what KIVO heard and
//! what it answers are raised as UI Automation notifications on the overlay window, so Narrator,
//! NVDA and JAWS read them even though the Island never takes focus.

/// Longest announcement; a screen reader reads the rest from the Control Center.
const MAX_CHARS: usize = 400;

/// Raises one notification on the calling window. The newest one replaces any still queued.
#[tauri::command]
pub fn announce(window: tauri::WebviewWindow, text: String) {
    let text: String = text.trim().chars().take(MAX_CHARS).collect();
    if text.is_empty() {
        return;
    }
    #[cfg(windows)]
    raise(&window, &text);
    #[cfg(not(windows))]
    let _ = window;
}

#[cfg(windows)]
fn raise(window: &tauri::WebviewWindow, text: &str) {
    use windows::Win32::UI::Accessibility::{
        NotificationKind_Other, NotificationProcessing_ImportantMostRecent,
        UiaHostProviderFromHwnd, UiaRaiseNotificationEvent,
    };
    use windows::core::BSTR;
    let Ok(hwnd) = window.hwnd() else { return };
    // SAFETY: the window handle is live for the duration of the call; UIA copies the strings.
    let raised = unsafe {
        UiaHostProviderFromHwnd(hwnd).and_then(|provider| {
            UiaRaiseNotificationEvent(
                &provider,
                NotificationKind_Other,
                NotificationProcessing_ImportantMostRecent,
                &BSTR::from(text),
                &BSTR::from("KIVO.Island"),
            )
        })
    };
    if let Err(e) = raised {
        eprintln!("kivo-app: couldn't announce: {e}");
    }
}
