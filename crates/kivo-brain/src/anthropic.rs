//! Anthropic's Messages API (BRAIN-10), native so that prompt caching works: the last cacheable
//! system block carries `cache_control`, which caches the whole stable prefix (CONV-07). Streams
//! text, tool use (input JSON arrives in pieces), thinking (hidden, BRAIN-09) and usage including
//! cache reads.

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

pub const BASE_URL: &str = "https://api.anthropic.com";
const VERSION: &str = "2023-06-01";

pub struct Anthropic {
    base_url: String,
    key: Arc<Secret<String>>,
    http: Http,
}

impl Anthropic {
    pub fn new(base_url: impl Into<String>, key: Secret<String>, http: Http) -> Self {
        Self {
            base_url: base_url.into(),
            key: Arc::new(key),
            http,
        }
    }

    fn headers(&self) -> Vec<Header> {
        vec![
            Header::secret("x-api-key", self.key.expose().clone()),
            Header::plain("anthropic-version", VERSION),
        ]
    }

    fn url(&self, path: &str) -> String {
        format!("{}/v1/{path}", self.base_url.trim_end_matches('/'))
    }
}

/// The request body for `request`.
pub fn body(request: &ChatRequest) -> Value {
    let last_cacheable = request.system.iter().rposition(|b| b.cacheable);
    let system: Vec<Value> = request
        .system
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let mut block = json!({ "type": "text", "text": b.text });
            if Some(i) == last_cacheable {
                block["cache_control"] = json!({ "type": "ephemeral" });
            }
            block
        })
        .collect();
    let mut messages: Vec<Value> = Vec::new();
    for message in &request.messages {
        let role = match message.role {
            Role::Assistant => "assistant",
            // Tool results are the user's turn in Anthropic's API.
            Role::User | Role::Tool => "user",
        };
        let content: Vec<Value> = message
            .parts
            .iter()
            .filter_map(|p| match p {
                Part::Text { text } if text.is_empty() => None,
                Part::Text { text } => Some(json!({ "type": "text", "text": text })),
                Part::ToolCall { id, name, args } => {
                    Some(json!({ "type": "tool_use", "id": id, "name": name, "input": args }))
                }
                Part::ToolResult {
                    id,
                    content,
                    is_error,
                    ..
                } => Some(json!({
                    "type": "tool_result", "tool_use_id": id, "content": content, "is_error": is_error,
                })),
                Part::Image { media_type, data } => Some(json!({
                    "type": "image",
                    "source": { "type": "base64", "media_type": media_type, "data": data },
                })),
            })
            .collect();
        if content.is_empty() {
            continue;
        }
        // Consecutive turns of the same role merge (the API wants them alternating).
        match messages.last_mut() {
            Some(last) if last["role"] == role => {
                if let Some(parts) = last["content"].as_array_mut() {
                    parts.extend(content);
                }
            }
            _ => messages.push(json!({ "role": role, "content": content })),
        }
    }
    let mut body = json!({
        "model": request.model,
        "max_tokens": request.max_tokens,
        "messages": messages,
        "stream": true,
    });
    if !system.is_empty() {
        body["system"] = Value::Array(system);
    }
    if let Some(t) = request.temperature {
        body["temperature"] = json!(t);
    }
    if !request.tools.is_empty() {
        body["tools"] = Value::Array(
            request
                .tools
                .iter()
                .map(|t| json!({ "name": t.name, "description": t.description, "input_schema": t.params }))
                .collect(),
        );
    }
    body
}

#[derive(Default)]
pub struct Decoder {
    /// Content blocks by index: tool uses collect their input JSON.
    tools: BTreeMap<usize, (String, String, String)>,
    stop: Option<StopReason>,
}

