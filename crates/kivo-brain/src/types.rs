//! The brain contract's data (BRAINS §3): requests, the events a brain streams back, normalized
//! errors, and what a provider says about itself and its models.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

/// Who wrote a message. The system prompt travels separately (`ChatRequest::system`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    User,
    Assistant,
    /// Results of tool calls, answering the assistant's calls.
    Tool,
}

/// One piece of a message.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum Part {
    Text {
        text: String,
    },
    /// The assistant asked for a tool.
    ToolCall {
        id: String,
        name: String,
        args: Value,
    },
    /// What a tool returned.
    ToolResult {
        id: String,
        name: String,
        content: String,
        #[serde(default)]
        is_error: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub parts: Vec<Part>,
}

impl Message {
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            parts: vec![Part::Text { text: text.into() }],
        }
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            parts: vec![Part::Text { text: text.into() }],
        }
    }

    /// The message's text parts, joined.
    pub fn text(&self) -> String {
        self.parts
            .iter()
            .filter_map(|p| match p {
                Part::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

/// A block of the system prompt. `cacheable` marks the stable prefix (CONV-07): providers that
/// support prompt caching are told to cache up to the last cacheable block.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SystemBlock {
    pub text: String,
    pub cacheable: bool,
}

/// A tool the brain may call, as the provider sees it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    /// JSON Schema of the arguments.
    pub params: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub system: Vec<SystemBlock>,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDef>,
    pub max_tokens: u32,
    pub temperature: Option<f32>,
}

impl ChatRequest {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            system: Vec::new(),
            messages: Vec::new(),
            tools: Vec::new(),
            max_tokens: 1024,
            temperature: None,
        }
    }

    /// The system prompt as one text (for providers without blocks).
    pub fn system_text(&self) -> String {
        self.system
            .iter()
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

/// Tokens a request used (BRAIN-34).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Input tokens read from the provider's prompt cache (billed at a discount).
    pub cached_tokens: u64,
}

impl Usage {
    pub fn add(&mut self, other: Usage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cached_tokens += other.cached_tokens;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StopReason {
    /// The answer is complete.
    EndTurn,
    /// It stopped to call tools; the caller runs them and continues.
    ToolUse,
    MaxTokens,
    Other,
}

/// What a brain streams back.
#[derive(Clone, Debug, PartialEq)]
pub enum BrainEvent {
    TextDelta(String),
    /// Part of a tool call as it arrives (for progress); `ToolCall` follows with the whole call.
    ToolCallDelta {
        index: usize,
        name: Option<String>,
        args_delta: String,
    },
    ToolCall {
        id: String,
        name: String,
        args: Value,
    },
    /// The model's reasoning. Never shown or spoken (BRAIN-09); kept only if the user turns on
    /// the reasoning log.
    Reasoning(String),
    Usage(Usage),
    Done(StopReason),
    Error(NormalizedError),
}

/// Every provider's failures, in one vocabulary (BRAINS §3).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NormalizedError {
    #[error("the sign-in or key was refused")]
    Auth,
    #[error("the provider is rate limiting")]
    RateLimited { retry_after: Option<Duration> },
    #[error("the account is out of credit or quota")]
    Quota,
    #[error("the request is too long for the model")]
    ContextTooLong,
    #[error("the provider's content filter blocked it")]
    ContentFiltered,
    #[error("network problem: {0}")]
    Network(String),
    #[error("the provider is down: {0}")]
    ProviderDown(String),
    #[error("cancelled")]
    Cancelled,
    #[error("{0}")]
    Other(String),
}

impl NormalizedError {
    /// Failures another provider might not have (BRAIN-22 failover).
    pub fn fails_over(&self) -> bool {
        matches!(
            self,
            Self::RateLimited { .. } | Self::ProviderDown(_) | Self::Network(_)
        )
    }

    /// A stable name for logs, activity and the UI.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Auth => "auth",
            Self::RateLimited { .. } => "rateLimited",
            Self::Quota => "quota",
            Self::ContextTooLong => "contextTooLong",
            Self::ContentFiltered => "contentFiltered",
            Self::Network(_) => "network",
            Self::ProviderDown(_) => "providerDown",
            Self::Cancelled => "cancelled",
            Self::Other(_) => "other",
        }
    }
}

/// How a provider is reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderKind {
    Api,
    /// A CLI agent over ACP.
    Cli,
    Local,
    ManagedLogin,
}

/// Where the user's words go (BRAIN-22: failover stays within a class).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PrivacyClass {
    /// Stays on this PC.
    Local,
    /// Goes to the provider.
    Cloud,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    pub privacy: PrivacyClass,
    /// Usable at no cost (local, free tiers; CONV-08).
    pub free: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    /// Tokens, when the provider says.
    pub context_window: Option<u32>,
    pub tools: bool,
    pub vision: bool,
    pub streaming: bool,
    /// Dollars per million tokens, when the provider says (OpenRouter does).
    pub input_price: Option<f64>,
    pub output_price: Option<f64>,
}

impl ModelInfo {
    pub fn named(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            context_window: None,
            tools: true,
            vision: false,
            streaming: true,
            input_price: None,
            output_price: None,
        }
    }
}

/// Is a provider usable right now (BRAIN-23)?
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum Health {
    Ready,
    NeedsSignIn,
    RateLimited,
    Unreachable { reason: String },
}

impl Health {
    pub fn from_error(e: &NormalizedError) -> Self {
        match e {
            NormalizedError::Auth | NormalizedError::Quota => Self::NeedsSignIn,
            NormalizedError::RateLimited { .. } => Self::RateLimited,
            other => Self::Unreachable {
                reason: other.to_string(),
            },
        }
    }

    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }
}
