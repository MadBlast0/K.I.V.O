//! OpenAI-compatible chat completions (BRAIN-10, BRAIN-12): OpenAI itself, OpenRouter, Groq,
//! Mistral, DeepSeek, xAI, and local servers (Ollama, LM Studio, llama.cpp, any compatible
//! localhost URL). Streaming with tool calls, usage (including cached input) and reasoning deltas.
//! OpenAI caches long prompt prefixes by itself, so the stable system blocks simply go first
//! (CONV-07).

use crate::http::{Decode, Header, Http};
use crate::provider::{BrainProvider, BrainStream, channel};
use crate::sse::SseEvent;
use crate::types::{
    BrainEvent, ChatRequest, ModelInfo, NormalizedError, Part, PrivacyClass, ProviderInfo,
    ProviderKind, Role, StopReason, Usage,
};
use kivo_core::Secret;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// How one OpenAI-compatible service is reached.
#[derive(Clone, Debug)]
pub struct OpenAiConfig {
    pub id: String,
    pub name: String,
    /// Up to and including the version, e.g. `https://api.openai.com/v1`.
    pub base_url: String,
    pub kind: ProviderKind,
    pub privacy: PrivacyClass,
    pub free: bool,
    /// OpenAI's newer models take `max_completion_tokens`; most others `max_tokens`.
    pub max_completion_tokens: bool,
    /// Extra headers (OpenRouter's app attribution).
    pub extra_headers: Vec<(&'static str, String)>,
}

impl OpenAiConfig {
    pub fn openai() -> Self {
        Self {
            id: "openai".into(),
            name: "OpenAI".into(),
            base_url: "https://api.openai.com/v1".into(),
            kind: ProviderKind::Api,
            privacy: PrivacyClass::Cloud,
            free: false,
            max_completion_tokens: true,
            extra_headers: Vec::new(),
        }
    }

    pub fn openrouter() -> Self {
        Self {
            id: "openrouter".into(),
            name: "OpenRouter".into(),
            base_url: "https://openrouter.ai/api/v1".into(),
            kind: ProviderKind::Api,
            privacy: PrivacyClass::Cloud,
            // Free models exist; paid ones are the user's choice (CONV-08).
            free: true,
            max_completion_tokens: false,
            extra_headers: vec![
                (
                    "http-referer",
                    "https://github.com/MadBlast0/K.I.V.O".into(),
                ),
                ("x-title", "KIVO".into()),
            ],
        }
    }

    /// A server on this PC (Ollama `http://127.0.0.1:11434/v1`, LM Studio `:1234/v1`, …).
    pub fn local(
        id: impl Into<String>,
        name: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            base_url: base_url.into(),
            kind: ProviderKind::Local,
            privacy: PrivacyClass::Local,
            free: true,
            max_completion_tokens: false,
            extra_headers: Vec::new(),
        }
    }

    /// Any other OpenAI-compatible cloud service (Groq, Mistral, DeepSeek, xAI, custom).
    pub fn compatible(
        id: impl Into<String>,
        name: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            base_url: base_url.into(),
            kind: ProviderKind::Api,
            privacy: PrivacyClass::Cloud,
            free: false,
            max_completion_tokens: false,
            extra_headers: Vec::new(),
        }
    }
}

pub struct OpenAi {
    config: OpenAiConfig,
    key: Option<Arc<Secret<String>>>,
    http: Http,
}

impl OpenAi {
    /// `key` is loaded by the runtime from the credential store; local servers need none.
    pub fn new(config: OpenAiConfig, key: Option<Secret<String>>, http: Http) -> Self {
        Self {
            config,
            key: key.map(Arc::new),
            http,
        }
    }

    fn headers(config: &OpenAiConfig, key: Option<&Secret<String>>) -> Vec<Header> {
        let mut headers: Vec<Header> = config
            .extra_headers
            .iter()
            .map(|(n, v)| Header::plain(n, v.clone()))
            .collect();
        if let Some(key) = key {
            headers.push(Header::secret(
                "authorization",
                format!("Bearer {}", key.expose()),
            ));
        }
        headers
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{path}", self.config.base_url.trim_end_matches('/'))
    }
}

