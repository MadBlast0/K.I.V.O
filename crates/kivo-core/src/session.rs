//! The assistant session state machine (ARCHITECTURE §4.2). The runtime owns it; UIs render the
//! `SessionState` they receive. It drives the Island, the companion, sounds and the tray icon.
//!
//! ```text
//! Idle ─activate─► Listening ─endOfSpeech─► Thinking ─┬─► Acting ─┬─► Speaking ─► FollowUp ─timeout─► Idle
//!                                                     └───────────┴───► (finished) ─► Idle
//! Listening/Thinking/Acting/Speaking/AwaitingConfirmation ─cancel─► Interrupted ─► Idle | Listening
//! Thinking/Acting ─needConfirmation─► AwaitingConfirmation ─confirmed─► Acting | ─denied─► Idle
//! Idle/FollowUp ─pause─► Paused ─resume─► Idle        any active state ─fail─► Error ─errorShown─► Idle
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionState {
    Idle,
    Listening,
    Thinking,
    Acting,
    Speaking,
    /// After an answer: listening for a follow-up without the wake word (UX §8.1).
    FollowUp,
    /// A turn was cancelled; the runtime is winding it down.
    Interrupted,
    /// Listening is off (mic released).
    Paused,
    AwaitingConfirmation,
    Error,
}

/// What happened, from the state machine's point of view.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionInput {
    /// Wake word, push-to-talk, typed request, or speech during a follow-up.
    Activate,
    EndOfSpeech,
    StartActing,
    StartSpeaking,
    SpeechFinished,
    /// Work finished with nothing to say (for example, speech replies are off).
    Finished,
    FollowUpTimeout,
    /// Stop, Esc, the overlay X, the emergency stop or barge-in.
    Cancel,
    /// The interrupted turn is wound down. `listen` starts a new turn (barge-in).
    InterruptionHandled {
        listen: bool,
    },
    NeedConfirmation,
    Confirmed,
    Denied,
    Pause,
    Resume,
    Fail,
    ErrorShown,
    /// A realtime conversation listens again after the model's turn or a decision (BRAIN-33).
    LiveListening,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid session transition: {input:?} in state {from:?}")]
pub struct InvalidTransition {
    pub from: SessionState,
    pub input: SessionInput,
}

impl SessionState {
    /// The state after `input`, or an error if the input makes no sense in this state.
    pub fn next(self, input: SessionInput) -> Result<SessionState, InvalidTransition> {
        use SessionInput as I;
        use SessionState as S;
        let next = match (self, input) {
            (S::Idle | S::FollowUp, I::Activate) => S::Listening,
            (S::Listening, I::EndOfSpeech) => S::Thinking,
            // A brain's streamed answer can start speaking before it calls a tool (BRAIN-28).
            (S::Thinking | S::Speaking, I::StartActing) => S::Acting,
            (S::Thinking | S::Acting, I::StartSpeaking) => S::Speaking,
            (S::Speaking, I::SpeechFinished) => S::FollowUp,
            (S::Thinking | S::Acting, I::Finished) => S::Idle,
            (S::FollowUp, I::FollowUpTimeout) => S::Idle,
            (
                S::Listening | S::Thinking | S::Acting | S::Speaking | S::AwaitingConfirmation,
                I::Cancel,
            ) => S::Interrupted,
            (S::FollowUp, I::Cancel) => S::Idle,
            (S::Interrupted, I::InterruptionHandled { listen: true }) => S::Listening,
            (S::Interrupted, I::InterruptionHandled { listen: false }) => S::Idle,
            (S::Thinking | S::Acting | S::Speaking, I::NeedConfirmation) => S::AwaitingConfirmation,
            (S::AwaitingConfirmation, I::Confirmed) => S::Acting,
            (S::AwaitingConfirmation, I::Denied) => S::Idle,
            (S::Idle | S::FollowUp, I::Pause) => S::Paused,
            (S::Paused, I::Resume) => S::Idle,
            (
                S::Listening | S::Thinking | S::Acting | S::Speaking | S::AwaitingConfirmation,
                I::Fail,
            ) => S::Error,
            (S::Error, I::ErrorShown) => S::Idle,
            (S::Idle | S::Thinking | S::Acting | S::Speaking, I::LiveListening) => S::Listening,
            (from, input) => return Err(InvalidTransition { from, input }),
        };
        Ok(next)
    }

    /// True while a turn is in progress (a cancel has something to stop).
    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Listening
                | Self::Thinking
                | Self::Acting
                | Self::Speaking
                | Self::AwaitingConfirmation
        )
    }
}

impl fmt::Display for SessionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// Holds the current state and applies inputs. An invalid transition is a bug: it is logged at
/// `error` and the state is left unchanged.
#[derive(Debug)]
pub struct Session {
    state: SessionState,
}

impl Session {
    pub fn new() -> Self {
        Self {
            state: SessionState::Idle,
        }
    }

    pub fn state(&self) -> SessionState {
        self.state
    }

