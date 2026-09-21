//! The tray icon (ARCHITECTURE §1, UX §1): it shows that KIVO is on and what it is doing, and it
//! is the way to open the Control Center, pause listening or quit even when the app is not
//! running. The menu, icon and tooltip are derived from the session state.

use crate::core::{Core, describe};
use kivo_core::SessionState;
use kivo_core::config::PermissionMode;
use kivo_platform::{Tray, TrayIcon, TrayMenuItem};
use kivo_platform_windows::{TrayEvent, WindowsTray};
use std::sync::Arc;
use tokio::sync::mpsc;

const OPEN: &str = "open";
const PAUSE: &str = "pause";
const RESUME: &str = "resume";
const SETTINGS: &str = "settings";
const QUIT: &str = "quit";

fn menu(state: SessionState) -> Vec<TrayMenuItem> {
    let item = |id: &str, label: &str, enabled: bool| TrayMenuItem::Item {
        id: id.into(),
        label: label.into(),
        enabled,
    };
    let listening = if state == SessionState::Paused {
        item(RESUME, "Resume listening", true)
    } else {
        // Pausing is possible only when KIVO isn't in the middle of a request.
        item(
            PAUSE,
            "Pause listening",
            matches!(state, SessionState::Idle | SessionState::FollowUp),
        )
    };
    vec![
        item(OPEN, "Open KIVO", true),
        listening,
        TrayMenuItem::Separator,
        item(SETTINGS, "Settings", true),
        item(QUIT, "Quit KIVO", true),
    ]
}

fn icon(state: SessionState) -> TrayIcon {
    match state {
        SessionState::Listening | SessionState::FollowUp => TrayIcon::Listening,
        SessionState::Paused => TrayIcon::Paused,
        SessionState::Error => TrayIcon::Error,
        _ => TrayIcon::Normal,
    }
}

/// "KIVO · Ready", then the permission mode and running tasks (UX-56).
fn tooltip(state: SessionState, mode: PermissionMode) -> String {
    let mode = match mode {
        PermissionMode::Ask => "Ask every time",
        PermissionMode::AcceptEdits => "Accept edits",
        PermissionMode::Plan => "Plan first",
        PermissionMode::Auto => "Auto",
        PermissionMode::Bypass => "Bypass permissions",
    };
    // Tasks arrive in M5; until then nothing runs in the background.
    format!("KIVO · {}\n{mode} mode · 0 tasks running", describe(state))
}

/// What a tray choice does.
fn handle(core: &Core, event: &TrayEvent) {
    let result = match event {
        TrayEvent::Click => {
            core.open_control_center(None);
            Ok(())
        }
        TrayEvent::Menu(id) => match id.as_str() {
            OPEN => {
                core.open_control_center(None);
                Ok(())
            }
            SETTINGS => {
                core.open_control_center(Some("settings"));
                Ok(())
            }
            PAUSE => core.pause().map(drop),
            RESUME => core.resume().map(drop),
            QUIT => {
                core.quit();
                Ok(())
            }
            other => {
                tracing::warn!(id = other, "unknown tray menu item");
                Ok(())
            }
        },
    };
    if let Err(refused) = result {
        tracing::info!(reason = refused.0, "tray request refused");
    }
}

/// Shows the tray icon and keeps it in step with the session until KIVO quits. The icon is
/// removed when this returns.
pub async fn run(core: Arc<Core>, mode: PermissionMode) {
    let mut state = core.state();
    let mut shown = state.borrow_and_update().session;
    let (tx, mut events) = mpsc::unbounded_channel();
    let tray = match WindowsTray::start(&tooltip(shown, mode), &menu(shown), move |e| {
        let _ = tx.send(e);
    }) {
        Ok(tray) => tray,
        Err(e) => {
            // KIVO still works without the tray (voice, the app); the error is logged.
            tracing::error!(%e, "couldn't create the tray icon");
            return;
        }
    };
    if let Err(e) = tray.set_icon(icon(shown)) {
        tracing::warn!(%e, "tray icon update failed");
    }
    let shutdown = core.shutdown();
    loop {
        tokio::select! {
            Some(event) = events.recv() => handle(&core, &event),
            changed = state.changed() => {
                if changed.is_err() { break }
                let session = state.borrow_and_update().session;
                if session == shown { continue }
                shown = session;
                let updated = tray
                    .set_icon(icon(session))
                    .and_then(|()| tray.set_tooltip(&tooltip(session, mode)))
                    .and_then(|()| tray.set_menu(&menu(session)));
                if let Err(e) = updated {
                    tracing::warn!(%e, "tray update failed");
                }
            }
            () = shutdown.cancelled() => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(items: &[TrayMenuItem]) -> Vec<(&str, bool)> {
        items
            .iter()
            .filter_map(|i| match i {
                TrayMenuItem::Item { id, enabled, .. } => Some((id.as_str(), *enabled)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_menu_offers_pause_or_resume_to_fit_the_state() {
        assert_eq!(
            ids(&menu(SessionState::Idle)),
            [(OPEN, true), (PAUSE, true), (SETTINGS, true), (QUIT, true)]
        );
        assert_eq!(ids(&menu(SessionState::Paused))[1], (RESUME, true));
        assert_eq!(ids(&menu(SessionState::Thinking))[1], (PAUSE, false));
    }

    #[test]
    fn the_icon_and_tooltip_follow_the_state() {
        assert_eq!(icon(SessionState::Listening), TrayIcon::Listening);
        assert_eq!(icon(SessionState::Paused), TrayIcon::Paused);
        assert_eq!(icon(SessionState::Error), TrayIcon::Error);
        assert_eq!(icon(SessionState::Thinking), TrayIcon::Normal);
        assert_eq!(
            tooltip(SessionState::Idle, PermissionMode::Auto),
            "KIVO · Ready\nAuto mode · 0 tasks running"
        );
    }

    #[tokio::test]
    async fn tray_choices_reach_the_core() {
        let core = Core::new();
        handle(&core, &TrayEvent::Menu(PAUSE.into()));
        assert_eq!(core.session(), SessionState::Paused);
        handle(&core, &TrayEvent::Menu(RESUME.into()));
        assert_eq!(core.session(), SessionState::Idle);
        handle(&core, &TrayEvent::Menu(QUIT.into()));
        assert!(core.shutdown().is_cancelled());
    }
}
