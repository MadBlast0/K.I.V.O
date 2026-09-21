//! The Island overlay window (UX §2, UX-05). Created hidden at startup so it can appear without
//! loading anything: transparent, borderless, always on top, never focusable (`WS_EX_NOACTIVATE`),
//! click-through and absent from the taskbar. It is shown at the top center of the monitor under
//! the pointer while the session is active, and hidden otherwise, with WebView2 set invisible so a
//! hidden Island costs no frames (UX §3 idle rule). The window is only as tall as the Island needs
//! (the page reports it): compositing a large transparent window every frame cost ~12 W of package
//! power while animating (`kivo-bench overlay`).

use kivo_core::SessionState;
use std::sync::Mutex;
use std::time::Duration;
use tauri::window::Color;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "overlay";
/// Wide enough for the largest card (520 px) plus its shadow.
const WIDTH: f64 = 560.0;
/// Before the page reports its size: the collapsed Island (36 px) and its shadow.
const INITIAL_HEIGHT: f64 = 84.0;
/// The Island sits 8 px below the top edge (UX §2).
const TOP: f64 = 8.0;
/// The Island's exit animation runs before the window is hidden.
const HIDE_AFTER: Duration = Duration::from_millis(450);
/// How long a mode-change notice shows (the page uses the same time).
const NOTICE: Duration = Duration::from_millis(1600);

#[derive(Default)]
struct State {
    shown: bool,
    /// Bumped on every change, so a pending hide is dropped when the Island comes back first.
    generation: u64,
    /// The last session state (`None`: not connected).
    session: Option<SessionState>,
    /// A mode-change notice is showing.
    noticing: bool,
}

#[derive(Default)]
pub struct Overlay(Mutex<State>);

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
        .inner_size(WIDTH, INITIAL_HEIGHT)
        .build()?;
    window.set_ignore_cursor_events(true)?;
    window.as_ref().hide()?;
    Ok(())
}

/// The session changed (`None`: not connected): show the Island while it is active.
pub fn apply(app: &AppHandle, session: Option<SessionState>) {
    let state = app.state::<Overlay>();
    let mut guard = state.0.lock().unwrap_or_else(|e| e.into_inner());
    guard.session = session;
    update(app, &mut guard);
}

/// The permission mode changed: show the Island's notice for a moment.
pub fn notice(app: &AppHandle) {
    {
        let state = app.state::<Overlay>();
        let mut guard = state.0.lock().unwrap_or_else(|e| e.into_inner());
        guard.noticing = true;
        update(app, &mut guard);
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(NOTICE).await;
        let state = app.state::<Overlay>();
        let mut guard = state.0.lock().unwrap_or_else(|e| e.into_inner());
        guard.noticing = false;
        update(&app, &mut guard);
    });
}

/// Shows the window if there is something to show, else hides it after the exit animation.
fn update(app: &AppHandle, state: &mut State) {
    let active = state
        .session
        .is_some_and(|s| !matches!(s, SessionState::Idle | SessionState::Paused));
    let visible = active || (state.noticing && state.session.is_some());
    state.generation += 1;
    if visible {
        if !state.shown {
            state.shown = true;
            show(app);
        }
    } else if state.shown {
        let expected = state.generation;
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(HIDE_AFTER).await;
            let overlay = app.state::<Overlay>();
            let mut guard = overlay.0.lock().unwrap_or_else(|e| e.into_inner());
            if guard.generation == expected && guard.shown {
                guard.shown = false;
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

/// The page's Island (with its shadow) is `height` CSS px tall: fit the window to it, up to half
/// the monitor (UX §2: the card grows to at most 50% of the screen).
#[tauri::command]
pub fn overlay_fit(window: tauri::WebviewWindow, height: f64) {
    if window.label() != LABEL || !height.is_finite() {
        return;
    }
    let Ok(Some(monitor)) = window.current_monitor() else {
        return;
    };
    let scale = monitor.scale_factor();
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "window sizes in physical pixels"
    )]
    let (width, height) = (
        (WIDTH * scale).round() as u32,
        ((height.max(1.0) * scale).ceil() as u32).min(monitor.size().height / 2),
    );
    let _ = window.set_size(PhysicalSize::new(width, height));
}

/// Top center of the monitor under the pointer, 8 px down. The height follows the Island
/// (`overlay_fit`).
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
    let width = (WIDTH * scale).round() as u32;
    #[allow(clippy::cast_possible_truncation, reason = "a pixel offset")]
    let top = (TOP * scale).round() as i32;
    let left = origin.x + i32::try_from(area.width.saturating_sub(width) / 2).unwrap_or(0);
    let _ = window.set_position(PhysicalPosition::new(left, origin.y + top));
}
