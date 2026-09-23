//! The Island overlay window (UX §2, UX-05). Created hidden at startup so it can appear without
//! loading anything: transparent, borderless, always on top, never focusable (`WS_EX_NOACTIVATE`),
//! click-through and absent from the taskbar. It is shown at the top center of the monitor under
//! the pointer while the session is active, and hidden otherwise, with WebView2 set invisible so a
//! hidden Island costs no frames (UX §3 idle rule). The window is only as tall as the Island needs
//! (the page reports it): compositing a large transparent window every frame cost ~12 W of package
//! power while animating (`kivo-bench overlay`).

use crate::memory::{self, Visibility};
use kivo_core::SessionState;
use kivo_core::config::OverlayPosition;
use kivo_ipc::protocol::IslandPlacement;
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
/// A drag has ended when the Island hasn't moved for this long (UX-13).
const DRAG_SETTLE: Duration = Duration::from_millis(500);
/// Below a title bar, this much space (UX-14).
const BELOW_TITLE: f64 = 4.0;
/// Only a title bar this close to the monitor's top edge is under the Island.
const TITLE_REACH: f64 = 120.0;

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
    /// The placement setting and remembered spots (UX-13).
    placement: IslandPlacement,
    /// The bottom of the title bar or tabs of the window in front (UX-14).
    title_bar_bottom: Option<i32>,
    /// A drag is in progress: moves are reported once it settles (UX-13).
    dragging: bool,
    /// Bumped on every move while dragging.
    drag_generation: u64,
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
    memory::apply(&window, Visibility::Hidden, false);
    // A drag moves the window; once it settles, the spot is remembered for that monitor.
    let app_handle = app.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Moved(_) = event {
            moved(&app_handle);
        }
    });
    Ok(())
}

/// The Island asks to be dragged (the user pressed on it, UX-13).
#[tauri::command]
pub fn overlay_drag(window: tauri::WebviewWindow) {
    if window.label() != LABEL {
        return;
    }
    {
        let state = window.app_handle().state::<Overlay>();
        let mut guard = state.0.lock().unwrap_or_else(|e| e.into_inner());
        guard.dragging = true;
    }
    let _ = window.start_dragging();
}

fn moved(app: &AppHandle) {
    let generation = {
        let state = app.state::<Overlay>();
        let mut guard = state.0.lock().unwrap_or_else(|e| e.into_inner());
        if !guard.dragging {
            return;
        }
        guard.drag_generation += 1;
        guard.drag_generation
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(DRAG_SETTLE).await;
        {
            let state = app.state::<Overlay>();
            let mut guard = state.0.lock().unwrap_or_else(|e| e.into_inner());
            if guard.drag_generation != generation || !guard.dragging {
                return;
            }
            guard.dragging = false;
        }
        let Some(window) = app.get_webview_window(LABEL) else {
            return;
        };
        let (Ok(pos), Ok(size)) = (window.outer_position(), window.outer_size()) else {
            return;
        };
        let centre = (
            f64::from(pos.x) + f64::from(size.width) / 2.0,
            f64::from(pos.y) + 20.0,
        );
        let Some(monitor) = app.monitor_from_point(centre.0, centre.1).ok().flatten() else {
            return;
        };
        let origin = monitor.position();
        let params = serde_json::json!({
            "monitor": monitor.name().cloned().unwrap_or_default(),
            "x": pos.x - origin.x,
            "y": pos.y - origin.y,
        });
        let runtime = app.state::<crate::runtime::Runtime>();
        if let Err(e) = runtime
            .request(kivo_ipc::method::ISLAND_MOVED, params)
            .await
        {
            eprintln!("kivo-app: couldn't remember the Island's place: {e}");
        }
    });
}