impl Decode for Decoder {
    fn decode(&mut self, event: &SseEvent, out: &mut Vec<BrainEvent>) {
        let Ok(data) = serde_json::from_str::<Value>(&event.data) else {
            return;
        };
        let kind = data.get("type").and_then(Value::as_str).unwrap_or_default();
        let index = data
            .get("index")
            .and_then(Value::as_u64)
            .and_then(|i| usize::try_from(i).ok())
            .unwrap_or(0);
        match kind {
            "message_start" => {
                let u = &data["message"]["usage"];
                let n = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
                out.push(BrainEvent::Usage(Usage {
                    // Anthropic counts cached reads apart from the rest of the input.
                    input_tokens: n("input_tokens")
                        + n("cache_read_input_tokens")
                        + n("cache_creation_input_tokens"),
                    // The final count arrives with `message_delta`.
                    output_tokens: 0,
                    cached_tokens: n("cache_read_input_tokens"),
                }));
            }
            "content_block_start" => {
                let block = &data["content_block"];
                if block["type"] == "tool_use" {
                    let id = block["id"].as_str().unwrap_or_default().to_owned();
                    let name = block["name"].as_str().unwrap_or_default().to_owned();
                    out.push(BrainEvent::ToolCallDelta {
                        index,
                        name: Some(name.clone()),
                        args_delta: String::new(),
                    });
                    self.tools.insert(index, (id, name, String::new()));
                }
            }
            "content_block_delta" => {
                let delta = &data["delta"];
                match delta["type"].as_str().unwrap_or_default() {
                    "text_delta" => {
                        if let Some(t) = delta["text"].as_str().filter(|t| !t.is_empty()) {
                            out.push(BrainEvent::TextDelta(t.to_owned()));
                        }
                    }
                    "input_json_delta" => {
                        let piece = delta["partial_json"].as_str().unwrap_or_default();
                        if let Some(entry) = self.tools.get_mut(&index) {
                            entry.2.push_str(piece);
                        }
                        out.push(BrainEvent::ToolCallDelta {
                            index,
                            name: None,
                            args_delta: piece.to_owned(),
                        });
                    }
                    "thinking_delta" => {
                        if let Some(t) = delta["thinking"].as_str() {
                            out.push(BrainEvent::Reasoning(t.to_owned()));
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                if let Some((id, name, args)) = self.tools.remove(&index) {
                    let args = if args.trim().is_empty() {
                        json!({})
                    } else {
                        serde_json::from_str(&args).unwrap_or_else(|_| json!({ "_raw": args }))
                    };
                    out.push(BrainEvent::ToolCall { id, name, args });
                }
            }
            "message_delta" => {
                if let Some(output) = data.pointer("/usage/output_tokens").and_then(Value::as_u64) {
                    out.push(BrainEvent::Usage(Usage {
                        input_tokens: 0,
                        output_tokens: output,
                        cached_tokens: 0,
                    }));
                }
                if let Some(reason) = data.pointer("/delta/stop_reason").and_then(Value::as_str) {
                    self.stop = Some(match reason {
                        "end_turn" | "stop_sequence" => StopReason::EndTurn,
                        "tool_use" => StopReason::ToolUse,
                        "max_tokens" => StopReason::MaxTokens,
                        "refusal" => {
                            out.push(BrainEvent::Error(NormalizedError::ContentFiltered));
                            StopReason::Other
                        }
                        _ => StopReason::Other,
                    });
                }
            }
            "error" => {
                let error = &data["error"];
                let message = error["message"].as_str().unwrap_or("error");
                let status = match error["type"].as_str().unwrap_or_default() {
                    "overloaded_error" => 529,
                    "rate_limit_error" => 429,
                    "authentication_error" | "permission_error" => 401,
                    "invalid_request_error" => 400,
                    _ => 500,
                };
                out.push(BrainEvent::Error(crate::http::error_for(
                    status,
                    None,
                    &json!({ "error": { "message": message } }).to_string(),
                )));
            }
            _ => {}
        }
    }

    fn finish(&mut self, out: &mut Vec<BrainEvent>) {
        out.push(BrainEvent::Done(
            self.stop.take().unwrap_or(StopReason::EndTurn),
        ));
    }
}

#[async_trait::async_trait]
impl BrainProvider for Anthropic {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "anthropic".into(),
            name: "Anthropic".into(),
            kind: ProviderKind::Api,
            privacy: PrivacyClass::Cloud,
            free: false,
        }
    }

    async fn models(&self) -> Result<Vec<ModelInfo>, NormalizedError> {
        let list = self
            .http
            .get_json(
                &self.url("models"),
                &self.headers(),
                &CancellationToken::new(),
            )
            .await?;
        Ok(list
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|m| m.get("id").and_then(Value::as_str))
            .map(|id| ModelInfo {
                context_window: Some(200_000),
                vision: true,
                ..ModelInfo::named(id)
            })
            .collect())
    }

    fn chat(&self, request: ChatRequest, cancel: CancellationToken) -> BrainStream {
        let (tx, rx) = channel();
        let http = self.http.clone();
        let url = self.url("messages");
        let headers = self.headers();
        let body = body(&request);
        tokio::spawn(async move {
            http.stream_chat(&url, &headers, &body, &cancel, &mut Decoder::default(), &tx)
                .await;
        });
        rx
    }
}