/// The request body for `request`.
pub fn body(config: &OpenAiConfig, request: &ChatRequest) -> Value {
    let mut messages = Vec::new();
    let system = request.system_text();
    if !system.is_empty() {
        messages.push(json!({ "role": "system", "content": system }));
    }
    for message in &request.messages {
        match message.role {
            Role::User => messages.push(json!({ "role": "user", "content": message.text() })),
            Role::Assistant => {
                let calls: Vec<Value> = message
                    .parts
                    .iter()
                    .filter_map(|p| match p {
                        Part::ToolCall { id, name, args } => Some(json!({
                            "id": id,
                            "type": "function",
                            "function": { "name": name, "arguments": args.to_string() },
                        })),
                        _ => None,
                    })
                    .collect();
                let mut m = json!({ "role": "assistant", "content": message.text() });
                if !calls.is_empty() {
                    m["tool_calls"] = Value::Array(calls);
                }
                messages.push(m);
            }
            Role::Tool => {
                for part in &message.parts {
                    if let Part::ToolResult { id, content, .. } = part {
                        messages.push(
                            json!({ "role": "tool", "tool_call_id": id, "content": content }),
                        );
                    }
                }
            }
        }
    }
    let mut body = json!({
        "model": request.model,
        "messages": messages,
        "stream": true,
        "stream_options": { "include_usage": true },
    });
    let limit = if config.max_completion_tokens {
        "max_completion_tokens"
    } else {
        "max_tokens"
    };
    body[limit] = json!(request.max_tokens);
    if let Some(t) = request.temperature {
        body["temperature"] = json!(t);
    }
    if !request.tools.is_empty() {
        body["tools"] = Value::Array(
            request
                .tools
                .iter()
                .map(|t| {
                    json!({
                        "type": "function",
                        "function": { "name": t.name, "description": t.description, "parameters": t.params },
                    })
                })
                .collect(),
        );
    }
    body
}

/// Turns streamed chunks into `BrainEvent`s, assembling tool calls from their pieces.
#[derive(Default)]
pub struct Decoder {
    calls: BTreeMap<usize, (String, String, String)>,
    stop: Option<StopReason>,
}

