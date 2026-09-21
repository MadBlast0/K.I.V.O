//! The Island overlay window (UX §2, UX-05). Created hidden at startup so it can appear without
//! loading anything: transparent, borderless, always on top, never focusable (`WS_EX_NOACTIVATE`),
//! click-through and absent from the taskbar. It is shown at the top center of the monitor under
//! the pointer while the session is active, and hidden otherwise, with WebView2 set invisible so a
//! hidden Island costs no frames (UX §3 idle rule).

use kivo_core::SessionState;
use std::sync::Mutex;
use std::time::Duration;
use tauri::window::Color;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "overlay";
/// Wide enough for the largest card (520 px) plus its shadow.
const WIDTH: f64 = 560.0;
/// The Island sits 8 px below the top edge (UX §2).
const TOP: f64 = 8.0;
/// The Island's exit animation runs before the window is hidden.
const HIDE_AFTER: Duration = Duration::from_millis(450);

/// Whether the window is shown, and a generation that cancels a pending hide when the Island
/// comes back before it ran.
#[derive(Default)]
pub struct Overlay(Mutex<(bool, u64)>);

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("overlay.html".into()))
        .title("KIVO Island")
        .transparent(true)
        // No white flash: WebView2 paints nothing until the page does.
        .background_color(Color(0, 0, 0, 0))
        .decorations(false)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focusable(false)
        .focused(false)
        .visible(false)
        .inner_size(WIDTH, 400.0)
        .build()?;
    window.set_ignore_cursor_events(true)?;
    window.as_ref().hide()?;
    Ok(())
}

/// Shows the Island for an active session, hides it otherwise (`None`: not connected).
pub fn apply(app: &AppHandle, session: Option<SessionState>) {
    let visible = session.is_some_and(|s| !matches!(s, SessionState::Idle | SessionState::Paused));
    let state = app.state::<Overlay>();
    let mut guard = state.0.lock().unwrap_or_else(|e| e.into_inner());
    let (shown, generation) = &mut *guard;
    *generation += 1;
    if visible {
        if !*shown {
            *shown = true;
            show(app);
        }
    } else if *shown {
        let expected = *generation;
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(HIDE_AFTER).await;
            let state = app.state::<Overlay>();
            let mut guard = state.0.lock().unwrap_or_else(|e| e.into_inner());
            if guard.1 == expected && guard.0 {
                guard.0 = false;
                hide(&app);
            }
        });
    }
}

fn show(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    place(app, &window);
    // Re-assert topmost so the Island also rises above other always-on-top windows (a video's
    // picture-in-picture, another app's floating toolbar). The window is still hidden here, and
    // setting the same value again is ignored, so it is turned off and on.
    let _ = window.set_always_on_top(false);
    let _ = window.set_always_on_top(true);
    let _ = window.as_ref().show();
    let _ = window.show();
}

fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.as_ref().hide();
        let _ = window.hide();
    }
}

/// Top center of the monitor under the pointer, 8 px down; half the monitor tall so the card
/// can grow to 50% of the screen (UX §2).
fn place(app: &AppHandle, window: &tauri::WebviewWindow) {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };
    let scale = monitor.scale_factor();
    let (area, origin) = (monitor.size(), monitor.position());
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "window sizes in physical pixels"
    )]
    let (width, height) = (
        (WIDTH * scale).round() as u32,
        area.height / 2 + (40.0 * scale).round() as u32,
    );
    #[allow(clippy::cast_possible_truncation, reason = "a pixel offset")]
    let top = (TOP * scale).round() as i32;
    let left = origin.x + i32::try_from(area.width.saturating_sub(width) / 2).unwrap_or(0);
    let _ = window.set_size(PhysicalSize::new(width, height));
    let _ = window.set_position(PhysicalPosition::new(left, origin.y + top));
}
