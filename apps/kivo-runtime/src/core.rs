//! The runtime's authoritative state (ARCHITECTURE §1, §4): the session state machine and the
//! permission mode, published as a `StateSnapshot` that the IPC server pushes to every UI, plus
//! the event bus and the shutdown signal every subsystem listens to. Settings the user changes
//! here are saved to `kivo.toml`.

use kivo_core::config::PermissionMode;
use kivo_core::event::{EventKind, SystemEvent, TurnSource, UiEvent};
use kivo_core::{Event, EventBus, KivoConfig, Session, SessionInput, SessionState};
use kivo_ipc::StateSnapshot;
use kivo_ipc::protocol::{SpeechStatus, StepView, TurnView};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

pub struct Core {
    pub bus: EventBus,
    session: Mutex<Session>,
    config: Mutex<KivoConfig>,
    /// Where the settings are saved; `None` keeps changes in memory (tests).
    config_file: Option<PathBuf>,
    state: watch::Sender<StateSnapshot>,
    shutdown: CancellationToken,
}

/// A core with default settings that are never saved (tests).
#[cfg(test)]
impl Default for Core {
    fn default() -> Self {
        Self::with_config(KivoConfig::default(), None)
    }
}

/// Why a user request was refused (not a bug: the UI or tray raced a state change).
#[derive(Debug, PartialEq, Eq)]
pub struct Refused(pub String);