    pub fn apply(&mut self, input: SessionInput) -> Result<SessionState, InvalidTransition> {
        match self.state.next(input) {
            Ok(next) => {
                tracing::debug!(from = %self.state, to = %next, ?input, "session transition");
                self.state = next;
                Ok(next)
            }
            Err(err) => {
                tracing::error!(%err, "invalid session transition");
                Err(err)
            }
        }
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use SessionInput as I;
    use SessionState as S;

    const STATES: [S; 10] = [
        S::Idle,
        S::Listening,
        S::Thinking,
        S::Acting,
        S::Speaking,
        S::FollowUp,
        S::Interrupted,
        S::Paused,
        S::AwaitingConfirmation,
        S::Error,
    ];
    const INPUTS: [I; 17] = [
        I::Activate,
        I::EndOfSpeech,
        I::StartActing,
        I::StartSpeaking,
        I::SpeechFinished,
        I::Finished,
        I::FollowUpTimeout,
        I::Cancel,
        I::InterruptionHandled { listen: true },
        I::InterruptionHandled { listen: false },
        I::NeedConfirmation,
        I::Confirmed,
        I::Denied,
        I::Pause,
        I::Resume,
        I::Fail,
        I::ErrorShown,
    ];

    /// Every valid transition, written out independently of `next` (the spec, as a table).
    const VALID: [(S, I, S); 33] = [
        (S::Idle, I::Activate, S::Listening),
        (S::FollowUp, I::Activate, S::Listening),
        (S::Listening, I::EndOfSpeech, S::Thinking),
        (S::Thinking, I::StartActing, S::Acting),
        (S::Speaking, I::StartActing, S::Acting),
        (S::Thinking, I::StartSpeaking, S::Speaking),
        (S::Acting, I::StartSpeaking, S::Speaking),
        (S::Speaking, I::SpeechFinished, S::FollowUp),
        (S::Thinking, I::Finished, S::Idle),
        (S::Acting, I::Finished, S::Idle),
        (S::FollowUp, I::FollowUpTimeout, S::Idle),
        (S::Listening, I::Cancel, S::Interrupted),
        (S::Thinking, I::Cancel, S::Interrupted),
        (S::Acting, I::Cancel, S::Interrupted),
        (S::Speaking, I::Cancel, S::Interrupted),
        (S::AwaitingConfirmation, I::Cancel, S::Interrupted),
        (S::FollowUp, I::Cancel, S::Idle),
        (
            S::Interrupted,
            I::InterruptionHandled { listen: true },
            S::Listening,
        ),
        (
            S::Interrupted,
            I::InterruptionHandled { listen: false },
            S::Idle,
        ),
        (S::Thinking, I::NeedConfirmation, S::AwaitingConfirmation),
        (S::Acting, I::NeedConfirmation, S::AwaitingConfirmation),
        (S::Speaking, I::NeedConfirmation, S::AwaitingConfirmation),
        (S::AwaitingConfirmation, I::Confirmed, S::Acting),
        (S::AwaitingConfirmation, I::Denied, S::Idle),
        (S::Idle, I::Pause, S::Paused),
        (S::FollowUp, I::Pause, S::Paused),
        (S::Paused, I::Resume, S::Idle),
        (S::Listening, I::Fail, S::Error),
        (S::Thinking, I::Fail, S::Error),
        (S::Acting, I::Fail, S::Error),
        (S::Speaking, I::Fail, S::Error),
        (S::AwaitingConfirmation, I::Fail, S::Error),
        (S::Error, I::ErrorShown, S::Idle),
    ];

    #[test]
    fn every_pair_matches_the_table_exactly() {
        for from in STATES {
            for input in INPUTS {
                let expected = VALID
                    .iter()
                    .find(|(f, i, _)| *f == from && *i == input)
                    .map(|(_, _, to)| *to);
                match (from.next(input), expected) {
                    (Ok(got), Some(want)) => assert_eq!(got, want, "{from:?} + {input:?}"),
                    (Err(e), None) => assert_eq!(e, InvalidTransition { from, input }),
                    (got, want) => panic!("{from:?} + {input:?}: got {got:?}, table says {want:?}"),
                }
            }
        }
    }

    #[test]
    fn a_voice_command_with_a_spoken_reply_goes_round_to_idle() {
        let mut s = Session::new();
        for input in [
            I::Activate,
            I::EndOfSpeech,
            I::StartActing,
            I::StartSpeaking,
            I::SpeechFinished,
            I::FollowUpTimeout,
        ] {
            s.apply(input).unwrap();
        }
        assert_eq!(s.state(), S::Idle);
    }

    #[test]
    fn barge_in_starts_a_new_turn() {
        let mut s = Session::new();
        for input in [I::Activate, I::EndOfSpeech, I::StartSpeaking, I::Cancel] {
            s.apply(input).unwrap();
        }
        assert_eq!(s.state(), S::Interrupted);
        assert_eq!(
            s.apply(I::InterruptionHandled { listen: true }),
            Ok(S::Listening)
        );
    }

    #[test]
    fn an_invalid_input_leaves_the_state_unchanged() {
        let mut s = Session::new();
        let err = s.apply(I::SpeechFinished).unwrap_err();
        assert_eq!(err.from, S::Idle);
        assert_eq!(s.state(), S::Idle);
    }

    #[test]
    fn active_states_are_exactly_the_cancellable_ones() {
        for state in STATES {
            let cancel_interrupts = state.next(I::Cancel) == Ok(S::Interrupted);
            assert_eq!(state.is_active(), cancel_interrupts, "{state:?}");
        }
    }
}
