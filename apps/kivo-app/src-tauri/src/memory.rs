//! Hidden webviews give memory back (BENCHMARKS §3, BENCH-12: UI idle RAM ≤ 120 MB). While a
//! window is hidden its WebView2 runs at the *low* memory target, and the Control Center is also
//! suspended (like Edge's sleeping tabs: script and timers pause, caches are trimmed). Showing a
//! window resumes it first; a resume takes milliseconds, well inside the 300 ms open budget.
//! The Island is never suspended: it must react to the runtime at once.

use tauri::WebviewWindow;

/// Whether a window's webview is being hidden or shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Hidden,
    Shown,
}

/// Sets the webview's memory target for `visibility`, and suspends or resumes it when `suspend`.
pub fn apply(window: &WebviewWindow, visibility: Visibility, suspend: bool) {
    #[cfg(windows)]
    {
        let result = window.with_webview(move |webview| {
            // SAFETY: WebView2 COM calls on the webview's own thread (with_webview runs there);
            // an older WebView2 without these interfaces is skipped.
            unsafe { windows_impl::apply(&webview.controller(), visibility, suspend) }
        });
        if let Err(e) = result {
            eprintln!("KIVO: couldn't reach the webview to change its memory use: {e}");
        }
    }
    #[cfg(not(windows))]
    let _ = (window, visibility, suspend);
}

#[cfg(windows)]
mod windows_impl {
    use super::Visibility;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
        ICoreWebView2_3, ICoreWebView2_19, ICoreWebView2Controller,
        ICoreWebView2TrySuspendCompletedHandler,
    };
    use windows_core::Interface;

    pub unsafe fn apply(
        controller: &ICoreWebView2Controller,
        visibility: Visibility,
        suspend: bool,
    ) {
        let Ok(core) = (unsafe { controller.CoreWebView2() }) else {
            return;
        };
        let hidden = visibility == Visibility::Hidden;
        if suspend
            && !hidden
            && let Ok(core3) = core.cast::<ICoreWebView2_3>()
        {
            let _ = unsafe { core3.Resume() };
        }
        if let Ok(core19) = core.cast::<ICoreWebView2_19>() {
            let level = if hidden {
                COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
            } else {
                COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
            };
            let _ = unsafe { core19.SetMemoryUsageTargetLevel(level) };
        }
        // TrySuspend needs the controller invisible, which the window's hide has done.
        if suspend
            && hidden
            && let Ok(core3) = core.cast::<ICoreWebView2_3>()
        {
            let _ = unsafe { core3.TrySuspend(None::<&ICoreWebView2TrySuspendCompletedHandler>) };
        }
    }
}
