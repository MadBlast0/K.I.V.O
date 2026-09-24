//! Computer use (CAPABILITIES §4, CAP-09): a vision model looks at a screenshot and proposes the
//! next input action. Behind one trait for Anthropic's computer-use tool, OpenAI's computer use
//! (the Responses API) and Gemini's computer use. The provider only proposes: KIVO's loop checks
//! each action with the permission engine, runs it through its own input tools (never into a
//! password field) and takes the next screenshot. Each keeps its own conversation state, with only
//! the latest screenshot sent in full (older ones are dropped to bound the cost).

use crate::http::{Header, Http};
use crate::types::NormalizedError;
use base64::Engine as _;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

/// What the model wants done next, in the screenshot's pixels.
#[derive(Clone, Debug, PartialEq)]
pub enum CuAction {
    Click {
        x: i32,
        y: i32,
        button: String,
        count: u8,
    },
    /// Click at a point, then type (Gemini's `type_text_at`).
    TypeAt {
        x: i32,
        y: i32,
        text: String,
        enter: bool,
    },
    Type {
        text: String,
    },
    /// A key or a combination: ["ctrl", "s"].
    Press {
        keys: Vec<String>,
    },
    /// Positive lines scroll up.
    Scroll {
        x: i32,
        y: i32,
        lines: i32,
    },
    Move {
        x: i32,
        y: i32,
    },
    /// Nothing to do but look again (a wait, or the model asking for a screenshot).
    Look {
        ms: u64,
    },
    Done {
        summary: String,
    },
    /// The model can't or won't go on; or it needs something KIVO doesn't do (dragging).
    Stop {
        reason: String,
    },
}

/// A screenshot of what the model is working on.
pub struct Screenshot {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl Screenshot {
    fn base64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(&self.png)
    }
}

/// One task's conversation with the provider.
#[derive(Clone, Debug, Default)]
pub struct CuSession {
    pub task: String,
    /// Provider-specific history.
    pub state: Value,
    /// The pending tool call the next screenshot answers.
    pub call: Option<(String, String)>,
    /// Usage so far: input and output tokens.
    pub tokens: (u64, u64),
}

impl CuSession {
    pub fn new(task: &str) -> Self {
        Self {
            task: task.to_owned(),
            ..Self::default()
        }
    }
}

#[async_trait::async_trait]
pub trait ComputerUseProvider: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    /// A rough cost of one step in US dollars (a screenshot in, an action out), for the estimate
    /// shown before a task starts.
    fn cost_per_step(&self) -> f64;
    /// The next action for `shot`.
    async fn step(
        &self,
        session: &mut CuSession,
        shot: &Screenshot,
        cancel: &CancellationToken,
    ) -> Result<CuAction, NormalizedError>;
}

fn int(v: &Value) -> i32 {
    v.as_f64().map_or(0, |f| f.round() as i32)
}

/// "ctrl+shift+s" → ["ctrl", "shift", "s"].
fn keys(text: &str) -> Vec<String> {
    text.split(['+', ' '])
        .map(|k| k.trim().to_lowercase())
        .filter(|k| !k.is_empty())
        .collect()
}

// ---- Anthropic ------------------------------------------------------------------------------

/// Claude with Anthropic's computer-use tool.
pub struct AnthropicComputer {
    pub base_url: String,
    pub key: kivo_core::Secret<String>,
    pub model: String,
    pub tool_type: String,
    pub beta: String,
    pub http: Http,
}

impl AnthropicComputer {
    pub fn new(key: kivo_core::Secret<String>, http: Http) -> Self {
        Self {
            base_url: crate::anthropic::BASE_URL.into(),
            key,
            model: "claude-sonnet-5".into(),
            tool_type: "computer_20250124".into(),
            beta: "computer-use-2025-01-24".into(),
            http,
        }
    }
}

