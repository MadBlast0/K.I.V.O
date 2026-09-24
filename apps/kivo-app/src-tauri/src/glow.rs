//! The wake glow (UX-16): a soft light along the edges of the screen for a moment when "Hey
//! Kivo" wakes KIVO. Off by default (Settings → Island). A transparent, click-through window that
//! never takes focus covers the monitor where the user is working only while the glow plays
//! (~400 ms), then hides: at rest it isn't shown and draws nothing.
//!
//! While KIVO controls the screen (computer use, CAP-12) the same window shows the frame the user
//! chose (subtle or full), a banner ("KIVO is controlling your screen — Stop
//! (Ctrl+Alt+Shift+Esc)") and KIVO's cursor where it acts; it hides when control ends.

use tauri::window::Color;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder,
};

pub const LABEL: &str = "glow";
/// The event that plays the glow once.
const PLAY: &str = "kivo://glow";
/// How long the window stays up: the animation (400 ms) and a little slack.
const SHOWN_MS: u64 = 550;

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("glow.html".into()))
        .title("KIVO glow")
        .transparent(true)
        .background_color(Color(0, 0, 0, 0))
        .decorations(false)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focusable(false)
        .focused(false)
        .visible(false)
        .build()?;
    window.set_ignore_cursor_events(true)?;
    Ok(())
}

/// Plays the glow on the monitor with `anchor` (the window in front), else the one under the
/// pointer.
pub fn flash(app: &AppHandle, anchor: Option<(i32, i32)>) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
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
    let _ = window.set_position(PhysicalPosition::new(
        monitor.position().x,
        monitor.position().y,
    ));
    let _ = window.set_size(PhysicalSize::new(
        monitor.size().width,
        monitor.size().height,
    ));
    let _ = window.show();
    let _ = window.set_ignore_cursor_events(true);
    let _ = window.emit_to(LABEL, PLAY, ());
    let window = window.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(SHOWN_MS));
        let _ = window.hide();
    });
}

/// The control overlay's state, sent to the page.
const CONTROL: &str = "kivo://control";

/// Shows, updates or hides the control overlay (CAP-12).
pub fn control(
    app: &AppHandle,
    view: Option<&kivo_ipc::protocol::ControlView>,
    anchor: Option<(i32, i32)>,
) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    let Some(view) = view else {
        let _ = window.emit_to(LABEL, CONTROL, serde_json::Value::Null);
        let _ = window.hide();
        return;
    };
    let monitor = anchor
        .or(view.cursor.as_ref().map(|c| (c.x, c.y)))
        .and_then(|(x, y)| {
            app.monitor_from_point(f64::from(x), f64::from(y))
                .ok()
                .flatten()
        })
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };
    let (ox, oy) = (monitor.position().x, monitor.position().y);
    let _ = window.set_position(PhysicalPosition::new(ox, oy));
    let _ = window.set_size(PhysicalSize::new(
        monitor.size().width,
        monitor.size().height,
    ));
    let _ = window.show();
    let _ = window.set_ignore_cursor_events(true);
    let scale = monitor.scale_factor();
    #[allow(
        clippy::cast_possible_truncation,
        reason = "a point on the monitor, in CSS pixels"
    )]
    let cursor = view.cursor.as_ref().map(|c| {
        serde_json::json!({
            "x": (f64::from(c.x - ox) / scale).round() as i64,
            "y": (f64::from(c.y - oy) / scale).round() as i64,
        })
    });
    let _ = window.emit_to(
        LABEL,
        CONTROL,
        serde_json::json!({
            "frame": view.frame, "paused": view.paused, "app": view.app,
            "step": view.step, "maxSteps": view.max_steps, "cursor": cursor,
        }),
    );
}

/// Whether this change of state is a wake that should glow: KIVO starts listening from the wake
/// word, with the glow on and the Island not asked to stay away.
pub fn wakes(
    before: Option<kivo_core::SessionState>,
    now: &kivo_ipc::protocol::StateSnapshot,
) -> bool {
    should_glow(
        now.island.wake_glow,
        now.island_hidden,
        before,
        now.session,
        now.turn.as_ref().map(|t| t.source),
    )
}

fn should_glow(
    on: bool,
    hidden: bool,
    before: Option<kivo_core::SessionState>,
    now: kivo_core::SessionState,
    source: Option<kivo_core::event::TurnSource>,
) -> bool {
    use kivo_core::SessionState::{FollowUp, Listening};
    on && !hidden
        && now == Listening
        && !matches!(before, Some(Listening | FollowUp))
        && source == Some(kivo_core::event::TurnSource::WakeWord)
}

#[cfg(test)]
mod tests {
    use super::should_glow;
    use kivo_core::SessionState::{Idle, Listening, Thinking};
    use kivo_core::event::TurnSource::{PushToTalk, WakeWord};

    #[test]
    fn glows_once_on_the_wake_word_only_when_on() {
        assert!(should_glow(
            true,
            false,
            Some(Idle),
            Listening,
            Some(WakeWord)
        ));
        assert!(
            !should_glow(false, false, Some(Idle), Listening, Some(WakeWord)),
            "off by default"
        );
        assert!(
            !should_glow(true, true, Some(Idle), Listening, Some(WakeWord)),
            "Island hidden"
        );
        assert!(
            !should_glow(true, false, Some(Idle), Listening, Some(PushToTalk)),
            "not a wake"
        );
        assert!(
            !should_glow(true, false, Some(Listening), Listening, Some(WakeWord)),
            "once"
        );
        assert!(!should_glow(
            true,
            false,
            Some(Idle),
            Thinking,
            Some(WakeWord)
        ));
    }
}
