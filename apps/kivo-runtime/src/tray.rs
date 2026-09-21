//! The tray icon (ARCHITECTURE §1, UX §1): it shows that KIVO is on and what it is doing, and it
//! is the way to open the Control Center, pause listening or quit even when the app is not
//! running. The menu, icon and tooltip are derived from the session state.

use crate::core::{Core, describe};
use crate::engine::Engine;
use kivo_core::SessionState;
use kivo_core::config::PermissionMode;
use kivo_ipc::StateSnapshot;
use kivo_platform::{Tray, TrayIcon, TrayMenuItem};
use kivo_platform_windows::{TrayEvent, WindowsTray};
use std::sync::Arc;
use tokio::sync::mpsc;

const OPEN: &str = "open";
const PAUSE: &str = "pause";
const RESUME: &str = "resume";
const SETTINGS: &str = "settings";
const QUIT: &str = "quit";
const HIDE_ISLAND: &str = "hide-island";
const SHOW_ISLAND: &str = "show-island";
const STOP: &str = "stop";
/// UX §1: "Hide overlay for 1 hour".
const HIDE_FOR: std::time::Duration = std::time::Duration::from_secs(60 * 60);
/// Menu ids for the permission modes: "mode:<mode>".
const MODE_PREFIX: &str = "mode:";
/// Bypass needs its confirmation step, which lives in the Control Center (SEC-03).
const BYPASS: &str = "mode:bypass";

const MODES: [(PermissionMode, &str, &str); 4] = [
    (PermissionMode::Ask, "mode:ask", "Ask every time"),
    (
        PermissionMode::AcceptEdits,
        "mode:accept-edits",
        "Accept edits",
    ),
    (PermissionMode::Plan, "mode:plan", "Plan first"),
    (PermissionMode::Auto, "mode:auto", "Auto"),
];

fn mode_label(mode: PermissionMode) -> &'static str {
    MODES
        .iter()
        .find(|(m, _, _)| *m == mode)
        .map_or("Bypass permissions", |(_, _, label)| label)
}

/// What the tray shows: it changes with these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Shown {
    session: SessionState,
    mode: PermissionMode,
    island_hidden: bool,
}

impl From<&StateSnapshot> for Shown {
    fn from(s: &StateSnapshot) -> Self {
        Self {
            session: s.session,
            mode: s.mode,
            island_hidden: s.island_hidden,
        }
    }
}