/// The session changed (`None`: not connected, or the Island is set to hide right now): show the
/// Island while it is active or has a turn to show.
pub fn apply(
    app: &AppHandle,
    session: Option<SessionState>,
    turn: bool,
    anchor: Option<(i32, i32)>,
    placement: IslandPlacement,
    title_bar_bottom: Option<i32>,
) {
    let state = app.state::<Overlay>();
    let mut guard = state.0.lock().unwrap_or_else(|e| e.into_inner());
    let was_listening = listening(guard.session);
    guard.session = session;
    guard.turn = turn && session.is_some();
    let moved = (anchor.is_some() && anchor != guard.anchor)
        || placement != guard.placement
        || title_bar_bottom != guard.title_bar_bottom
        || was_listening != listening(session);
    if anchor.is_some() {
        guard.anchor = anchor;
    }
    guard.placement = placement;
    guard.title_bar_bottom = title_bar_bottom;
    update(app, &mut guard);
    // The request's window is known a moment after the Island appears, and the Island moves
    // below a title bar only while listening: place it again when either changes.
    if moved
        && guard.shown
        && !guard.dragging
        && let Some(window) = app.get_webview_window(LABEL)
    {
        place(app, &window, &guard);
    }
}

fn listening(session: Option<SessionState>) -> bool {
    matches!(
        session,
        Some(SessionState::Listening | SessionState::FollowUp)
    )
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
            show(app, state);
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

fn show(app: &AppHandle, state: &State) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    place(app, &window, state);
    // Re-assert topmost so the Island also rises above other always-on-top windows (a video's
    // picture-in-picture, another app's floating toolbar). The window is still hidden here, and
    // setting the same value again is ignored, so it is turned off and on.
    let _ = window.set_always_on_top(false);
    let _ = window.set_always_on_top(true);
    memory::apply(&window, Visibility::Shown, false);
    let _ = window.as_ref().show();
    let _ = window.show();
}

fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.as_ref().hide();
        let _ = window.hide();
        memory::apply(&window, Visibility::Hidden, false);
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

/// On the monitor with the request's window (else the one under the pointer): top center 8 px
/// down, bottom center, or where the user dragged it on that monitor (UX-13). While only
/// listening, a title bar or tab strip under it pushes it just below (UX-14). The height
/// follows the Island (`overlay_fit`).
fn place(app: &AppHandle, window: &tauri::WebviewWindow, state: &State) {
    let anchor = state.anchor;
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
    let screen = Screen {
        name: monitor.name().map(String::as_str),
        origin: (monitor.position().x, monitor.position().y),
        size: (monitor.size().width, monitor.size().height),
        scale: monitor.scale_factor(),
    };
    let height = window.outer_size().map_or(0, |s| s.height);
    let (x, y) = position(
        &screen,
        &state.placement,
        height,
        state.title_bar_bottom,
        listening(state.session),
    );
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

/// A monitor as placement sees it, in physical pixels.
struct Screen<'a> {
    name: Option<&'a str>,
    origin: (i32, i32),
    size: (u32, u32),
    scale: f64,
}

