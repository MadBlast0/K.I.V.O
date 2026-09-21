//! Global hotkeys, in the runtime so they work whether or not the app is running: push-to-talk
//! (VOICE-41: hold the configured keys, Ctrl+Space by default, to talk) and Ctrl+Shift+M to switch
//! the permission mode (SECURITY §1.1).

use crate::core::Core;
use kivo_platform::{Chord, HotkeyEvent, HotkeyId, Hotkeys, PlatformError};
use kivo_platform_windows::WindowsHotkeys;
use std::sync::Arc;
use tokio::sync::mpsc;

const PUSH_TO_TALK: HotkeyId = HotkeyId(1);
const SWITCH_MODE: HotkeyId = HotkeyId(2);
const EMERGENCY_STOP: HotkeyId = HotkeyId(3);

fn handle(core: &Core, event: HotkeyEvent) {
    let result = match event {
        HotkeyEvent::Pressed(PUSH_TO_TALK) => core.start_listening().map(drop),
        HotkeyEvent::Released(PUSH_TO_TALK) => core.stop_listening().map(drop),
        HotkeyEvent::Pressed(SWITCH_MODE) => core.cycle_mode().map(drop),
        HotkeyEvent::Pressed(EMERGENCY_STOP) => {
            core.stop_everything();
            Ok(())
        }
        HotkeyEvent::Pressed(_) | HotkeyEvent::Released(_) => return,
    };
    if let Err(refused) = result {
        tracing::debug!(reason = refused.0, ?event, "push-to-talk ignored");
    }
}

/// Registers push-to-talk and handles it until KIVO quits.
pub async fn run(core: Arc<Core>, keys: Vec<String>, emergency_stop: Vec<String>) {
    let (tx, mut events) = mpsc::unbounded_channel();
    let hotkeys = match WindowsHotkeys::start(move |e| {
        let _ = tx.send(e);
    }) {
        Ok(hotkeys) => hotkeys,
        Err(e) => {
            tracing::error!(%e, "couldn't start the hotkey thread");
            return;
        }
    };
    let chord = Chord(keys);
    match hotkeys.register(PUSH_TO_TALK, &chord) {
        Ok(()) => tracing::info!(%chord, "push-to-talk ready"),
        // Another app owns the keys; the rebind prompt joins with Settings → Shortcuts (M1).
        Err(PlatformError::Conflict(_)) => {
            tracing::warn!(%chord, "another app already uses the push-to-talk keys");
        }
        Err(e) => tracing::error!(%e, %chord, "couldn't register push-to-talk"),
    }
    let mode_keys = Chord(vec!["Ctrl".into(), "Shift".into(), "M".into()]);
    if let Err(e) = hotkeys.register(SWITCH_MODE, &mode_keys) {
        tracing::warn!(%e, chord = %mode_keys, "couldn't register the mode hotkey");
    }
    let stop_keys = Chord(emergency_stop);
    if let Err(e) = hotkeys.register(EMERGENCY_STOP, &stop_keys) {
        tracing::error!(%e, chord = %stop_keys, "couldn't register the emergency stop");
    }
    let shutdown = core.shutdown();
    loop {
        tokio::select! {
            Some(event) = events.recv() => handle(&core, event),
            () = shutdown.cancelled() => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kivo_core::SessionState;

    #[test]
    fn holding_the_keys_listens_and_releasing_ends_the_turn() {
        let core = Core::new();
        handle(&core, HotkeyEvent::Pressed(PUSH_TO_TALK));
        assert_eq!(core.session(), SessionState::Listening);
        handle(&core, HotkeyEvent::Released(PUSH_TO_TALK));
        assert_eq!(core.session(), SessionState::Idle);
        handle(&core, HotkeyEvent::Pressed(SWITCH_MODE));
        assert_eq!(
            core.state().borrow().mode,
            kivo_core::config::PermissionMode::Ask,
            "Ctrl+Shift+M moves Auto on to Ask"
        );
        core.start_listening().unwrap();
        handle(&core, HotkeyEvent::Pressed(EMERGENCY_STOP));
        assert_eq!(
            core.session(),
            SessionState::Idle,
            "the emergency stop ends the turn"
        );
        handle(&core, HotkeyEvent::Pressed(HotkeyId(99)));
        assert_eq!(
            core.session(),
            SessionState::Idle,
            "other hotkeys are not push-to-talk"
        );
    }
}