fn menu(shown: Shown) -> Vec<TrayMenuItem> {
    let Shown {
        session: state,
        mode,
        island_hidden,
    } = shown;
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
    let mut modes: Vec<TrayMenuItem> = MODES
        .iter()
        .map(|(m, id, label)| TrayMenuItem::Check {
            id: (*id).into(),
            label: (*label).into(),
            checked: *m == mode,
        })
        .collect();
    modes.push(TrayMenuItem::Check {
        id: BYPASS.into(),
        label: "Bypass permissions…".into(),
        checked: mode == PermissionMode::Bypass,
    });
    vec![
        item(OPEN, "Open KIVO", true),
        listening,
        TrayMenuItem::Submenu {
            label: "Permission mode".into(),
            items: modes,
        },
        if island_hidden {
            item(SHOW_ISLAND, "Show the Island", true)
        } else {
            item(HIDE_ISLAND, "Hide Island for 1 hour", true)
        },
        TrayMenuItem::Separator,
        item(STOP, "Stop everything", true),
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
    // Tasks arrive in M5; until then nothing runs in the background.
    format!(
        "KIVO · {}\n{} mode · 0 tasks running",
        describe(state),
        mode_label(mode)
    )
}

/// What a tray choice does.
fn handle(core: &Arc<Core>, engine: Option<&Arc<Engine>>, event: &TrayEvent) {
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
            HIDE_ISLAND => {
                core.hide_island_for(HIDE_FOR);
                Ok(())
            }
            SHOW_ISLAND => {
                core.set_island_hidden(false);
                Ok(())
            }
            STOP => {
                // Stop everything: the turn, the voice and (from M5) tasks (SEC-25).
                match engine {
                    Some(engine) => engine.stop_everything(),
                    None => core.stop_everything(),
                }
                Ok(())
            }
            // Bypass is confirmed in the Control Center, not from a menu click (SEC-03).
            BYPASS => {
                core.open_control_center(Some("permissions"));
                Ok(())
            }
            mode if mode.starts_with(MODE_PREFIX) => {
                match MODES.iter().find(|(_, id, _)| *id == mode) {
                    Some((m, _, _)) => core.set_mode(*m).map(drop),
                    None => Ok(()),
                }
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
pub async fn run(core: Arc<Core>, engine: Arc<Engine>) {
    let mut state = core.state();
    let mut shown = Shown::from(&*state.borrow_and_update());
    let (tx, mut events) = mpsc::unbounded_channel();
    let tray = match WindowsTray::start(
        &tooltip(shown.session, shown.mode),
        &menu(shown),
        move |e| {
            let _ = tx.send(e);
        },
    ) {
        Ok(tray) => tray,
        Err(e) => {
            // KIVO still works without the tray (voice, the app); the error is logged.
            tracing::error!(%e, "couldn't create the tray icon");
            return;
        }
    };
    if let Err(e) = tray.set_icon(icon(shown.session)) {
        tracing::warn!(%e, "tray icon update failed");
    }
    let shutdown = core.shutdown();
    loop {
        tokio::select! {
            Some(event) = events.recv() => handle(&core, Some(&engine), &event),
            changed = state.changed() => {
                if changed.is_err() { break }
                let now = Shown::from(&*state.borrow_and_update());
                if now == shown { continue }
                shown = now;
                let updated = tray
                    .set_icon(icon(now.session))
                    .and_then(|()| tray.set_tooltip(&tooltip(now.session, now.mode)))
                    .and_then(|()| tray.set_menu(&menu(now)));
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
        let shown = |session| Shown {
            session,
            mode: PermissionMode::Auto,
            island_hidden: false,
        };
        assert_eq!(
            ids(&menu(shown(SessionState::Idle))),
            [
                (OPEN, true),
                (PAUSE, true),
                (HIDE_ISLAND, true),
                (STOP, true),
                (SETTINGS, true),
                (QUIT, true)
            ]
        );
        assert_eq!(ids(&menu(shown(SessionState::Paused)))[1], (RESUME, true));
        assert_eq!(ids(&menu(shown(SessionState::Thinking)))[1], (PAUSE, false));
        let hidden = Shown {
            island_hidden: true,
            ..shown(SessionState::Idle)
        };
        assert_eq!(ids(&menu(hidden))[2], (SHOW_ISLAND, true));
    }

    #[test]
    fn the_mode_submenu_checks_the_current_mode() {
        let menu = menu(Shown {
            session: SessionState::Idle,
            mode: PermissionMode::Plan,
            island_hidden: false,
        });
        let Some(TrayMenuItem::Submenu { items, .. }) = menu.get(2) else {
            panic!("the third item is the permission-mode submenu");
        };
        let checked: Vec<&str> = items
            .iter()
            .filter_map(|i| match i {
                TrayMenuItem::Check {
                    id, checked: true, ..
                } => Some(id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(checked, ["mode:plan"]);
        assert_eq!(items.len(), 5, "four modes and Bypass");
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
        let core = Arc::new(Core::default());
        handle(&core, None, &TrayEvent::Menu(PAUSE.into()));
        assert_eq!(core.session(), SessionState::Paused);
        handle(&core, None, &TrayEvent::Menu(RESUME.into()));
        assert_eq!(core.session(), SessionState::Idle);
        handle(&core, None, &TrayEvent::Menu("mode:ask".into()));
        assert_eq!(core.state().borrow().mode, PermissionMode::Ask);
        handle(&core, None, &TrayEvent::Menu(BYPASS.into()));
        assert_eq!(
            core.state().borrow().mode,
            PermissionMode::Ask,
            "Bypass isn't switched on from the tray"
        );
        handle(&core, None, &TrayEvent::Menu(HIDE_ISLAND.into()));
        assert!(core.state().borrow().island_hidden);
        handle(&core, None, &TrayEvent::Menu(SHOW_ISLAND.into()));
        assert!(!core.state().borrow().island_hidden);
        core.begin_turn("t1", kivo_core::event::TurnSource::PushToTalk, "")
            .unwrap();
        handle(&core, None, &TrayEvent::Menu(STOP.into()));
        assert_eq!(core.session(), SessionState::Idle);
        handle(&core, None, &TrayEvent::Menu(QUIT.into()));
        assert!(core.shutdown().is_cancelled());
    }
}
