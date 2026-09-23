//! Google Gemini's API (BRAIN-10): `streamGenerateContent` over server-sent events, with function
//! calling, hidden thought parts (BRAIN-09) and usage including cached tokens. Gemini caches
//! repeated prompt prefixes by itself (implicit caching), so the stable system blocks go first in
//! `systemInstruction` (CONV-07). Its function declarations accept only part of JSON Schema, so
//! tool schemas are trimmed to what it takes.

use crate::http::{Decode, Header, Http};
use crate::provider::{BrainProvider, BrainStream, channel};
use crate::sse::SseEvent;
use crate::types::{
    BrainEvent, ChatRequest, ModelInfo, NormalizedError, Part, PrivacyClass, ProviderInfo,
    ProviderKind, Role, StopReason, Usage,
};
use kivo_core::Secret;
use serde_json::{Map, Value, json};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub const BASE_URL: &str = "https://generativelanguage.googleapis.com";

pub struct Gemini {
    base_url: String,
    key: Arc<Secret<String>>,
    http: Http,
}

impl Gemini {
    pub fn new(base_url: impl Into<String>, key: Secret<String>, http: Http) -> Self {
        Self {
            base_url: base_url.into(),
            key: Arc::new(key),
            http,
        }
    }

    fn headers(&self) -> Vec<Header> {
        vec![Header::secret("x-goog-api-key", self.key.expose().clone())]
    }

    fn url(&self, path: &str) -> String {
        format!("{}/v1beta/{path}", self.base_url.trim_end_matches('/'))
    }
}