/// Keeps only the latest screenshot in a Claude history (older ones become a note).
fn anthropic_trim(messages: &mut [Value]) {
    for m in messages.iter_mut() {
        if let Some(parts) = m["content"].as_array_mut() {
            for p in parts.iter_mut() {
                if p["type"] == "tool_result" {
                    p["content"] = json!([{ "type": "text", "text": "(earlier screenshot)" }]);
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl ComputerUseProvider for AnthropicComputer {
    fn id(&self) -> &str {
        "anthropic"
    }
    fn name(&self) -> &str {
        "Claude"
    }
    fn cost_per_step(&self) -> f64 {
        0.02
    }

    async fn step(
        &self,
        s: &mut CuSession,
        shot: &Screenshot,
        cancel: &CancellationToken,
    ) -> Result<CuAction, NormalizedError> {
        let image = json!({ "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": shot.base64() } });
        let mut messages: Vec<Value> = s.state.as_array().cloned().unwrap_or_default();
        anthropic_trim(&mut messages);
        match s.call.take() {
            Some((id, _)) => messages.push(json!({ "role": "user", "content": [
                { "type": "tool_result", "tool_use_id": id, "content": [image] }
            ]})),
            None => messages.push(json!({ "role": "user", "content": [
                { "type": "text", "text": s.task }, image
            ]})),
        }
        let body = json!({
            "model": self.model,
            "max_tokens": 1024,
            "tools": [{ "type": self.tool_type, "name": "computer",
                        "display_width_px": shot.width, "display_height_px": shot.height }],
            "messages": messages,
        });
        let reply = self
            .http
            .post_json(
                &format!("{}/v1/messages", self.base_url),
                &[
                    Header::secret("x-api-key", self.key.expose().clone()),
                    Header::plain("anthropic-version", "2023-06-01"),
                    Header::plain("anthropic-beta", self.beta.clone()),
                ],
                &body,
                cancel,
            )
            .await?;
        s.tokens.0 += reply["usage"]["input_tokens"].as_u64().unwrap_or(0);
        s.tokens.1 += reply["usage"]["output_tokens"].as_u64().unwrap_or(0);
        let content = reply["content"].as_array().cloned().unwrap_or_default();
        messages.push(json!({ "role": "assistant", "content": content }));
        s.state = Value::Array(messages);
        let said: String = content
            .iter()
            .filter(|c| c["type"] == "text")
            .filter_map(|c| c["text"].as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let Some(call) = content.iter().find(|c| c["type"] == "tool_use") else {
            return Ok(CuAction::Done { summary: said });
        };
        s.call = Some((
            call["id"].as_str().unwrap_or_default().to_owned(),
            "computer".into(),
        ));
        let input = &call["input"];
        let at = || (int(&input["coordinate"][0]), int(&input["coordinate"][1]));
        Ok(match input["action"].as_str().unwrap_or_default() {
            "left_click" => {
                let (x, y) = at();
                CuAction::Click {
                    x,
                    y,
                    button: "left".into(),
                    count: 1,
                }
            }
            "right_click" => {
                let (x, y) = at();
                CuAction::Click {
                    x,
                    y,
                    button: "right".into(),
                    count: 1,
                }
            }
            "middle_click" => {
                let (x, y) = at();
                CuAction::Click {
                    x,
                    y,
                    button: "middle".into(),
                    count: 1,
                }
            }
            "double_click" => {
                let (x, y) = at();
                CuAction::Click {
                    x,
                    y,
                    button: "left".into(),
                    count: 2,
                }
            }
            "triple_click" => {
                let (x, y) = at();
                CuAction::Click {
                    x,
                    y,
                    button: "left".into(),
                    count: 3,
                }
            }
            "mouse_move" => {
                let (x, y) = at();
                CuAction::Move { x, y }
            }
            "type" => CuAction::Type {
                text: input["text"].as_str().unwrap_or_default().to_owned(),
            },
            "key" => CuAction::Press {
                keys: keys(input["text"].as_str().unwrap_or_default()),
            },
            "scroll" => {
                let (x, y) = at();
                let amount = input["scroll_amount"].as_i64().unwrap_or(3) as i32;
                let lines = if input["scroll_direction"] == "down" {
                    -amount
                } else {
                    amount
                };
                CuAction::Scroll { x, y, lines }
            }
            "wait" => CuAction::Look { ms: 1000 },
            "screenshot" | "cursor_position" => CuAction::Look { ms: 0 },
            other => CuAction::Stop {
                reason: format!("the model asked for “{other}”, which KIVO doesn't do"),
            },
        })
    }
}

// ---- OpenAI ---------------------------------------------------------------------------------

/// OpenAI's computer use through the Responses API.
pub struct OpenAiComputer {
    pub base_url: String,
    pub key: kivo_core::Secret<String>,
    pub model: String,
    pub http: Http,
}

impl OpenAiComputer {
    pub fn new(key: kivo_core::Secret<String>, http: Http) -> Self {
        Self {
            base_url: "https://api.openai.com/v1".into(),
            key,
            model: "computer-use-preview".into(),
            http,
        }
    }
}

#[async_trait::async_trait]
impl ComputerUseProvider for OpenAiComputer {
    fn id(&self) -> &str {
        "openai"
    }
    fn name(&self) -> &str {
        "OpenAI"
    }
    fn cost_per_step(&self) -> f64 {
        0.02
    }

    async fn step(
        &self,
        s: &mut CuSession,
        shot: &Screenshot,
        cancel: &CancellationToken,
    ) -> Result<CuAction, NormalizedError> {
        let url = format!("data:image/png;base64,{}", shot.base64());
        let tools = json!([{ "type": "computer_use_preview", "display_width": shot.width,
                             "display_height": shot.height, "environment": "windows" }]);
        let mut body = json!({ "model": self.model, "tools": tools, "truncation": "auto" });
        match s.call.take() {
            Some((call_id, _)) => {
                body["previous_response_id"] = s.state["previous"].clone();
                body["input"] = json!([{ "type": "computer_call_output", "call_id": call_id,
                    "output": { "type": "computer_screenshot", "image_url": url } }]);
            }
            None => {
                body["input"] = json!([{ "role": "user", "content": [
                    { "type": "input_text", "text": s.task },
                    { "type": "input_image", "image_url": url }
                ]}]);
            }
        }
        let reply = self
            .http
            .post_json(
                &format!("{}/responses", self.base_url),
                &[Header::secret(
                    "authorization",
                    format!("Bearer {}", self.key.expose()),
                )],
                &body,
                cancel,
            )
            .await?;
        s.tokens.0 += reply["usage"]["input_tokens"].as_u64().unwrap_or(0);
        s.tokens.1 += reply["usage"]["output_tokens"].as_u64().unwrap_or(0);
        s.state = json!({ "previous": reply["id"] });
        let output = reply["output"].as_array().cloned().unwrap_or_default();
        let said: String = output
            .iter()
            .filter(|o| o["type"] == "message")
            .flat_map(|o| o["content"].as_array().cloned().unwrap_or_default())
            .filter_map(|c| c["text"].as_str().map(str::to_owned))
            .collect::<Vec<_>>()
            .join(" ");
        let Some(call) = output.iter().find(|o| o["type"] == "computer_call") else {
            return Ok(CuAction::Done { summary: said });
        };
        // A step OpenAI's own checks flagged (a suspicious page, a sensitive domain) isn't taken.
        if call["pending_safety_checks"]
            .as_array()
            .is_some_and(|c| !c.is_empty())
        {
            let why = call["pending_safety_checks"][0]["message"]
                .as_str()
                .unwrap_or("a safety check")
                .to_owned();
            return Ok(CuAction::Stop { reason: why });
        }
        s.call = Some((
            call["call_id"].as_str().unwrap_or_default().to_owned(),
            "computer".into(),
        ));
        let a = &call["action"];
        let (x, y) = (int(&a["x"]), int(&a["y"]));
        Ok(match a["type"].as_str().unwrap_or_default() {
            "click" => CuAction::Click {
                x,
                y,
                button: a["button"].as_str().unwrap_or("left").to_owned(),
                count: 1,
            },
            "double_click" => CuAction::Click {
                x,
                y,
                button: "left".into(),
                count: 2,
            },
            "move" => CuAction::Move { x, y },
            "type" => CuAction::Type {
                text: a["text"].as_str().unwrap_or_default().to_owned(),
            },
            "keypress" => CuAction::Press {
                keys: a["keys"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_lowercase)
                    .collect(),
            },
            "scroll" => CuAction::Scroll {
                x,
                y,
                lines: -(int(&a["scroll_y"]) / 100).clamp(-20, 20),
            },
            "wait" => CuAction::Look { ms: 1000 },
            "screenshot" => CuAction::Look { ms: 0 },
            other => CuAction::Stop {
                reason: format!("the model asked for “{other}”, which KIVO doesn't do"),
            },
        })
    }
}

// ---- Gemini ---------------------------------------------------------------------------------

/// Gemini's computer use (coordinates on a 1000 × 1000 grid over the screenshot).
pub struct GeminiComputer {
    pub base_url: String,
    pub key: kivo_core::Secret<String>,
    pub model: String,
    pub http: Http,
}

impl GeminiComputer {
    pub fn new(key: kivo_core::Secret<String>, http: Http) -> Self {
        Self {
            base_url: format!("{}/v1beta", crate::gemini::BASE_URL),
            key,
            model: "gemini-2.5-computer-use-preview-10-2025".into(),
            http,
        }
    }
}

#[async_trait::async_trait]
impl ComputerUseProvider for GeminiComputer {
    fn id(&self) -> &str {
        "gemini"
    }
    fn name(&self) -> &str {
        "Gemini"
    }
    fn cost_per_step(&self) -> f64 {
        0.01
    }

    async fn step(
        &self,
        s: &mut CuSession,
        shot: &Screenshot,
        cancel: &CancellationToken,
    ) -> Result<CuAction, NormalizedError> {
        let image = json!({ "inline_data": { "mime_type": "image/png", "data": shot.base64() } });
        let mut contents: Vec<Value> = s.state.as_array().cloned().unwrap_or_default();
        // Older screenshots are dropped.
        for c in &mut contents {
            if let Some(parts) = c["parts"].as_array_mut() {
                parts.retain(|p| p.get("inline_data").is_none());
            }
        }
        match s.call.take() {
            Some((_, name)) => contents.push(json!({ "role": "user", "parts": [
                { "functionResponse": { "name": name, "response": { "done": true } } }, image
            ]})),
            None => contents.push(json!({ "role": "user", "parts": [{ "text": s.task }, image] })),
        }
        let body = json!({
            "contents": contents,
            "tools": [{ "computer_use": { "environment": "ENVIRONMENT_BROWSER" } }],
        });
        let reply = self
            .http
            .post_json(
                &format!("{}/models/{}:generateContent", self.base_url, self.model),
                &[Header::secret("x-goog-api-key", self.key.expose().clone())],
                &body,
                cancel,
            )
            .await?;
        s.tokens.0 += reply["usageMetadata"]["promptTokenCount"]
            .as_u64()
            .unwrap_or(0);
        s.tokens.1 += reply["usageMetadata"]["candidatesTokenCount"]
            .as_u64()
            .unwrap_or(0);
        let content = reply["candidates"][0]["content"].clone();
        contents.push(content.clone());
        s.state = Value::Array(contents);
        let parts = content["parts"].as_array().cloned().unwrap_or_default();
        let said: String = parts
            .iter()
            .filter_map(|p| p["text"].as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let Some(call) = parts.iter().find_map(|p| p.get("functionCall")) else {
            return Ok(CuAction::Done { summary: said });
        };
        let name = call["name"].as_str().unwrap_or_default().to_owned();
        s.call = Some((String::new(), name.clone()));
        let a = &call["args"];
        let scale = |v: &Value, size: u32| {
            (v.as_f64().unwrap_or(0.0) / 1000.0 * f64::from(size)).round() as i32
        };
        let (x, y) = (scale(&a["x"], shot.width), scale(&a["y"], shot.height));
        Ok(match name.as_str() {
            "click_at" => CuAction::Click {
                x,
                y,
                button: "left".into(),
                count: 1,
            },
            "hover_at" => CuAction::Move { x, y },
            "type_text_at" => CuAction::TypeAt {
                x,
                y,
                text: a["text"].as_str().unwrap_or_default().to_owned(),
                enter: a["press_enter"].as_bool().unwrap_or(false),
            },
            "key_combination" => CuAction::Press {
                keys: keys(a["keys"].as_str().unwrap_or_default()),
            },
            "scroll_at" | "scroll_document" => {
                let lines = match a["direction"].as_str() {
                    Some("down") => -5,
                    Some("up") => 5,
                    _ => 0,
                };
                CuAction::Scroll { x, y, lines }
            }
            "wait_5_seconds" => CuAction::Look { ms: 5000 },
            other => CuAction::Stop {
                reason: format!(
                    "the model asked for “{other}”, which KIVO doesn't do on the desktop"
                ),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{MockServer, Reply};

    fn shot() -> Screenshot {
        Screenshot {
            png: vec![137, 80, 78, 71],
            width: 1280,
            height: 800,
        }
    }

    fn key() -> kivo_core::Secret<String> {
        kivo_core::Secret::new("sk-test".to_owned())
    }

    #[tokio::test]
    async fn claude_proposes_clicks_and_types_then_finishes() {
        let server = MockServer::start(|r| {
            let n = r.body["messages"].as_array().map_or(0, Vec::len);
            if n == 1 {
                Reply::json(200, &json!({ "content": [
                    { "type": "text", "text": "I'll open the menu." },
                    { "type": "tool_use", "id": "t1", "name": "computer", "input": { "action": "left_click", "coordinate": [100, 40] } }
                ], "usage": { "input_tokens": 1500, "output_tokens": 30 } }))
            } else {
                Reply::json(200, &json!({ "content": [{ "type": "text", "text": "Exported." }], "usage": { "input_tokens": 1600, "output_tokens": 5 } }))
            }
        })
        .await;
        let mut p = AnthropicComputer::new(key(), Http::new());
        p.base_url = server.url.clone();
        let mut s = CuSession::new("Export the note as PDF");
        let cancel = CancellationToken::new();
        let a = p.step(&mut s, &shot(), &cancel).await.unwrap();
        assert_eq!(
            a,
            CuAction::Click {
                x: 100,
                y: 40,
                button: "left".into(),
                count: 1
            }
        );
        let first = server.requests().pop().unwrap();
        assert_eq!(
            first.header("anthropic-beta"),
            Some("computer-use-2025-01-24")
        );
        assert_eq!(first.body["tools"][0]["display_width_px"], 1280);
        assert_eq!(first.body["messages"][0]["content"][1]["type"], "image");
        let done = p.step(&mut s, &shot(), &cancel).await.unwrap();
        assert_eq!(
            done,
            CuAction::Done {
                summary: "Exported.".into()
            }
        );
        let second = server.requests().pop().unwrap();
        let msgs = second.body["messages"].as_array().unwrap();
        assert_eq!(msgs.last().unwrap()["content"][0]["tool_use_id"], "t1");
        assert_eq!(s.tokens, (3100, 35));
    }

    #[tokio::test]
    async fn openai_follows_its_calls_and_stops_on_flagged_steps() {
        let server = MockServer::start(|r| {
            if r.body.get("previous_response_id").is_none() {
                Reply::json(200, &json!({ "id": "r1", "output": [
                    { "type": "computer_call", "call_id": "c1", "action": { "type": "keypress", "keys": ["CTRL", "S"] }, "pending_safety_checks": [] }
                ]}))
            } else {
                Reply::json(200, &json!({ "id": "r2", "output": [
                    { "type": "computer_call", "call_id": "c2", "action": { "type": "click", "x": 5, "y": 6, "button": "left" },
                      "pending_safety_checks": [{ "id": "s1", "code": "malicious_instructions", "message": "The page may be trying to trick the agent." }] }
                ]}))
            }
        })
        .await;
        let mut p = OpenAiComputer::new(key(), Http::new());
        p.base_url = server.url.clone();
        let mut s = CuSession::new("Save it");
        let cancel = CancellationToken::new();
        assert_eq!(
            p.step(&mut s, &shot(), &cancel).await.unwrap(),
            CuAction::Press {
                keys: vec!["ctrl".into(), "s".into()]
            }
        );
        let second = p.step(&mut s, &shot(), &cancel).await.unwrap();
        assert!(matches!(second, CuAction::Stop { .. }), "{second:?}");
        let req = server.requests().pop().unwrap();
        assert_eq!(req.body["previous_response_id"], "r1");
        assert_eq!(req.body["input"][0]["call_id"], "c1");
        assert_eq!(
            req.body["input"][0]["output"]["type"],
            "computer_screenshot"
        );
    }

    #[tokio::test]
    async fn gemini_coordinates_are_scaled_from_its_grid() {
        let server = MockServer::start(|_| {
            Reply::json(200, &json!({ "candidates": [{ "content": { "role": "model", "parts": [
                { "functionCall": { "name": "type_text_at", "args": { "x": 500, "y": 250, "text": "hello", "press_enter": true } } }
            ]}}]}))
        })
        .await;
        let mut p = GeminiComputer::new(key(), Http::new());
        p.base_url = server.url.clone();
        let mut s = CuSession::new("Search for hello");
        let a = p
            .step(&mut s, &shot(), &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(
            a,
            CuAction::TypeAt {
                x: 640,
                y: 200,
                text: "hello".into(),
                enter: true
            }
        );
        assert_eq!(
            server.requests().pop().unwrap().header("x-goog-api-key"),
            Some("sk-test")
        );
    }
}
