//! A turn: one request from start to response (ARCHITECTURE §4.2). It owns a root cancellation
//! token; each stage (STT stream, brain request, every tool call, TTS) takes a child token, so
//! cancelling the turn reaches every layer at once, while a stage can be cancelled on its own.

use crate::event::TurnSource;
use crate::ids::TurnId;
use crate::time::Timestamp;
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
pub struct Turn {
    id: TurnId,
    source: TurnSource,
    started: Timestamp,
    root: CancellationToken,
}

impl Turn {
    pub fn start(source: TurnSource) -> Self {
        Self {
            id: TurnId::new(),
            source,
            started: Timestamp::now(),
            root: CancellationToken::new(),
        }
    }

    pub fn id(&self) -> TurnId {
        self.id
    }

    pub fn source(&self) -> TurnSource {
        self.source
    }

    pub fn started(&self) -> Timestamp {
        self.started
    }

    /// A token for one stage. Cancelled when the turn is; cancelling it doesn't cancel the turn.
    pub fn stage_token(&self) -> CancellationToken {
        self.root.child_token()
    }

    /// Cancels the turn and every stage.
    pub fn cancel(&self) {
        self.root.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.root.is_cancelled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelling_the_turn_cancels_every_stage() {
        let turn = Turn::start(TurnSource::PushToTalk);
        let stt = turn.stage_token();
        let tts = turn.stage_token();
        turn.cancel();
        assert!(turn.is_cancelled() && stt.is_cancelled() && tts.is_cancelled());
    }

    #[test]
    fn cancelling_a_stage_leaves_the_turn_running() {
        let turn = Turn::start(TurnSource::WakeWord);
        let tool = turn.stage_token();
        let tts = turn.stage_token();
        tool.cancel();
        assert!(!turn.is_cancelled() && !tts.is_cancelled());
    }

    #[test]
    fn stages_created_after_cancel_start_cancelled() {
        let turn = Turn::start(TurnSource::Typed);
        turn.cancel();
        assert!(turn.stage_token().is_cancelled());
    }

    #[tokio::test]
    async fn a_waiting_stage_wakes_when_the_turn_is_cancelled() {
        let turn = Turn::start(TurnSource::Routine);
        let stage = turn.stage_token();
        let waiter = tokio::spawn(async move { stage.cancelled().await });
        turn.cancel();
        waiter.await.unwrap();
    }
}
