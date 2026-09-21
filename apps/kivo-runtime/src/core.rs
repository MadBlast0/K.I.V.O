//! The runtime's authoritative state (ARCHITECTURE §1, §4): the session state machine, published
//! as a `StateSnapshot` that the IPC server pushes to every UI, plus the event bus and the
//! shutdown signal every subsystem listens to.

use kivo_core::event::{EventKind, SystemEvent, UiEvent};
use kivo_core::{Event, EventBus, Session, SessionInput, SessionState};
use kivo_ipc::StateSnapshot;
use std::sync::Mutex;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

pub struct Core {
    pub bus: EventBus,
    session: Mutex<Session>,
    state: watch::Sender<StateSnapshot>,
    shutdown: CancellationToken,
}

/// Why a user request was refused (not a bug: the UI or tray raced a state change).
#[derive(Debug, PartialEq, Eq)]
pub struct Refused(pub String);

impl Core {
    pub fn new() -> Self {
        Self {
            bus: EventBus::new(),
            session: Mutex::new(Session::new()),
            state: watch::Sender::new(StateSnapshot {
                session: SessionState::Idle,
                revision: 0,
            }),
            shutdown: CancellationToken::new(),
        }
    }

    /// Live state; every change is a new snapshot.
    pub fn state(&self) -> watch::Receiver<StateSnapshot> {
        self.state.subscribe()
    }

    #[cfg(test)]
    pub fn session(&self) -> SessionState {
        self.state.borrow().session
    }

    /// Cancelled when KIVO quits.
    pub fn shutdown(&self) -> CancellationToken {
        self.shutdown.clone()
    }

    /// Applies an input the user asked for. Unlike an internal transition, a request that doesn't
    /// fit the current state is refused with a reason instead of being logged as a bug.
    fn request(&self, input: SessionInput, refusal: &str) -> Result<SessionState, Refused> {
        let mut session = self.session.lock().unwrap_or_else(|e| e.into_inner());
        if session.state().next(input).is_err() {
            return Err(Refused(format!(
                "{refusal} ({})",
                describe(session.state())
            )));
        }
        let next = session.apply(input).map_err(|e| Refused(e.to_string()))?;
        self.state.send_modify(|s| {
            s.session = next;
            s.revision += 1;
        });
        Ok(next)
    }

    /// "Pause listening": the mic is released until resumed (UX §1).
    pub fn pause(&self) -> Result<SessionState, Refused> {
        self.request(
            SessionInput::Pause,
            "KIVO can pause only while it isn't busy",
        )
    }

    pub fn resume(&self) -> Result<SessionState, Refused> {
        self.request(SessionInput::Resume, "KIVO isn't paused")
    }

    /// Push-to-talk pressed: start listening (VOICE-41).
    pub fn start_listening(&self) -> Result<SessionState, Refused> {
        self.request(
            SessionInput::Activate,
            "KIVO can listen only when it isn't busy or paused",
        )
    }

    /// Push-to-talk released. Speech recognition joins in M1; until then there is nothing to do
    /// with what was heard, so the turn ends as cancelled instead of pretending to think.
    pub fn stop_listening(&self) -> Result<SessionState, Refused> {
        if self.state.borrow().session != SessionState::Listening {
            return Err(Refused("KIVO isn't listening".into()));
        }
        self.request(SessionInput::Cancel, "KIVO isn't listening")?;
        self.request(
            SessionInput::InterruptionHandled { listen: false },
            "KIVO isn't stopping",
        )
    }

    /// Asks the app to show the Control Center, optionally on a page. If the app isn't running,
    /// the supervisor launches it for this.
    pub fn open_control_center(&self, page: Option<&str>) {
        self.bus
            .publish(Event::new(EventKind::Ui(UiEvent::ControlCenterRequested {
                page: page.map(str::to_owned),
            })));
    }

    /// Quit KIVO: tell the UIs, then stop everything (the shutdown order is in `main`).
    pub fn quit(&self) {
        if self.shutdown.is_cancelled() {
            return;
        }
        tracing::info!("quitting");
        self.bus
            .publish(Event::new(EventKind::System(SystemEvent::ShuttingDown)));
        self.shutdown.cancel();
    }
}

/// A short, user-facing name for a state (tray tooltip, refusals).
pub fn describe(state: SessionState) -> &'static str {
    match state {
        SessionState::Idle => "Ready",
        SessionState::Listening => "Listening",
        SessionState::Thinking => "Thinking",
        SessionState::Acting => "Working",
        SessionState::Speaking => "Speaking",
        SessionState::FollowUp => "Listening for a follow-up",
        SessionState::Interrupted => "Stopping",
        SessionState::Paused => "Paused",
        SessionState::AwaitingConfirmation => "Waiting for your OK",
        SessionState::Error => "Something went wrong",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kivo_core::Received;

    #[test]
    fn pause_and_resume_publish_new_snapshots() {
        let core = Core::new();
        let state = core.state();
        assert_eq!(core.pause(), Ok(SessionState::Paused));
        assert_eq!(
            *state.borrow(),
            StateSnapshot {
                session: SessionState::Paused,
                revision: 1
            }
        );
        assert_eq!(core.resume(), Ok(SessionState::Idle));
        assert_eq!(state.borrow().revision, 2);
    }

    #[test]
    fn requests_that_dont_fit_the_state_are_refused_without_changing_it() {
        let core = Core::new();
        let refused = core.resume().unwrap_err();
        assert_eq!(refused, Refused("KIVO isn't paused (Ready)".into()));
        core.pause().unwrap();
        assert!(core.pause().is_err());
        assert_eq!(core.session(), SessionState::Paused);
        assert_eq!(core.state().borrow().revision, 1);
    }

    #[test]
    fn push_to_talk_listens_while_held() {
        let core = Core::new();
        let mut state = core.state();
        assert_eq!(core.start_listening(), Ok(SessionState::Listening));
        assert!(core.start_listening().is_err(), "already listening");
        assert_eq!(core.stop_listening(), Ok(SessionState::Idle));
        assert!(core.stop_listening().is_err(), "not listening any more");
        assert_eq!(
            state.borrow_and_update().revision,
            3,
            "listening, stopping, idle"
        );

        core.pause().unwrap();
        assert!(
            core.start_listening().is_err(),
            "a paused KIVO doesn't listen"
        );
    }

    #[tokio::test]
    async fn quit_announces_shutdown_once_then_cancels() {
        let core = Core::new();
        let mut events = core.bus.subscribe();
        let shutdown = core.shutdown();
        core.quit();
        core.quit();
        assert!(shutdown.is_cancelled());
        let Received::Event(event) = events.recv().await else {
            panic!("expected the ShuttingDown event");
        };
        assert_eq!(event.kind, EventKind::System(SystemEvent::ShuttingDown));
        let again = tokio::time::timeout(std::time::Duration::from_millis(50), events.recv()).await;
        assert!(again.is_err(), "announced only once");
    }

    #[tokio::test]
    async fn opening_the_control_center_is_an_event_for_the_app() {
        let core = Core::new();
        let mut events = core.bus.subscribe();
        core.open_control_center(Some("settings"));
        let Received::Event(event) = events.recv().await else {
            panic!("expected an event");
        };
        assert_eq!(
            event.kind,
            EventKind::Ui(UiEvent::ControlCenterRequested {
                page: Some("settings".into())
            })
        );
    }
}
