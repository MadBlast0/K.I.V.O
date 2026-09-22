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
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder,
};

pub const LABEL: &str = "overlay";
/// Sent to the Island page to open its text field (UX-41).
pub const TYPE_EVENT: &str = "island://type";
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
    /// The runtime has a turn to show (its answer stays up for a moment after the session ends).
    turn: bool,
    /// The user is typing to KIVO (UX-41): the Island stays and can take focus.
    typing: bool,
    /// The pointer is over the Island: it stays until the pointer leaves (UX-10).
    hovering: bool,
    /// Where the current request began (the centre of the window in front), in physical pixels.
    anchor: Option<(i32, i32)>,
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

/// The session changed (`None`: not connected, or the Island is set to hide right now): show the
/// Island while it is active or has a turn to show.
pub fn apply(
    app: &AppHandle,
    session: Option<SessionState>,
    turn: bool,
    anchor: Option<(i32, i32)>,
) {
    let state = app.state::<Overlay>();
    let mut guard = state.0.lock().unwrap_or_else(|e| e.into_inner());
    guard.session = session;
    guard.turn = turn && session.is_some();
    let moved = anchor.is_some() && anchor != guard.anchor;
    if anchor.is_some() {
        guard.anchor = anchor;
    }
    update(app, &mut guard);
    // The request's window is known a moment after the Island appears: move there if needed.
    if moved
        && guard.shown
        && let Some(window) = app.get_webview_window(LABEL)
    {
        place(app, &window, guard.anchor);
    }
}

/// Ctrl+Shift+Space (UX-41): open the Island with a text field that has focus.
pub fn start_typing(app: &AppHandle) {
    take_focus(app);
    let _ = app.emit_to(LABEL, TYPE_EVENT, ());
}

/// The Island may take keyboard focus until it gives it back (`overlay_typing_done`).
fn take_focus(app: &AppHandle) {
    {
        let state = app.state::<Overlay>();
        let mut guard = state.0.lock().unwrap_or_else(|e| e.into_inner());
        guard.typing = true;
        update(app, &mut guard);
    }
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.set_focusable(true);
        let _ = window.set_ignore_cursor_events(false);
        let _ = window.set_focus();
    }
}

/// The user clicked into the Island to type (fix the transcript, a follow-up in the footer):
/// the Island takes focus until the text is sent or dropped (UX-09).
#[tauri::command]
pub fn overlay_focus(window: tauri::WebviewWindow) {
    if window.label() == LABEL {
        take_focus(window.app_handle());
    }
}

/// The Island asks to go back to never taking focus (typing finished or was cancelled).
fn stop_typing(app: &AppHandle) {
    {
        let state = app.state::<Overlay>();
        let mut guard = state.0.lock().unwrap_or_else(|e| e.into_inner());
        guard.typing = false;
        update(app, &mut guard);
    }
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.set_focusable(false);
    }
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
    let visible = active
        || state.turn
        || state.typing
        || state.hovering
        || (state.noticing && state.session.is_some());
    state.generation += 1;
    if visible {
        if !state.shown {
            state.shown = true;
            show(app, state.anchor);
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

fn show(app: &AppHandle, anchor: Option<(i32, i32)>) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    place(app, &window, anchor);
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

/// Top center of the monitor with the request's window (else the one under the pointer), 8 px
/// down. The height follows the Island (`overlay_fit`).
fn place(app: &AppHandle, window: &tauri::WebviewWindow, anchor: Option<(i32, i32)>) {
    let monitor = anchor
        .and_then(|(x, y)| {
            app.monitor_from_point(f64::from(x), f64::from(y))
                .ok()
                .flatten()
        })
        .or_else(|| {
            app.cursor_position()
                .ok()
                .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        })
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

/// The Island's buttons (Allow, Deny, Stop, the text field) must receive clicks; the rest of the
/// time it lets clicks through to what is underneath (UX §2: it never gets in the way).
#[tauri::command]
pub fn overlay_interactive(window: tauri::WebviewWindow, interactive: bool) {
    if window.label() == LABEL {
        let _ = window.set_ignore_cursor_events(!interactive);
    }
}

/// The pointer entered or left the Island: while it is over it, the Island stays (UX-10).
#[tauri::command]
pub fn overlay_hover(window: tauri::WebviewWindow, hovering: bool) {
    if window.label() != LABEL {
        return;
    }
    let app = window.app_handle();
    let state = app.state::<Overlay>();
    let mut guard = state.0.lock().unwrap_or_else(|e| e.into_inner());
    guard.hovering = hovering;
    update(app, &mut guard);
}

/// The Island's text field closed: it no longer takes focus.
#[tauri::command]
pub fn overlay_typing_done(window: tauri::WebviewWindow) {
    if window.label() == LABEL {
        stop_typing(window.app_handle());
    }
}

/// What the Island may ask the runtime: only the actions its own buttons offer (SECURITY §9: the
/// overlay window gets almost nothing).
const ISLAND_METHODS: [&str; 5] = [
    kivo_ipc::method::SESSION_CANCEL,
    kivo_ipc::method::SESSION_SAY,
    kivo_ipc::method::SESSION_TALK,
    kivo_ipc::method::PERMISSIONS_ANSWER,
    kivo_ipc::method::SESSION_STOP_ALL,
];

/// "Turn it on" (CAP-02) may only switch on the capability the current request needed, and only
/// on: the Island can never switch a capability off or turn on anything else.
fn allowed_capability_change(
    needed: Option<kivo_core::Capability>,
    params: &serde_json::Value,
) -> bool {
    let needed = needed.and_then(|c| serde_json::to_value(c).ok());
    params.get("on") == Some(&serde_json::Value::Bool(true))
        && needed.is_some()
        && params.get("capability") == needed.as_ref()
}

/// A request from one of the Island's buttons.
#[tauri::command]
pub async fn island_request(
    window: tauri::WebviewWindow,
    runtime: tauri::State<'_, crate::runtime::Runtime>,
    method: String,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    if window.label() != LABEL {
        return Err("only the Island sends these".into());
    }
    if method == "island.openControlCenter" {
        crate::show_main_window(window.app_handle(), Some("activity"));
        return Ok(serde_json::Value::Null);
    }
    // The mode chip opens Home, where the mode is switched; the overlay never switches it.
    if method == "island.openMode" {
        crate::show_main_window(window.app_handle(), Some("home"));
        return Ok(serde_json::Value::Null);
    }
    let params = params.unwrap_or(serde_json::Value::Null);
    let capability = method == kivo_ipc::method::CAPABILITIES_SET
        && allowed_capability_change(
            runtime
                .link()
                .snapshot
                .and_then(|s| s.turn)
                .and_then(|t| t.capability_off),
            &params,
        );
    if !capability && !ISLAND_METHODS.contains(&method.as_str()) {
        return Err(format!("the Island can't ask for {method}"));
    }
    runtime.request(&method, params).await
}

#[cfg(test)]
mod tests {
    use super::allowed_capability_change;
    use kivo_core::Capability;
    use serde_json::json;

    #[test]
    fn the_island_may_only_turn_on_the_capability_the_request_needed() {
        let needed = Some(Capability::ScreenAwareness);
        let on = |c: &str| json!({ "capability": c, "on": true });
        assert!(allowed_capability_change(needed, &on("screen-awareness")));
        assert!(!allowed_capability_change(needed, &on("computer-use")));
        assert!(!allowed_capability_change(
            needed,
            &json!({ "capability": "screen-awareness", "on": false })
        ));
        assert!(!allowed_capability_change(None, &on("screen-awareness")));
    }
}
