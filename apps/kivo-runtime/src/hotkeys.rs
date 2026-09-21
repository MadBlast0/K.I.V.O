//! Global hotkeys, in the runtime so they work whether or not the app is running: push-to-talk
//! (VOICE-41: hold the configured keys, Ctrl+Space by default, to talk), Ctrl+Shift+Space to type
//! to KIVO (UX §8), Ctrl+Shift+M to switch the permission mode (SECURITY §1.1) and the emergency
//! stop (SECURITY §8).

use crate::core::Core;
use crate::engine::Engine;
use kivo_core::event::{CancelReason, TurnSource};
use kivo_platform::{Chord, HotkeyEvent, HotkeyId, Hotkeys, PlatformError};
use kivo_platform_windows::WindowsHotkeys;
use std::sync::Arc;
use tokio::sync::mpsc;

const PUSH_TO_TALK: HotkeyId = HotkeyId(1);
const SWITCH_MODE: HotkeyId = HotkeyId(2);
const EMERGENCY_STOP: HotkeyId = HotkeyId(3);
const TYPE_TO_KIVO: HotkeyId = HotkeyId(4);
/// Esc, registered only while KIVO is busy so it never takes Esc from other apps otherwise.
const CANCEL: HotkeyId = HotkeyId(5);

/// What a hotkey means. Kept separate from the async work so it can be tested without a desktop.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    StartListening,
    StopListening,
    CycleMode,
    StopEverything,
    OpenTextBox,
    Cancel,
    Ignore,
}

pub fn action(event: HotkeyEvent, toggle_mode: bool, listening: bool) -> Action {
    match event {
        // In toggle mode a press starts listening and the next press ends it (VOICE-41).
        HotkeyEvent::Pressed(PUSH_TO_TALK) if toggle_mode && listening => Action::StopListening,
        HotkeyEvent::Pressed(PUSH_TO_TALK) => Action::StartListening,
        HotkeyEvent::Released(PUSH_TO_TALK) if toggle_mode => Action::Ignore,
        HotkeyEvent::Released(PUSH_TO_TALK) => Action::StopListening,
        HotkeyEvent::Pressed(SWITCH_MODE) => Action::CycleMode,
        HotkeyEvent::Pressed(EMERGENCY_STOP) => Action::StopEverything,
        HotkeyEvent::Pressed(TYPE_TO_KIVO) => Action::OpenTextBox,
        HotkeyEvent::Pressed(CANCEL) => Action::Cancel,
        HotkeyEvent::Pressed(_) | HotkeyEvent::Released(_) => Action::Ignore,
    }
}

async fn handle(core: &Core, engine: &Arc<Engine>, event: HotkeyEvent) {
    let config = core.config();
    let listening = engine.is_listening();
    match action(event, config.voice.toggle_mode, listening) {
        Action::StartListening => {
            if let Err(reason) = engine.talk(TurnSource::PushToTalk).await {
                tracing::debug!(reason, "push-to-talk ignored");
            }
        }
        Action::StopListening => engine.release(),
        Action::CycleMode => {
            if let Err(refused) = core.cycle_mode() {
                tracing::debug!(reason = refused.0, "mode switch ignored");
            }
        }
        Action::StopEverything => {
            engine.stop_everything();
            engine.cancel(CancelReason::EmergencyStop);
        }
        // Type to KIVO opens the Island's text box in the app (UX-41).
        Action::OpenTextBox => core.open_control_center(Some("type")),
        // Esc cancels what KIVO is doing (UX-12).
        Action::Cancel => engine.cancel(CancelReason::Hotkey),
        Action::Ignore => {}
    }
}

/// Registers the hotkeys and handles them until KIVO quits.
pub async fn run(
    core: Arc<Core>,
    engine: Arc<Engine>,
    keys: Vec<String>,
    emergency_stop: Vec<String>,
) {
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
        // Another app owns the keys: KIVO says so and the user can rebind in Settings → Shortcuts.
        Err(PlatformError::Conflict(_)) => {
            tracing::warn!(%chord, "another app already uses the push-to-talk keys");
            core.set_hotkey_conflict(Some(chord.to_string()));
        }
        Err(e) => tracing::error!(%e, %chord, "couldn't register push-to-talk"),
    }
    let mode_keys = Chord(vec!["Ctrl".into(), "Shift".into(), "M".into()]);
    if let Err(e) = hotkeys.register(SWITCH_MODE, &mode_keys) {
        tracing::warn!(%e, chord = %mode_keys, "couldn't register the mode hotkey");
    }
    let type_keys = Chord(core.config().voice.type_to_kivo.clone());
    if let Err(e) = hotkeys.register(TYPE_TO_KIVO, &type_keys) {
        tracing::warn!(%e, chord = %type_keys, "couldn't register the type-to-KIVO keys");
    }
    let stop_keys = Chord(emergency_stop);
    if let Err(e) = hotkeys.register(EMERGENCY_STOP, &stop_keys) {
        tracing::error!(%e, chord = %stop_keys, "couldn't register the emergency stop");
    }
    let shutdown = core.shutdown();
    let mut state = core.state();
    let esc = Chord(vec!["Esc".into()]);
    let mut esc_registered = false;
    loop {
        // Esc belongs to KIVO only while a request is in progress.
        let busy = state.borrow_and_update().session.is_active();
        if busy != esc_registered {
            let result = if busy {
                hotkeys.register(CANCEL, &esc)
            } else {
                hotkeys.unregister(CANCEL)
            };
            match result {
                Ok(()) => esc_registered = busy,
                Err(e) => {
                    tracing::debug!(%e, busy, "Esc couldn't be (un)registered");
                    esc_registered = busy;
                }
            }
        }
        tokio::select! {
            Some(event) = events.recv() => handle(&core, &engine, event).await,
            changed = state.changed() => if changed.is_err() { break },
            () = shutdown.cancelled() => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holding_talks_and_releasing_ends_the_utterance() {
        assert_eq!(
            action(HotkeyEvent::Pressed(PUSH_TO_TALK), false, false),
            Action::StartListening
        );
        assert_eq!(
            action(HotkeyEvent::Released(PUSH_TO_TALK), false, true),
            Action::StopListening
        );
    }

    #[test]
    fn toggle_mode_starts_on_one_press_and_ends_on_the_next() {
        assert_eq!(
            action(HotkeyEvent::Pressed(PUSH_TO_TALK), true, false),
            Action::StartListening
        );
        assert_eq!(
            action(HotkeyEvent::Released(PUSH_TO_TALK), true, true),
            Action::Ignore
        );
        assert_eq!(
            action(HotkeyEvent::Pressed(PUSH_TO_TALK), true, true),
            Action::StopListening
        );
    }

    #[test]
    fn the_other_hotkeys_map_to_their_actions() {
        assert_eq!(
            action(HotkeyEvent::Pressed(SWITCH_MODE), false, false),
            Action::CycleMode
        );
        assert_eq!(
            action(HotkeyEvent::Pressed(EMERGENCY_STOP), false, true),
            Action::StopEverything
        );
        assert_eq!(
            action(HotkeyEvent::Pressed(TYPE_TO_KIVO), false, false),
            Action::OpenTextBox
        );
        assert_eq!(
            action(HotkeyEvent::Pressed(HotkeyId(99)), false, false),
            Action::Ignore
        );
    }
}