impl Decode for Decoder {
    fn decode(&mut self, event: &SseEvent, out: &mut Vec<BrainEvent>) {
        let data = event.data.as_str();
        if data.trim() == "[DONE]" {
            return;
        }
        let Ok(chunk) = serde_json::from_str::<Value>(data) else {
            return;
        };
        if let Some(error) = chunk.get("error") {
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("error")
                .to_owned();
            let code = error.get("code").and_then(Value::as_u64).unwrap_or(500);
            out.push(BrainEvent::Error(crate::http::error_for(
                u16::try_from(code).unwrap_or(500),
                None,
                &json!({ "error": { "message": message } }).to_string(),
            )));
            return;
        }
        if let Some(choice) = chunk.pointer("/choices/0") {
            let delta = &choice["delta"];
            for key in ["reasoning", "reasoning_content"] {
                if let Some(r) = delta
                    .get(key)
                    .and_then(Value::as_str)
                    .filter(|r| !r.is_empty())
                {
                    out.push(BrainEvent::Reasoning(r.to_owned()));
                }
            }
            if let Some(text) = delta
                .get("content")
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
            {
                out.push(BrainEvent::TextDelta(text.to_owned()));
            }
            for call in delta
                .get("tool_calls")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let index = call
                    .get("index")
                    .and_then(Value::as_u64)
                    .and_then(|i| usize::try_from(i).ok())
                    .unwrap_or(self.calls.len());
                let entry = self.calls.entry(index).or_default();
                if let Some(id) = call.get("id").and_then(Value::as_str) {
                    id.clone_into(&mut entry.0);
                }
                let name = call.pointer("/function/name").and_then(Value::as_str);
                if let Some(name) = name {
                    entry.1.push_str(name);
                }
                let args = call
                    .pointer("/function/arguments")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                entry.2.push_str(args);
                out.push(BrainEvent::ToolCallDelta {
                    index,
                    name: name.map(str::to_owned),
                    args_delta: args.to_owned(),
                });
            }
            if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
                self.stop = Some(match reason {
                    "stop" => StopReason::EndTurn,
                    "tool_calls" | "function_call" => StopReason::ToolUse,
                    "length" => StopReason::MaxTokens,
                    "content_filter" => {
                        out.push(BrainEvent::Error(NormalizedError::ContentFiltered));
                        StopReason::Other
                    }
                    _ => StopReason::Other,
                });
            }
        }
        if let Some(usage) = chunk.get("usage").filter(|u| u.is_object()) {
            let n = |p: &str| usage.pointer(p).and_then(Value::as_u64).unwrap_or(0);
            out.push(BrainEvent::Usage(Usage {
                input_tokens: n("/prompt_tokens"),
                output_tokens: n("/completion_tokens"),
                cached_tokens: n("/prompt_tokens_details/cached_tokens"),
            }));
        }
    }

    fn finish(&mut self, out: &mut Vec<BrainEvent>) {
        let calls = std::mem::take(&mut self.calls);
        let any = !calls.is_empty();
        for (index, (id, name, args)) in calls {
            let args = if args.trim().is_empty() {
                json!({})
            } else {
                serde_json::from_str(&args).unwrap_or_else(|_| json!({ "_raw": args }))
            };
            let id = if id.is_empty() {
                format!("call_{index}")
            } else {
                id
            };
            out.push(BrainEvent::ToolCall { id, name, args });
        }
        let stop = self.stop.take().unwrap_or(if any {
            StopReason::ToolUse
        } else {
            StopReason::EndTurn
        });
        out.push(BrainEvent::Done(if any {
            StopReason::ToolUse
        } else {
            stop
        }));
    }
}

#[async_trait::async_trait]
impl BrainProvider for OpenAi {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: self.config.id.clone(),
            name: self.config.name.clone(),
            kind: self.config.kind,
            privacy: self.config.privacy,
            free: self.config.free,
        }
    }

    async fn models(&self) -> Result<Vec<ModelInfo>, NormalizedError> {
        let headers = Self::headers(&self.config, self.key.as_deref());
        let list = self
            .http
            .get_json(&self.url("models"), &headers, &CancellationToken::new())
            .await?;
        Ok(list
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|m| {
                let id = m.get("id").and_then(Value::as_str)?;
                let price = |p: &str| {
                    m.pointer(p)
                        .and_then(|v| {
                            v.as_str()
                                .and_then(|s| s.parse::<f64>().ok())
                                .or_else(|| v.as_f64())
                        })
                        .map(|per_token| per_token * 1_000_000.0)
                };
                Some(ModelInfo {
                    id: id.to_owned(),
                    context_window: m
                        .get("context_length")
                        .and_then(Value::as_u64)
                        .and_then(|c| u32::try_from(c).ok()),
                    tools: true,
                    vision: m
                        .pointer("/architecture/input_modalities")
                        .and_then(Value::as_array)
                        .is_some_and(|a| a.iter().any(|v| v == "image")),
                    streaming: true,
                    input_price: price("/pricing/prompt"),
                    output_price: price("/pricing/completion"),
                })
            })
            .collect())
    }

    fn chat(&self, request: ChatRequest, cancel: CancellationToken) -> BrainStream {
        let (tx, rx) = channel();
        let http = self.http.clone();
        let url = self.url("chat/completions");
        let headers = Self::headers(&self.config, self.key.as_deref());
        let body = body(&self.config, &request);
        tokio::spawn(async move {
            http.stream_chat(&url, &headers, &body, &cancel, &mut Decoder::default(), &tx)
                .await;
        });
        rx
    }
}