/// Keeps the JSON Schema keywords Gemini's function declarations accept.
pub fn schema(value: &Value) -> Value {
    const KEEP: [&str; 10] = [
        "type",
        "description",
        "properties",
        "required",
        "items",
        "enum",
        "format",
        "nullable",
        "minimum",
        "maximum",
    ];
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            for (k, v) in map {
                if !KEEP.contains(&k.as_str()) {
                    continue;
                }
                let v = match k.as_str() {
                    "properties" => Value::Object(
                        v.as_object()
                            .map(|p| {
                                p.iter()
                                    .map(|(name, s)| (name.clone(), schema(s)))
                                    .collect()
                            })
                            .unwrap_or_default(),
                    ),
                    "items" => schema(v),
                    _ => v.clone(),
                };
                out.insert(k.clone(), v);
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

/// The request body for `request`.
pub fn body(request: &ChatRequest) -> Value {
    let mut contents: Vec<Value> = Vec::new();
    for message in &request.messages {
        let role = match message.role {
            Role::Assistant => "model",
            Role::User | Role::Tool => "user",
        };
        let parts: Vec<Value> = message
            .parts
            .iter()
            .filter_map(|p| match p {
                Part::Text { text } if text.is_empty() => None,
                Part::Text { text } => Some(json!({ "text": text })),
                Part::ToolCall { name, args, .. } => {
                    Some(json!({ "functionCall": { "name": name, "args": args } }))
                }
                Part::ToolResult { name, content, is_error, .. } => Some(json!({
                    "functionResponse": {
                        "name": name,
                        "response": if *is_error { json!({ "error": content }) } else { json!({ "content": content }) },
                    }
                })),
            })
            .collect();
        if parts.is_empty() {
            continue;
        }
        match contents.last_mut() {
            Some(last) if last["role"] == role => {
                if let Some(p) = last["parts"].as_array_mut() {
                    p.extend(parts);
                }
            }
            _ => contents.push(json!({ "role": role, "parts": parts })),
        }
    }
    let mut config = json!({ "maxOutputTokens": request.max_tokens });
    if let Some(t) = request.temperature {
        config["temperature"] = json!(t);
    }
    let mut body = json!({ "contents": contents, "generationConfig": config });
    let system = request.system_text();
    if !system.is_empty() {
        body["systemInstruction"] = json!({ "parts": [{ "text": system }] });
    }
    if !request.tools.is_empty() {
        let declarations: Vec<Value> = request
            .tools
            .iter()
            .map(|t| json!({ "name": t.name, "description": t.description, "parameters": schema(&t.params) }))
            .collect();
        body["tools"] = json!([{ "functionDeclarations": declarations }]);
    }
    body
}

#[derive(Default)]
pub struct Decoder {
    calls: usize,
    stop: Option<StopReason>,
    usage: Option<Usage>,
}

impl Decode for Decoder {
    fn decode(&mut self, event: &SseEvent, out: &mut Vec<BrainEvent>) {
        let Ok(chunk) = serde_json::from_str::<Value>(&event.data) else {
            return;
        };
        if let Some(error) = chunk.get("error") {
            let status = error.get("code").and_then(Value::as_u64).unwrap_or(500);
            out.push(BrainEvent::Error(crate::http::error_for(
                u16::try_from(status).unwrap_or(500),
                None,
                &json!({ "error": { "message": error["message"] } }).to_string(),
            )));
            return;
        }
        if chunk.pointer("/promptFeedback/blockReason").is_some() {
            out.push(BrainEvent::Error(NormalizedError::ContentFiltered));
            return;
        }
        if let Some(candidate) = chunk.pointer("/candidates/0") {
            for part in candidate
                .pointer("/content/parts")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(call) = part.get("functionCall") {
                    let name = call["name"].as_str().unwrap_or_default().to_owned();
                    let args = call.get("args").cloned().unwrap_or_else(|| json!({}));
                    // Gemini gives calls no ids; the answer is matched by name and order.
                    let id = call
                        .get("id")
                        .and_then(Value::as_str)
                        .map_or_else(|| format!("call_{}", self.calls), str::to_owned);
                    out.push(BrainEvent::ToolCallDelta {
                        index: self.calls,
                        name: Some(name.clone()),
                        args_delta: args.to_string(),
                    });
                    self.calls += 1;
                    out.push(BrainEvent::ToolCall { id, name, args });
                } else if let Some(text) = part.get("text").and_then(Value::as_str) {
                    if part.get("thought").and_then(Value::as_bool) == Some(true) {
                        out.push(BrainEvent::Reasoning(text.to_owned()));
                    } else if !text.is_empty() {
                        out.push(BrainEvent::TextDelta(text.to_owned()));
                    }
                }
            }
            if let Some(reason) = candidate.get("finishReason").and_then(Value::as_str) {
                self.stop = Some(match reason {
                    "STOP" => StopReason::EndTurn,
                    "MAX_TOKENS" => StopReason::MaxTokens,
                    "SAFETY" | "RECITATION" | "PROHIBITED_CONTENT" | "BLOCKLIST" => {
                        out.push(BrainEvent::Error(NormalizedError::ContentFiltered));
                        StopReason::Other
                    }
                    _ => StopReason::Other,
                });
            }
        }
        if let Some(u) = chunk.get("usageMetadata") {
            // Each chunk repeats the running totals; the last one counts.
            let n = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
            self.usage = Some(Usage {
                input_tokens: n("promptTokenCount"),
                output_tokens: n("candidatesTokenCount") + n("thoughtsTokenCount"),
                cached_tokens: n("cachedContentTokenCount"),
            });
        }
    }

    fn finish(&mut self, out: &mut Vec<BrainEvent>) {
        if let Some(usage) = self.usage.take() {
            out.push(BrainEvent::Usage(usage));
        }
        let stop = if self.calls > 0 {
            StopReason::ToolUse
        } else {
            self.stop.take().unwrap_or(StopReason::EndTurn)
        };
        out.push(BrainEvent::Done(stop));
    }
}

#[async_trait::async_trait]
impl BrainProvider for Gemini {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "gemini".into(),
            name: "Google Gemini".into(),
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
            .get("models")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|m| {
                m["supportedGenerationMethods"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|v| v == "generateContent"))
            })
            .filter_map(|m| {
                let id = m["name"].as_str()?.trim_start_matches("models/");
                Some(ModelInfo {
                    context_window: m["inputTokenLimit"]
                        .as_u64()
                        .and_then(|c| u32::try_from(c).ok()),
                    vision: true,
                    ..ModelInfo::named(id)
                })
            })
            .collect())
    }

    fn chat(&self, request: ChatRequest, cancel: CancellationToken) -> BrainStream {
        let (tx, rx) = channel();
        let http = self.http.clone();
        let url = self.url(&format!(
            "models/{}:streamGenerateContent?alt=sse",
            request.model
        ));
        let headers = self.headers();
        let body = body(&request);
        tokio::spawn(async move {
            http.stream_chat(&url, &headers, &body, &cancel, &mut Decoder::default(), &tx)
                .await;
        });
        rx
    }
}
