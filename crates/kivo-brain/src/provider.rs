//! `BrainProvider` (BRAINS §3, BRAIN-08): what every brain adapter implements. A chat answers
//! with a stream of `BrainEvent`s on a channel; cancelling the token stops the provider's request
//! and ends the stream with `Error(Cancelled)`.

use crate::types::{BrainEvent, ChatRequest, Health, ModelInfo, NormalizedError, ProviderInfo};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// The events of one chat, in order; the last is `Done` or `Error`.
pub type BrainStream = mpsc::Receiver<BrainEvent>;

/// Enough room for a burst of deltas while the consumer speaks a phrase.
pub const STREAM_CAPACITY: usize = 256;

#[async_trait::async_trait]
pub trait BrainProvider: Send + Sync {
    fn info(&self) -> ProviderInfo;

    /// Reachable, signed in, not rate limited? Cheap: listing models is enough.
    async fn health(&self) -> Health {
        match self.models().await {
            Ok(_) => Health::Ready,
            Err(e) => Health::from_error(&e),
        }
    }

    async fn models(&self) -> Result<Vec<ModelInfo>, NormalizedError>;

    /// Starts a chat. Failures arrive in the stream as `Error`, never as a panic.
    fn chat(&self, request: ChatRequest, cancel: CancellationToken) -> BrainStream;
}

/// A stream and its sender, for adapters.
pub fn channel() -> (mpsc::Sender<BrainEvent>, BrainStream) {
    mpsc::channel(STREAM_CAPACITY)
}

/// Collects a whole answer (tests, summaries): the text, the tool calls, usage and the ending.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Collected {
    pub text: String,
    pub tool_calls: Vec<(String, String, serde_json::Value)>,
    pub reasoning: String,
    /// Signed thinking blocks (Anthropic), to send back with the tool calls.
    pub thinking: Vec<crate::types::Part>,
    pub usage: crate::types::Usage,
    pub stop: Option<crate::types::StopReason>,
    pub error: Option<NormalizedError>,
}

pub async fn collect(mut stream: BrainStream) -> Collected {
    let mut out = Collected::default();
    while let Some(event) = stream.recv().await {
        match event {
            BrainEvent::TextDelta(t) => out.text.push_str(&t),
            BrainEvent::ToolCall { id, name, args } => out.tool_calls.push((id, name, args)),
            BrainEvent::ToolCallDelta { .. } => {}
            BrainEvent::Reasoning(r) => out.reasoning.push_str(&r),
            BrainEvent::Thinking {
                text,
                signature,
                redacted,
            } => out.thinking.push(crate::types::Part::Thinking {
                text,
                signature,
                redacted,
            }),
            BrainEvent::Usage(u) => out.usage.add(u),
            BrainEvent::Done(stop) => out.stop = Some(stop),
            BrainEvent::Error(e) => out.error = Some(e),
        }
    }
    out
}
