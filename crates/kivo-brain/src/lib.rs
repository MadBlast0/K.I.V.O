//! KIVO's brains (BRAINS.md): the `BrainProvider` contract, the API adapters (Anthropic, OpenAI,
//! Gemini, OpenRouter and every OpenAI-compatible service, local servers), CLI agents over ACP,
//! profiles and routing, context assembly, and the speech side of answers. The runtime owns
//! which brains exist; `kivo-core` stays free of all of this (ARCH-38).

pub mod acp;
pub mod anthropic;
pub mod catalog;
pub mod computer;
pub mod context;
pub mod cost;
pub mod gemini;
pub mod http;
pub mod openai;
pub mod persona;
pub mod provider;
pub mod realtime;
pub mod reasoning;
pub mod routing;
pub mod speech;
pub mod sse;
pub mod testing;
pub mod types;

pub use provider::{BrainProvider, BrainStream, Collected, collect};
pub use types::{
    BrainEvent, ChatRequest, Health, Message, ModelInfo, NormalizedError, Part, PrivacyClass,
    ProviderInfo, ProviderKind, Role, StopReason, SystemBlock, ToolDef, Usage,
};

#[cfg(test)]
mod acp_tests;
#[cfg(test)]
mod contract_tests;