impl Core {
    /// A core that starts from `config` and saves changes to `config_file`.
    pub fn with_config(config: KivoConfig, config_file: Option<PathBuf>) -> Self {
        let mode = config.permissions.mode;
        Self {
            bus: EventBus::new(),
            session: Mutex::new(Session::new()),
            config: Mutex::new(config),
            config_file,
            state: watch::Sender::new(StateSnapshot {
                session: SessionState::Idle,
                mode,
                island_hidden: false,
                turn: None,
                speech: SpeechStatus::Missing,
                hotkey_conflict: None,
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

    /// Cancels whatever KIVO is doing now (Esc, the Island's Stop button, a hotkey release with
    /// nothing heard). The Island clears.
    pub fn cancel_turn(&self) -> Result<SessionState, Refused> {
        let state = self.state.borrow().session;
        if !state.is_active() && state != SessionState::FollowUp {
            return Err(Refused(format!("KIVO isn't busy ({})", describe(state))));
        }
        self.request(SessionInput::Cancel, "KIVO isn't busy")?;
        let next = if state == SessionState::FollowUp {
            SessionState::Idle
        } else {
            self.request(
                SessionInput::InterruptionHandled { listen: false },
                "KIVO isn't stopping",
            )?
        };
        self.clear_turn();
        Ok(next)
    }

    /// The settings as they are now.
    pub fn config(&self) -> KivoConfig {
        self.config
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Changes settings and saves them; returns the new settings.
    pub fn update_config(&self, change: impl FnOnce(&mut KivoConfig)) -> KivoConfig {
        let updated = {
            let mut config = self.config.lock().unwrap_or_else(|e| e.into_inner());
            change(&mut config);
            config.clone()
        };
        if let Some(file) = &self.config_file
            && let Err(e) = kivo_store::config::save(file, &updated)
        {
            tracing::error!(%e, "couldn't save the settings");
        }
        updated
    }

    /// Whether KIVO can hear right now (the speech model is a download, DIST-12).
    pub fn set_speech_status(&self, status: SpeechStatus) {
        self.state.send_if_modified(|s| {
            if s.speech == status {
                return false;
            }
            s.speech = status;
            s.revision += 1;
            true
        });
    }

    pub fn speech_status(&self) -> SpeechStatus {
        self.state.borrow().speech.clone()
    }

    /// Another app owns the push-to-talk keys (VOICE-41): the UI offers a rebind.
    pub fn set_hotkey_conflict(&self, keys: Option<String>) {
        self.state.send_if_modified(|s| {
            if s.hotkey_conflict == keys {
                return false;
            }
            s.hotkey_conflict = keys;
            s.revision += 1;
            true
        });
    }

    /// An internal transition (the pipeline's own progress, not a user request). An input that
    /// doesn't fit is a bug: `Session` logs it at error and the state is unchanged.
    pub fn advance(&self, input: SessionInput) -> SessionState {
        let mut session = self.session.lock().unwrap_or_else(|e| e.into_inner());
        let next = session.apply(input).unwrap_or_else(|_| session.state());
        drop(session);
        self.state.send_modify(|s| {
            if s.session != next {
                s.session = next;
            }
            s.revision += 1;
        });
        next
    }

    /// Starts a turn: KIVO is listening (or working on typed text), and the Island shows it.
    pub fn begin_turn(
        &self,
        id: &str,
        source: TurnSource,
        transcript: &str,
    ) -> Result<SessionState, Refused> {
        let state = self.request(
            SessionInput::Activate,
            "KIVO can listen only when it isn't busy or paused",
        )?;
        self.state.send_modify(|s| {
            s.turn = Some(TurnView {
                id: id.to_owned(),
                source,
                transcript: transcript.to_owned(),
                transcript_final: !transcript.is_empty(),
                ..TurnView::default()
            });
            s.revision += 1;
        });
        Ok(state)
    }

    /// Changes what the Island shows about the current turn.
    pub fn update_turn(&self, change: impl FnOnce(&mut TurnView)) {
        self.state.send_if_modified(|s| {
            let Some(turn) = s.turn.as_mut() else {
                return false;
            };
            let before = turn.clone();
            change(turn);
            let changed = *turn != before;
            if changed {
                s.revision += 1;
            }
            changed
        });
    }

    /// Adds or updates a step in the Island's list.
    pub fn set_step(&self, step: StepView) {
        self.update_turn(
            |turn| match turn.steps.iter_mut().find(|s| s.id == step.id) {
                Some(existing) => *existing = step,
                None => turn.steps.push(step),
            },
        );
    }

    /// Shows a short message in the Island when there is no turn to attach it to (for example
    /// "speech isn't installed yet"); it clears itself after `for_how_long`. Must run inside
    /// the async runtime.
    pub fn flash_error(self: &std::sync::Arc<Self>, message: &str, for_how_long: Duration) {
        let id = format!("notice-{}", self.state.borrow().revision);
        self.state.send_modify(|s| {
            if s.turn.is_none() {
                s.turn = Some(TurnView {
                    id: id.clone(),
                    error: Some(message.to_owned()),
                    ..TurnView::default()
                });
                s.revision += 1;
            }
        });
        let core = std::sync::Arc::clone(self);
        tokio::spawn(async move {
            tokio::time::sleep(for_how_long).await;
            if core.turn_view().is_some_and(|t| t.id == id) {
                core.clear_turn();
            }
        });
    }

    /// The turn the Island is showing, if any.
    pub fn turn_view(&self) -> Option<TurnView> {
        self.state.borrow().turn.clone()
    }

    /// Clears the Island (the turn ended).
    pub fn clear_turn(&self) {
        self.state.send_if_modified(|s| {
            if s.turn.is_none() {
                return false;
            }
            s.turn = None;
            s.revision += 1;
            true
        });
    }

    /// The end of speech: KIVO starts thinking about what it heard.
    pub fn end_of_speech(&self) -> SessionState {
        if self.state.borrow().session == SessionState::Listening {
            self.advance(SessionInput::EndOfSpeech)
        } else {
            self.state.borrow().session
        }
    }

    /// Switches the permission mode (SECURITY §1.1). Only the user does this, from the UI, the tray
    /// or the hotkey. Bypass needs its own opt-in dialog and expiry (SEC-03), so it is refused
    /// here until that exists.
    pub fn set_mode(&self, mode: PermissionMode) -> Result<PermissionMode, Refused> {
        if mode == PermissionMode::Bypass {
            return Err(Refused(
                "Bypass permissions needs its confirmation step, which isn't available yet".into(),
            ));
        }
        let saved = {
            let mut config = self.config.lock().unwrap_or_else(|e| e.into_inner());
            config.permissions.mode = mode;
            config.clone()
        };
        self.state.send_modify(|s| {
            s.mode = mode;
            s.revision += 1;
        });
        if let Some(file) = &self.config_file
            && let Err(e) = kivo_store::config::save(file, &saved)
        {
            tracing::error!(%e, "couldn't save the permission mode");
        }
        tracing::info!(?mode, "permission mode changed");
        Ok(mode)
    }

    /// Stop everything (SEC-25: tray, emergency hotkey, Island button): whatever KIVO is doing now
    /// is cancelled. Background tasks join this when they exist (M5).
    pub fn stop_everything(&self) {
        tracing::warn!("stop everything");
        let mut session = self.session.lock().unwrap_or_else(|e| e.into_inner());
        let state = session.state();
        let cancelled = if state.is_active() {
            session
                .apply(SessionInput::Cancel)
                .and_then(|_| session.apply(SessionInput::InterruptionHandled { listen: false }))
                .ok()
        } else if state == SessionState::FollowUp {
            session.apply(SessionInput::Cancel).ok()
        } else {
            None
        };
        if let Some(next) = cancelled {
            self.state.send_modify(|s| {
                s.session = next;
                s.turn = None;
                s.revision += 1;
            });
        }
    }

    /// "Hide Island for 1 hour" (UX §1): the Island shows nothing for `duration`, then comes back
    /// by itself. Must run inside the async runtime (it sets a timer).
    pub fn hide_island_for(self: &std::sync::Arc<Self>, duration: Duration) {
        self.set_island_hidden(true);
        let core = std::sync::Arc::clone(self);
        let shutdown = self.shutdown();
        tokio::spawn(async move {
            tokio::select! {
                () = tokio::time::sleep(duration) => core.set_island_hidden(false),
                () = shutdown.cancelled() => {}
            }
        });
    }

    pub fn set_island_hidden(&self, hidden: bool) {
        self.state.send_if_modified(|s| {
            if s.island_hidden == hidden {
                return false;
            }
            s.island_hidden = hidden;
            s.revision += 1;
            true
        });
    }

    /// Ctrl+Shift+M: the next everyday mode.
    pub fn cycle_mode(&self) -> Result<PermissionMode, Refused> {
        let current = self.state.borrow().mode;
        self.set_mode(current.next())
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
        let core = Core::default();
        let state = core.state();
        assert_eq!(core.pause(), Ok(SessionState::Paused));
        assert_eq!(
            *state.borrow(),
            StateSnapshot {
                session: SessionState::Paused,
                mode: PermissionMode::Auto,
                island_hidden: false,
                turn: None,
                speech: SpeechStatus::Missing,
                hotkey_conflict: None,
                revision: 1
            }
        );
        assert_eq!(core.resume(), Ok(SessionState::Idle));
        assert_eq!(state.borrow().revision, 2);
    }

    #[test]
    fn requests_that_dont_fit_the_state_are_refused_without_changing_it() {
        let core = Core::default();
        let refused = core.resume().unwrap_err();
        assert_eq!(refused, Refused("KIVO isn't paused (Ready)".into()));
        core.pause().unwrap();
        assert!(core.pause().is_err());
        assert_eq!(core.session(), SessionState::Paused);
        assert_eq!(core.state().borrow().revision, 1);
    }

    #[test]
    fn push_to_talk_listens_while_held() {
        let core = Core::default();
        let mut state = core.state();
        assert_eq!(
            core.begin_turn("t1", TurnSource::PushToTalk, ""),
            Ok(SessionState::Listening)
        );
        assert!(
            core.begin_turn("t1", TurnSource::PushToTalk, "").is_err(),
            "already listening"
        );
        assert_eq!(core.cancel_turn(), Ok(SessionState::Idle));
        assert!(core.cancel_turn().is_err(), "not listening any more");
        assert_eq!(
            state.borrow_and_update().revision,
            5,
            "listening, the Island's turn, stopping, idle, the Island cleared"
        );

        core.pause().unwrap();
        assert!(
            core.begin_turn("t2", TurnSource::PushToTalk, "").is_err(),
            "a paused KIVO doesn't listen"
        );
    }

    #[test]
    fn the_mode_changes_are_published_and_saved() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("kivo.toml");
        let core = Core::with_config(KivoConfig::default(), Some(file.clone()));
        assert_eq!(
            core.set_mode(PermissionMode::Plan),
            Ok(PermissionMode::Plan)
        );
        assert_eq!(core.state().borrow().mode, PermissionMode::Plan);
        let saved = kivo_store::config::load(&file).unwrap().config;
        assert_eq!(saved.permissions.mode, PermissionMode::Plan);
        assert_eq!(core.cycle_mode(), Ok(PermissionMode::Auto));
    }

    #[test]
    fn stop_everything_ends_what_kivo_is_doing() {
        let core = Core::default();
        core.stop_everything();
        assert_eq!(
            core.session(),
            SessionState::Idle,
            "nothing to stop is fine"
        );
        core.begin_turn("t9", TurnSource::PushToTalk, "").unwrap();
        core.stop_everything();
        assert_eq!(core.session(), SessionState::Idle);
    }

    #[tokio::test(start_paused = true)]
    async fn the_island_comes_back_after_being_hidden() {
        let core = std::sync::Arc::new(Core::default());
        core.hide_island_for(Duration::from_secs(3600));
        assert!(core.state().borrow().island_hidden);
        tokio::time::sleep(Duration::from_secs(3601)).await;
        assert!(
            !core.state().borrow().island_hidden,
            "shown again after the hour"
        );
        core.hide_island_for(Duration::from_secs(3600));
        core.set_island_hidden(false);
        assert!(
            !core.state().borrow().island_hidden,
            "\"Show the Island\" ends it early"
        );
    }

    #[test]
    fn bypass_is_never_switched_on_directly() {
        let core = Core::default();
        assert!(core.set_mode(PermissionMode::Bypass).is_err());
        assert_eq!(core.state().borrow().mode, PermissionMode::Auto);
    }

    #[tokio::test]
    async fn quit_announces_shutdown_once_then_cancels() {
        let core = Core::default();
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
        let core = Core::default();
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