/// Where the Island's top-left corner goes on `screen` (see `place`).
fn position(
    screen: &Screen,
    placement: &IslandPlacement,
    height: u32,
    title_bar_bottom: Option<i32>,
    listening: bool,
) -> (i32, i32) {
    let scale = screen.scale;
    let ((ox, oy), (area_w, area_h)) = (screen.origin, screen.size);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "window sizes in physical pixels"
    )]
    let width = (WIDTH * scale).round() as u32;
    #[allow(clippy::cast_possible_truncation, reason = "a pixel offset")]
    let px = |v: f64| (v * scale).round() as i32;
    let left = ox + i32::try_from(area_w.saturating_sub(width) / 2).unwrap_or(0);
    let bottom_edge = oy + i32::try_from(area_h).unwrap_or(0);
    let spot = placement
        .spots
        .iter()
        .find(|s| screen.name.is_some_and(|n| n == s.monitor));
    let (x, y) = match (placement.position, spot) {
        (OverlayPosition::RememberDrag, Some(s)) => {
            // Kept on the monitor even if its size changed since.
            let max_x = i32::try_from(area_w.saturating_sub(width)).unwrap_or(0);
            let max_y = i32::try_from(area_h.saturating_sub(height.max(1))).unwrap_or(0);
            (ox + s.x.clamp(0, max_x), oy + s.y.clamp(0, max_y))
        }
        (OverlayPosition::BottomCenter, _) => (
            left,
            bottom_edge - i32::try_from(height).unwrap_or(0) - px(TOP) - px(48.0),
        ),
        _ => (left, oy + px(TOP)),
    };
    // UX-14: only near the top, only while listening, only over a title bar.
    let y = match title_bar_bottom {
        Some(bar)
            if listening
                && placement.position != OverlayPosition::BottomCenter
                && bar > y
                && bar - oy <= px(TITLE_REACH) =>
        {
            bar + px(BELOW_TITLE)
        }
        _ => y,
    };
    (x, y)
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
const ISLAND_METHODS: [&str; 7] = [
    kivo_ipc::method::SESSION_CANCEL,
    // The Undo button (UX-43): only takes back KIVO's own last change.
    kivo_ipc::method::SESSION_UNDO,
    // "That's not what I meant" on a brain's answer (BRAIN-06): it only records a report.
    kivo_ipc::method::CHAT_MISROUTE,
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
    use super::{Screen, allowed_capability_change, position};
    use kivo_core::Capability;
    use kivo_core::config::{IslandSpot, OverlayPosition};
    use kivo_ipc::protocol::IslandPlacement;
    use serde_json::json;

    const SECOND: Screen<'static> = Screen {
        name: Some(r"\\.\DISPLAY2"),
        origin: (1920, 0),
        size: (2560, 1440),
        scale: 1.5,
    };

    fn placed(position_: OverlayPosition, spots: Vec<IslandSpot>) -> IslandPlacement {
        IslandPlacement {
            position: position_,
            spots,
        }
    }

    /// UX-13: top center 8 px down, bottom center above the taskbar's reach, or the spot the
    /// user dragged it to on this monitor — kept on screen if the monitor shrank.
    #[test]
    fn place_top_bottom_and_remembered_spots() {
        let width = 840; // WIDTH at 150 %
        let centre_x = 1920 + (2560 - width) / 2;
        let top = position(
            &SECOND,
            &placed(OverlayPosition::TopCenter, vec![]),
            120,
            None,
            false,
        );
        assert_eq!(top, (centre_x, 12));
        let bottom = position(
            &SECOND,
            &placed(OverlayPosition::BottomCenter, vec![]),
            120,
            None,
            false,
        );
        assert_eq!(bottom, (centre_x, 1440 - 120 - 12 - 72));
        let spot = |monitor: &str, x, y| IslandSpot {
            monitor: monitor.into(),
            x,
            y,
        };
        let remembered = placed(
            OverlayPosition::RememberDrag,
            vec![spot(r"\\.\DISPLAY1", 5, 5), spot(r"\\.\DISPLAY2", 300, 900)],
        );
        assert_eq!(
            position(&SECOND, &remembered, 120, None, false),
            (1920 + 300, 900),
            "this monitor's own spot"
        );
        let off_screen = placed(
            OverlayPosition::RememberDrag,
            vec![spot(r"\\.\DISPLAY2", 5000, -40)],
        );
        assert_eq!(
            position(&SECOND, &off_screen, 120, None, false),
            (1920 + 2560 - width, 0),
            "clamped onto the monitor"
        );
        let elsewhere = placed(
            OverlayPosition::RememberDrag,
            vec![spot(r"\\.\DISPLAY1", 5, 5)],
        );
        assert_eq!(
            position(&SECOND, &elsewhere, 120, None, false),
            (centre_x, 12),
            "no spot on this monitor yet: top center"
        );
    }

    /// UX-14: while listening, a title bar under the Island moves it just below; not while it
    /// shows anything else, not at the bottom, and not for a bar far down the screen.
    #[test]
    fn listening_moves_below_a_title_bar_only_while_listening() {
        let top = placed(OverlayPosition::TopCenter, vec![]);
        let (_, y) = position(&SECOND, &top, 120, Some(48), true);
        assert_eq!(y, 48 + 6);
        assert_eq!(position(&SECOND, &top, 120, Some(48), false).1, 12);
        assert_eq!(
            position(&SECOND, &top, 120, Some(400), true).1,
            12,
            "a bar out of reach"
        );
        assert_eq!(
            position(&SECOND, &top, 120, Some(8), true).1,
            12,
            "a bar above the Island"
        );
        let bottom = placed(OverlayPosition::BottomCenter, vec![]);
        assert_eq!(
            position(&SECOND, &bottom, 120, Some(48), true).1,
            1440 - 120 - 12 - 72
        );
    }

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
