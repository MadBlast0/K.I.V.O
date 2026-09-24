//! Realtime conversation mode (BRAINS §8, BRAIN-33): speech-to-speech brains (OpenAI
//! `gpt-realtime`, Gemini Live native audio) over one WebSocket per session.
//!
//! The session takes the user's audio (16 kHz mono, echo already removed by the runtime), typed
//! lines and tool results, and gives back the model's audio, both sides' transcripts, tool calls
//! and usage. Tool calls are only *reported* here: the runtime puts each through the permission
//! engine and answers with `Control::ToolResult` (invariant 4).
//!
//! Provider limits are hidden (BRAINS §8 "Session limits"): when the provider says the connection
//! is about to end (Gemini's `goAway`, OpenAI's `session_expired`) or it drops unexpectedly, the
//! adapter reconnects by itself, resuming with Gemini's session handle when it has one, and
//! otherwise carrying the conversation so far into the new session's instructions. The runtime
//! only sees `RtEvent::Resumed`.

use crate::types::{NormalizedError, ToolDef, Usage};
use async_trait::async_trait;
use base64::Engine as _;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{HeaderValue, Request};
use tokio_tungstenite::tungstenite::{Error as WsError, Message as WsMessage};
use tokio_util::sync::CancellationToken;

/// The rate of the audio the session takes (the runtime's capture rate).
pub const INPUT_RATE: u32 = 16_000;
/// Audio sent before the session is ready is kept this long at most.
const EARLY_AUDIO: usize = INPUT_RATE as usize * 2;
/// Reconnects in a row before the session gives up.
const MAX_RECONNECTS: u32 = 3;
/// The conversation carried into a new session when there's no handle, most recent last.
const CARRIED_CHARS: usize = 6_000;

/// What a session is set up with.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RealtimeConfig {
    /// Empty: the provider's default realtime model.
    pub model: String,
    pub instructions: String,
    pub tools: Vec<ToolDef>,
    /// Empty: the provider's default voice.
    pub voice: String,
}

/// What a session reports.
#[derive(Clone, Debug, PartialEq)]
pub enum RtEvent {
    /// The provider accepted the setup; audio flows from now on.
    Ready,
    /// The model's speech (mono, at `rate`).
    Audio {
        pcm: Vec<f32>,
        rate: u32,
    },
    /// The user started talking over the model: stop playing what it said.
    UserSpeaking,
    /// What the user said (one finished utterance).
    UserTranscript(String),
    /// A piece of what the model is saying.
    ModelTranscript(String),
    /// The model finished its turn.
    ModelTurnDone,
    /// The model asks for a tool; answer with `Control::ToolResult`.
    ToolCall {
        id: String,
        name: String,
        args: Value,
    },
    Usage(Usage),
    /// The connection was renewed; the conversation continues.
    Resumed,
    /// Something went wrong that didn't end the session.
    Error(NormalizedError),
    /// The session ended (asked to, cancelled, or it couldn't be kept going).
    Closed(Option<NormalizedError>),
}

/// What the runtime sends besides audio.
#[derive(Clone, Debug, PartialEq)]
pub enum Control {
    /// A typed line, or the request that opened the session.
    Text(String),
    ToolResult {
        id: String,
        name: String,
        output: String,
    },
    Close,
}

/// A live session.
pub struct RealtimeSession {
    /// The user's audio, 16 kHz mono; full means the connection is behind and audio is dropped.
    pub audio: mpsc::Sender<Vec<f32>>,
    pub control: mpsc::UnboundedSender<Control>,
    pub events: mpsc::UnboundedReceiver<RtEvent>,
}

#[async_trait]
pub trait RealtimeProvider: Send + Sync {
    /// The brain connection it belongs to (`openai`, `gemini`).
    fn id(&self) -> &'static str;
    fn name(&self) -> String;
    /// The model a session uses when the settings name none.
    fn default_model(&self) -> &'static str;
    /// A rough price per minute of conversation, in US dollars (BRAINS §8 "Cost").
    fn cost_per_minute(&self) -> f64;
    /// Opens a session; fails at once on a refused key or an unreachable service.
    async fn connect(
        &self,
        config: RealtimeConfig,
        cancel: CancellationToken,
    ) -> Result<RealtimeSession, NormalizedError>;
}

// ---------------------------------------------------------------------------------------------
// Audio helpers

/// Linear resampling (speech, upsampling or mild downsampling).
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    #[allow(clippy::cast_precision_loss, reason = "audio lengths")]
    let ratio = f64::from(from) / f64::from(to);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "audio lengths"
    )]
    let out_len = ((input.len() as f64) / ratio).floor() as usize;
    (0..out_len)
        .map(|i| {
            #[allow(clippy::cast_precision_loss, reason = "audio positions")]
            let pos = i as f64 * ratio;
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a position in the input"
            )]
            let at = pos.floor() as usize;
            #[allow(clippy::cast_possible_truncation, reason = "a fraction")]
            let frac = (pos - pos.floor()) as f32;
            let a = input[at.min(input.len() - 1)];
            let b = input[(at + 1).min(input.len() - 1)];
            a + (b - a) * frac
        })
        .collect()
}

/// f32 samples → little-endian PCM16, base64.
pub fn encode_pcm16(samples: &[f32]) -> String {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for s in samples {
        #[allow(clippy::cast_possible_truncation, reason = "clamped to the i16 range")]
        let v = (s.clamp(-1.0, 1.0) * 32_767.0).round() as i16;
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Base64 little-endian PCM16 → f32 samples.
pub fn decode_pcm16(data: &str) -> Vec<f32> {
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) else {
        return Vec::new();
    };
    bytes
        .chunks_exact(2)
        .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32_768.0)
        .collect()
}

/// The rate in a mime type such as `audio/pcm;rate=24000`.
fn mime_rate(mime: &str, fallback: u32) -> u32 {
    mime.split(';')
        .find_map(|p| p.trim().strip_prefix("rate="))
        .and_then(|r| r.parse().ok())
        .unwrap_or(fallback)
}

// ---------------------------------------------------------------------------------------------
// The shared driver

/// One provider's wire format.
trait Dialect: Send + 'static {
    fn request(&self, config: &RealtimeConfig) -> Result<Request<()>, NormalizedError>;
    /// What to send after connecting. `carried` is the conversation so far (for a new session
    /// without a handle).
    fn setup(&self, config: &RealtimeConfig, carried: &str) -> Vec<String>;
    fn input_rate(&self) -> u32;
    fn audio(&self, samples: &[f32]) -> String;
    fn text(&self, text: &str) -> Vec<String>;
    fn tool_result(&self, id: &str, name: &str, output: &str) -> Vec<String>;
    /// Reads one server message. Returns what it means and whether to reconnect now.
    fn parse(&mut self, message: &Value) -> (Vec<RtEvent>, bool);
}

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn open(request: Request<()>) -> Result<Ws, NormalizedError> {
    match tokio_tungstenite::connect_async(request).await {
        Ok((ws, _)) => Ok(ws),
        Err(WsError::Http(response)) => {
            let body = response
                .body()
                .as_ref()
                .map(|b| String::from_utf8_lossy(b).into_owned())
                .unwrap_or_default();
            Err(crate::http::error_for(
                response.status().as_u16(),
                None,
                &body,
            ))
        }
        Err(e) => Err(NormalizedError::Network(e.to_string())),
    }
}

async fn connect_with(
    mut dialect: Box<dyn Dialect>,
    config: RealtimeConfig,
    cancel: CancellationToken,
) -> Result<RealtimeSession, NormalizedError> {
    let mut ws = open(dialect.request(&config)?).await?;
    for message in dialect.setup(&config, "") {
        ws.send(WsMessage::text(message))
            .await
            .map_err(|e| NormalizedError::Network(e.to_string()))?;
    }
    let (audio_tx, mut audio_rx) = mpsc::channel::<Vec<f32>>(64);
    let (control_tx, mut control_rx) = mpsc::unbounded_channel::<Control>();
    let (events_tx, events_rx) = mpsc::unbounded_channel::<RtEvent>();
    tokio::spawn(async move {
        let mut ready = false;
        let mut early: Vec<f32> = Vec::new();
        let mut said: Vec<(bool, String)> = Vec::new();
        let mut model_line = String::new();
        let mut reconnects = 0u32;
        let mut audio_open = true;
        let ended = 'session: loop {
            let reconnect = loop {
                tokio::select! {
                    () = cancel.cancelled() => {
                        let _ = ws.close(None).await;
                        break 'session None;
                    }
                    incoming = ws.next() => {
                        let text = match incoming {
                            Some(Ok(WsMessage::Text(t))) => t.as_str().to_owned(),
                            Some(Ok(WsMessage::Binary(b))) => String::from_utf8_lossy(&b).into_owned(),
                            Some(Ok(WsMessage::Close(_)) | Err(_)) | None => break true,
                            Some(Ok(_)) => continue,
                        };
                        let Ok(value) = serde_json::from_str::<Value>(&text) else { continue };
                        let (events, again) = dialect.parse(&value);
                        for event in events {
                            match &event {
                                RtEvent::Ready => {
                                    ready = true;
                                    reconnects = 0;
                                    if !early.is_empty() {
                                        let held = std::mem::take(&mut early);
                                        let _ = ws.send(WsMessage::text(dialect.audio(&resample(&held, INPUT_RATE, dialect.input_rate())))).await;
                                    }
                                }
                                RtEvent::UserTranscript(t) => said.push((true, t.clone())),
                                RtEvent::ModelTranscript(d) => model_line.push_str(d),
                                RtEvent::ModelTurnDone if !model_line.is_empty() => {
                                    said.push((false, std::mem::take(&mut model_line)));
                                }
                                _ => {}
                            }
                            if events_tx.send(event).is_err() {
                                let _ = ws.close(None).await;
                                break 'session None;
                            }
                        }
                        if again {
                            break true;
                        }
                    }
                    samples = audio_rx.recv(), if audio_open => {
                        let Some(samples) = samples else {
                            audio_open = false;
                            continue;
                        };
                        if ready {
                            let converted = resample(&samples, INPUT_RATE, dialect.input_rate());
                            if ws.send(WsMessage::text(dialect.audio(&converted))).await.is_err() {
                                break true;
                            }
                        } else {
                            early.extend_from_slice(&samples);
                            if early.len() > EARLY_AUDIO {
                                let extra = early.len() - EARLY_AUDIO;
                                early.drain(..extra);
                            }
                        }
                    }
                    control = control_rx.recv() => {
                        let messages = match control {
                            None | Some(Control::Close) => {
                                let _ = ws.close(None).await;
                                break 'session None;
                            }
                            Some(Control::Text(t)) => {
                                said.push((true, t.clone()));
                                dialect.text(&t)
                            }
                            Some(Control::ToolResult { id, name, output }) => dialect.tool_result(&id, &name, &output),
                        };
                        for m in messages {
                            if ws.send(WsMessage::text(m)).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            };
            if !reconnect {
                break None;
            }
            // Renew the connection (BRAINS §8): the user doesn't see provider limits. The old
            // one is closed first.
            let _ = ws.close(None).await;
            reconnects += 1;
            if reconnects > MAX_RECONNECTS {
                break Some(NormalizedError::ProviderDown(
                    "the session kept dropping".into(),
                ));
            }
            tokio::time::sleep(Duration::from_millis(200 * u64::from(reconnects))).await;
            if cancel.is_cancelled() {
                break None;
            }
            let request = match dialect.request(&config) {
                Ok(r) => r,
                Err(e) => break Some(e),
            };
            match open(request).await {
                Ok(next) => {
                    ws = next;
                    ready = false;
                    let carried = carry(&said);
                    for message in dialect.setup(&config, &carried) {
                        let _ = ws.send(WsMessage::text(message)).await;
                    }
                    tracing::info!("realtime session renewed");
                    let _ = events_tx.send(RtEvent::Resumed);
                }
                Err(e) => break Some(e),
            }
        };
        let _ = events_tx.send(RtEvent::Closed(ended));
    });
    Ok(RealtimeSession {
        audio: audio_tx,
        control: control_tx,
        events: events_rx,
    })
}

/// The conversation so far as text for a new session's instructions (most recent kept).
fn carry(said: &[(bool, String)]) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut used = 0;
    for (user, text) in said.iter().rev() {
        let line = format!("{}: {}", if *user { "User" } else { "You" }, text.trim());
        used += line.len() + 1;
        if used > CARRIED_CHARS {
            break;
        }
        lines.push(line);
    }
    lines.reverse();
    lines.join("\n")
}

fn with_carried(instructions: &str, carried: &str) -> String {
    if carried.is_empty() {
        instructions.to_owned()
    } else {
        format!(
            "{instructions}\n\nThe conversation so far (continue it; don't greet again):\n{carried}"
        )
    }
}

fn header(value: &str) -> Result<HeaderValue, NormalizedError> {
    HeaderValue::from_str(value).map_err(|_| NormalizedError::Auth)
}

// ---------------------------------------------------------------------------------------------
// OpenAI Realtime

/// OpenAI's Realtime API (`gpt-realtime`), PCM16 at 24 kHz both ways, server VAD.
pub struct OpenAiRealtime {
    key: kivo_core::Secret<String>,
    /// `wss://api.openai.com`; tests point it at a local server.
    pub ws_base: String,
}

impl OpenAiRealtime {
    pub fn new(key: kivo_core::Secret<String>) -> Self {
        Self {
            key,
            ws_base: "wss://api.openai.com".into(),
        }
    }
}

struct OpenAiDialect {
    key: kivo_core::Secret<String>,
    ws_base: String,
    ready: bool,
}

const OPENAI_RATE: u32 = 24_000;

impl Dialect for OpenAiDialect {
    fn request(&self, config: &RealtimeConfig) -> Result<Request<()>, NormalizedError> {
        let model = if config.model.is_empty() {
            "gpt-realtime"
        } else {
            &config.model
        };
        let url = format!(
            "{}/v1/realtime?model={model}",
            self.ws_base.trim_end_matches('/')
        );
        let mut request = url
            .into_client_request()
            .map_err(|e| NormalizedError::Other(e.to_string()))?;
        request.headers_mut().insert(
            "Authorization",
            header(&format!("Bearer {}", self.key.expose()))?,
        );
        Ok(request)
    }

    fn setup(&self, config: &RealtimeConfig, carried: &str) -> Vec<String> {
        let tools: Vec<Value> = config
            .tools
            .iter()
            .map(|t| json!({ "type": "function", "name": t.name, "description": t.description, "parameters": t.params }))
            .collect();
        let voice = if config.voice.is_empty() {
            "marin"
        } else {
            &config.voice
        };
        let model = if config.model.is_empty() {
            "gpt-realtime"
        } else {
            &config.model
        };
        vec![
            json!({
                "type": "session.update",
                "session": {
                    "type": "realtime",
                    "model": model,
                    "instructions": with_carried(&config.instructions, carried),
                    "output_modalities": ["audio"],
                    "audio": {
                        "input": {
                            "format": { "type": "audio/pcm", "rate": OPENAI_RATE },
                            "transcription": { "model": "gpt-4o-mini-transcribe" },
                            "turn_detection": { "type": "server_vad" }
                        },
                        "output": {
                            "format": { "type": "audio/pcm", "rate": OPENAI_RATE },
                            "voice": voice
                        }
                    },
                    "tools": tools,
                    "tool_choice": "auto"
                }
            })
            .to_string(),
        ]
    }

    fn input_rate(&self) -> u32 {
        OPENAI_RATE
    }

    fn audio(&self, samples: &[f32]) -> String {
        json!({ "type": "input_audio_buffer.append", "audio": encode_pcm16(samples) }).to_string()
    }

    fn text(&self, text: &str) -> Vec<String> {
        vec![
            json!({
                "type": "conversation.item.create",
                "item": { "type": "message", "role": "user", "content": [{ "type": "input_text", "text": text }] }
            })
            .to_string(),
            json!({ "type": "response.create" }).to_string(),
        ]
    }

    fn tool_result(&self, id: &str, _name: &str, output: &str) -> Vec<String> {
        vec![
            json!({
                "type": "conversation.item.create",
                "item": { "type": "function_call_output", "call_id": id, "output": output }
            })
            .to_string(),
            json!({ "type": "response.create" }).to_string(),
        ]
    }

    fn parse(&mut self, m: &Value) -> (Vec<RtEvent>, bool) {
        let kind = m["type"].as_str().unwrap_or_default();
        let str_of = |v: &Value| v.as_str().unwrap_or_default().to_owned();
        let events = match kind {
            "session.updated" if !self.ready => {
                self.ready = true;
                vec![RtEvent::Ready]
            }
            "response.output_audio.delta" | "response.audio.delta" => vec![RtEvent::Audio {
                pcm: decode_pcm16(m["delta"].as_str().unwrap_or_default()),
                rate: OPENAI_RATE,
            }],
            "response.output_audio_transcript.delta" | "response.audio_transcript.delta" => {
                vec![RtEvent::ModelTranscript(str_of(&m["delta"]))]
            }
            "conversation.item.input_audio_transcription.completed" => {
                let t = str_of(&m["transcript"]);
                if t.trim().is_empty() {
                    Vec::new()
                } else {
                    vec![RtEvent::UserTranscript(t.trim().to_owned())]
                }
            }
            "input_audio_buffer.speech_started" => vec![RtEvent::UserSpeaking],
            "response.function_call_arguments.done" => {
                let args = serde_json::from_str(m["arguments"].as_str().unwrap_or("{}"))
                    .unwrap_or_else(|_| json!({}));
                vec![RtEvent::ToolCall {
                    id: str_of(&m["call_id"]),
                    name: str_of(&m["name"]),
                    args,
                }]
            }
            "response.done" => {
                let u = &m["response"]["usage"];
                let mut out = Vec::new();
                if u.is_object() {
                    out.push(RtEvent::Usage(Usage {
                        input_tokens: u["input_tokens"].as_u64().unwrap_or(0),
                        output_tokens: u["output_tokens"].as_u64().unwrap_or(0),
                        cached_tokens: u["input_token_details"]["cached_tokens"]
                            .as_u64()
                            .unwrap_or(0),
                    }));
                }
                out.push(RtEvent::ModelTurnDone);
                out
            }
            "error" => {
                let code = m["error"]["code"].as_str().unwrap_or_default();
                if code == "session_expired" {
                    return (Vec::new(), true);
                }
                let message = str_of(&m["error"]["message"]);
                let error = if code == "invalid_api_key" {
                    NormalizedError::Auth
                } else if code == "insufficient_quota" {
                    NormalizedError::Quota
                } else {
                    NormalizedError::Other(message)
                };
                vec![RtEvent::Error(error)]
            }
            _ => Vec::new(),
        };
        (events, false)
    }
}

#[async_trait]
impl RealtimeProvider for OpenAiRealtime {
    fn id(&self) -> &'static str {
        "openai"
    }
    fn name(&self) -> String {
        "OpenAI Realtime".into()
    }
    fn default_model(&self) -> &'static str {
        "gpt-realtime"
    }
    fn cost_per_minute(&self) -> f64 {
        0.11
    }
    async fn connect(
        &self,
        config: RealtimeConfig,
        cancel: CancellationToken,
    ) -> Result<RealtimeSession, NormalizedError> {
        let dialect = OpenAiDialect {
            key: kivo_core::Secret::new(self.key.expose().clone()),
            ws_base: self.ws_base.clone(),
            ready: false,
        };
        connect_with(Box::new(dialect), config, cancel).await
    }
}

// ---------------------------------------------------------------------------------------------
// Gemini Live

/// Gemini Live (`BidiGenerateContent`): PCM16 16 kHz in, 24 kHz out, with session resumption
/// and context compression so its ~10-minute connections don't end the conversation.
pub struct GeminiLive {
    key: kivo_core::Secret<String>,
    /// `wss://generativelanguage.googleapis.com`; tests point it at a local server.
    pub ws_base: String,
}

impl GeminiLive {
    pub fn new(key: kivo_core::Secret<String>) -> Self {
        Self {
            key,
            ws_base: "wss://generativelanguage.googleapis.com".into(),
        }
    }
}

const GEMINI_MODEL: &str = "gemini-2.5-flash-native-audio-preview-09-2025";

struct GeminiDialect {
    key: kivo_core::Secret<String>,
    ws_base: String,
    /// The latest resumption handle.
    handle: Option<String>,
    /// What the user said so far in this turn (Gemini sends it in pieces).
    heard: String,
}

impl GeminiDialect {
    fn flush_heard(&mut self, out: &mut Vec<RtEvent>) {
        let heard = std::mem::take(&mut self.heard);
        if !heard.trim().is_empty() {
            out.push(RtEvent::UserTranscript(heard.trim().to_owned()));
        }
    }
}

impl Dialect for GeminiDialect {
    fn request(&self, _config: &RealtimeConfig) -> Result<Request<()>, NormalizedError> {
        let url = format!(
            "{}/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent",
            self.ws_base.trim_end_matches('/')
        );
        let mut request = url
            .into_client_request()
            .map_err(|e| NormalizedError::Other(e.to_string()))?;
        // The key goes in a header, never in the URL (it would end up in logs).
        request
            .headers_mut()
            .insert("x-goog-api-key", header(self.key.expose())?);
        Ok(request)
    }

    fn setup(&self, config: &RealtimeConfig, carried: &str) -> Vec<String> {
        let model = if config.model.is_empty() {
            GEMINI_MODEL
        } else {
            &config.model
        };
        let voice = if config.voice.is_empty() {
            "Puck"
        } else {
            &config.voice
        };
        let declarations: Vec<Value> = config
            .tools
            .iter()
            .map(|t| json!({ "name": t.name, "description": t.description, "parameters": crate::gemini::schema(&t.params) }))
            .collect();
        // Resuming keeps the provider's own context; a new session gets the conversation so far.
        let (resumption, instructions) = match &self.handle {
            Some(h) => (json!({ "handle": h }), config.instructions.clone()),
            None => (json!({}), with_carried(&config.instructions, carried)),
        };
        let mut setup = json!({
            "model": format!("models/{model}"),
            "generationConfig": {
                "responseModalities": ["AUDIO"],
                "speechConfig": { "voiceConfig": { "prebuiltVoiceConfig": { "voiceName": voice } } }
            },
            "systemInstruction": { "parts": [{ "text": instructions }] },
            "inputAudioTranscription": {},
            "outputAudioTranscription": {},
            "sessionResumption": resumption,
            "contextWindowCompression": { "slidingWindow": {} }
        });
        if !declarations.is_empty() {
            setup["tools"] = json!([{ "functionDeclarations": declarations }]);
        }
        vec![json!({ "setup": setup }).to_string()]
    }

    fn input_rate(&self) -> u32 {
        INPUT_RATE
    }

    fn audio(&self, samples: &[f32]) -> String {
        json!({ "realtimeInput": { "audio": { "data": encode_pcm16(samples), "mimeType": "audio/pcm;rate=16000" } } })
            .to_string()
    }

    fn text(&self, text: &str) -> Vec<String> {
        vec![
            json!({ "clientContent": { "turns": [{ "role": "user", "parts": [{ "text": text }] }], "turnComplete": true } })
                .to_string(),
        ]
    }

    fn tool_result(&self, id: &str, name: &str, output: &str) -> Vec<String> {
        let response = serde_json::from_str::<Value>(output)
            .ok()
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({ "output": output }));
        vec![
            json!({ "toolResponse": { "functionResponses": [{ "id": id, "name": name, "response": response }] } })
                .to_string(),
        ]
    }

    fn parse(&mut self, m: &Value) -> (Vec<RtEvent>, bool) {
        let mut out = Vec::new();
        if m.get("setupComplete").is_some() {
            out.push(RtEvent::Ready);
        }
        if let Some(update) = m.get("sessionResumptionUpdate")
            && update["resumable"].as_bool().unwrap_or(true)
            && let Some(handle) = update["newHandle"].as_str()
            && !handle.is_empty()
        {
            self.handle = Some(handle.to_owned());
        }
        if let Some(content) = m.get("serverContent") {
            if let Some(t) = content["inputTranscription"]["text"].as_str() {
                self.heard.push_str(t);
            }
            if content["interrupted"].as_bool() == Some(true) {
                out.push(RtEvent::UserSpeaking);
            }
            let spoken = content["outputTranscription"]["text"].as_str();
            let parts = content["modelTurn"]["parts"].as_array();
            if spoken.is_some() || parts.is_some() {
                // The model is answering: what the user said is complete.
                self.flush_heard(&mut out);
            }
            for part in parts.into_iter().flatten() {
                let data = &part["inlineData"];
                let mime = data["mimeType"].as_str().unwrap_or_default();
                if mime.starts_with("audio/pcm") {
                    out.push(RtEvent::Audio {
                        pcm: decode_pcm16(data["data"].as_str().unwrap_or_default()),
                        rate: mime_rate(mime, 24_000),
                    });
                }
            }
            if let Some(t) = spoken {
                out.push(RtEvent::ModelTranscript(t.to_owned()));
            }
            if content["turnComplete"].as_bool() == Some(true) {
                self.flush_heard(&mut out);
                out.push(RtEvent::ModelTurnDone);
            }
        }
        if let Some(calls) = m["toolCall"]["functionCalls"].as_array() {
            self.flush_heard(&mut out);
            for call in calls {
                out.push(RtEvent::ToolCall {
                    id: call["id"].as_str().unwrap_or_default().to_owned(),
                    name: call["name"].as_str().unwrap_or_default().to_owned(),
                    args: call.get("args").cloned().unwrap_or_else(|| json!({})),
                });
            }
        }
        if let Some(u) = m.get("usageMetadata") {
            out.push(RtEvent::Usage(Usage {
                input_tokens: u["promptTokenCount"].as_u64().unwrap_or(0),
                output_tokens: u["responseTokenCount"].as_u64().unwrap_or(0),
                cached_tokens: u["cachedContentTokenCount"].as_u64().unwrap_or(0),
            }));
        }
        // The server is about to close this connection: renew it now, with the handle.
        let going = m.get("goAway").is_some();
        (out, going)
    }
}

#[async_trait]
impl RealtimeProvider for GeminiLive {
    fn id(&self) -> &'static str {
        "gemini"
    }
    fn name(&self) -> String {
        "Gemini Live".into()
    }
    fn default_model(&self) -> &'static str {
        GEMINI_MODEL
    }
    fn cost_per_minute(&self) -> f64 {
        0.02
    }
    async fn connect(
        &self,
        config: RealtimeConfig,
        cancel: CancellationToken,
    ) -> Result<RealtimeSession, NormalizedError> {
        let dialect = GeminiDialect {
            key: kivo_core::Secret::new(self.key.expose().clone()),
            ws_base: self.ws_base.clone(),
            handle: None,
            heard: String::new(),
        };
        connect_with(Box::new(dialect), config, cancel).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::handshake::server::{Request as HsRequest, Response};

    type Seen = std::sync::Arc<std::sync::Mutex<Vec<(String, Value)>>>;

    /// Accepts one connection, records the auth header and every message, and plays `script`:
    /// each entry is sent after the client's n-th message (0: right after connecting).
    // tungstenite's handshake callback fixes the closure's error type.
    #[allow(clippy::result_large_err)]
    async fn accept_one(
        listener: &TcpListener,
        seen: Seen,
        script: Vec<(usize, Value)>,
        close_after: bool,
    ) {
        let (stream, _) = listener.accept().await.unwrap();
        let auth = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let a = auth.clone();
        let mut ws =
            tokio_tungstenite::accept_hdr_async(stream, move |req: &HsRequest, res: Response| {
                let h = req
                    .headers()
                    .get("authorization")
                    .or_else(|| req.headers().get("x-goog-api-key"))
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_owned();
                *a.lock().unwrap() = format!("{} {h}", req.uri());
                Ok(res)
            })
            .await
            .unwrap();
        let auth = auth.lock().unwrap().clone();
        let mut received = 0usize;
        let mut script = std::collections::VecDeque::from(script);
        let send_due =
            |received: usize, script: &mut std::collections::VecDeque<(usize, Value)>| {
                let mut due = Vec::new();
                while script.front().is_some_and(|(at, _)| *at <= received) {
                    due.push(script.pop_front().unwrap().1);
                }
                due
            };
        for v in send_due(0, &mut script) {
            ws.send(WsMessage::text(v.to_string())).await.unwrap();
        }
        while let Some(Ok(msg)) = ws.next().await {
            let WsMessage::Text(t) = msg else {
                if matches!(msg, WsMessage::Close(_)) {
                    break;
                }
                continue;
            };
            received += 1;
            seen.lock()
                .unwrap()
                .push((auth.clone(), serde_json::from_str(&t).unwrap()));
            for v in send_due(received, &mut script) {
                ws.send(WsMessage::text(v.to_string())).await.unwrap();
            }
            if close_after && script.is_empty() {
                let _ = ws.close(None).await;
                break;
            }
        }
    }

    fn tool() -> ToolDef {
        ToolDef {
            name: "apps__open".into(),
            description: "Open an app".into(),
            params: json!({ "type": "object", "properties": { "name": { "type": "string" } }, "required": ["name"] }),
        }
    }

    async fn next(events: &mut mpsc::UnboundedReceiver<RtEvent>) -> RtEvent {
        tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .expect("an event in time")
            .expect("the session is open")
    }

    #[test]
    fn audio_round_trips_through_pcm16_and_resamples() {
        let tone: Vec<f32> = (0..160).map(|i| (i as f32 / 10.0).sin() * 0.5).collect();
        let back = decode_pcm16(&encode_pcm16(&tone));
        assert_eq!(back.len(), 160);
        assert!(tone.iter().zip(&back).all(|(a, b)| (a - b).abs() < 1e-3));
        assert_eq!(resample(&tone, 16_000, 24_000).len(), 240);
        assert_eq!(resample(&tone, 24_000, 16_000).len(), 106);
        assert_eq!(mime_rate("audio/pcm;rate=24000", 16_000), 24_000);
        assert_eq!(mime_rate("audio/pcm", 16_000), 16_000);
    }

    #[tokio::test]
    async fn openai_sets_up_streams_audio_reports_tools_and_takes_results() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("ws://{}", listener.local_addr().unwrap());
        let seen: Seen = Default::default();
        let s = seen.clone();
        let audio = encode_pcm16(&[0.25; 240]);
        let server = tokio::spawn(async move {
            accept_one(
                &listener,
                s,
                vec![
                    (1, json!({ "type": "session.updated" })),
                    (2, json!({ "type": "input_audio_buffer.speech_started" })),
                    (2, json!({ "type": "conversation.item.input_audio_transcription.completed", "transcript": " open notepad " })),
                    (2, json!({ "type": "response.function_call_arguments.done", "call_id": "c1", "name": "apps__open", "arguments": "{\"name\":\"Notepad\"}" })),
                    (4, json!({ "type": "response.output_audio.delta", "delta": audio })),
                    (4, json!({ "type": "response.output_audio_transcript.delta", "delta": "Opened it." })),
                    (4, json!({ "type": "response.done", "response": { "usage": { "input_tokens": 120, "output_tokens": 40, "input_token_details": { "cached_tokens": 20 } } } })),
                ],
                false,
            )
            .await;
        });
        let mut provider = OpenAiRealtime::new(kivo_core::Secret::new("sk-test".into()));
        provider.ws_base = base;
        let config = RealtimeConfig {
            instructions: "Be brief.".into(),
            tools: vec![tool()],
            ..RealtimeConfig::default()
        };
        let mut session = provider
            .connect(config, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(next(&mut session.events).await, RtEvent::Ready);
        session.audio.send(vec![0.1; 1_600]).await.unwrap();
        assert_eq!(next(&mut session.events).await, RtEvent::UserSpeaking);
        assert_eq!(
            next(&mut session.events).await,
            RtEvent::UserTranscript("open notepad".into())
        );
        let RtEvent::ToolCall { id, name, args } = next(&mut session.events).await else {
            panic!("a tool call");
        };
        assert_eq!(
            (id.as_str(), name.as_str(), args["name"].as_str()),
            ("c1", "apps__open", Some("Notepad"))
        );
        session
            .control
            .send(Control::ToolResult {
                id,
                name,
                output: "{\"ok\":true}".into(),
            })
            .unwrap();
        let RtEvent::Audio { pcm, rate } = next(&mut session.events).await else {
            panic!("audio");
        };
        assert_eq!((pcm.len(), rate), (240, 24_000));
        assert_eq!(
            next(&mut session.events).await,
            RtEvent::ModelTranscript("Opened it.".into())
        );
        assert_eq!(
            next(&mut session.events).await,
            RtEvent::Usage(Usage {
                input_tokens: 120,
                output_tokens: 40,
                cached_tokens: 20
            })
        );
        assert_eq!(next(&mut session.events).await, RtEvent::ModelTurnDone);
        session.control.send(Control::Close).unwrap();
        assert_eq!(next(&mut session.events).await, RtEvent::Closed(None));
        server.await.unwrap();

        let seen = seen.lock().unwrap();
        assert!(
            seen[0]
                .0
                .starts_with("/v1/realtime?model=gpt-realtime Bearer sk-test")
        );
        let setup = &seen[0].1;
        assert_eq!(setup["type"], "session.update");
        assert_eq!(setup["session"]["instructions"], "Be brief.");
        assert_eq!(setup["session"]["audio"]["input"]["format"]["rate"], 24_000);
        assert_eq!(setup["session"]["tools"][0]["name"], "apps__open");
        // 1,600 samples at 16 kHz arrive as 2,400 at 24 kHz.
        assert_eq!(seen[1].1["type"], "input_audio_buffer.append");
        let sent = decode_pcm16(seen[1].1["audio"].as_str().unwrap());
        assert_eq!(sent.len(), 2_400);
        assert_eq!(seen[2].1["item"]["type"], "function_call_output");
        assert_eq!(seen[2].1["item"]["call_id"], "c1");
        assert_eq!(seen[3].1["type"], "response.create");
    }

    #[tokio::test]
    async fn a_refused_key_fails_the_connect() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("ws://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf).await;
            let body = "{\"error\":{\"message\":\"Incorrect API key\"}}";
            let reply = format!(
                "HTTP/1.1 401 Unauthorized\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(reply.as_bytes()).await.unwrap();
        });
        let mut provider = OpenAiRealtime::new(kivo_core::Secret::new("sk-bad".into()));
        provider.ws_base = base;
        let result = provider
            .connect(RealtimeConfig::default(), CancellationToken::new())
            .await;
        assert!(
            matches!(result, Err(NormalizedError::Auth)),
            "{:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn gemini_streams_and_resumes_with_its_handle_when_the_server_goes_away() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("ws://{}", listener.local_addr().unwrap());
        let seen: Seen = Default::default();
        let s = seen.clone();
        let audio = encode_pcm16(&[0.5; 480]);
        let server = tokio::spawn(async move {
            // First connection: set up, hear the user, answer, hand out a handle, then go away.
            accept_one(
                &listener,
                s.clone(),
                vec![
                    (1, json!({ "setupComplete": {} })),
                    (2, json!({ "serverContent": { "inputTranscription": { "text": "what's the " } } })),
                    (2, json!({ "serverContent": { "inputTranscription": { "text": "weather" } } })),
                    (2, json!({ "serverContent": { "modelTurn": { "parts": [{ "inlineData": { "mimeType": "audio/pcm;rate=24000", "data": audio } }] }, "outputTranscription": { "text": "Sunny." } } })),
                    (2, json!({ "serverContent": { "turnComplete": true }, "usageMetadata": { "promptTokenCount": 50, "responseTokenCount": 9 } })),
                    (2, json!({ "sessionResumptionUpdate": { "newHandle": "h-42", "resumable": true } })),
                    (2, json!({ "goAway": { "timeLeft": "5s" } })),
                ],
                false,
            )
            .await;
            // The adapter comes back with the handle.
            accept_one(
                &listener,
                s,
                vec![
                    (1, json!({ "setupComplete": {} })),
                    (2, json!({ "toolCall": { "functionCalls": [{ "id": "f1", "name": "apps__open", "args": { "name": "Paint" } }] } })),
                ],
                false,
            )
            .await;
        });
        let mut provider = GeminiLive::new(kivo_core::Secret::new("g-key".into()));
        provider.ws_base = base;
        let config = RealtimeConfig {
            instructions: "Be brief.".into(),
            tools: vec![tool()],
            ..RealtimeConfig::default()
        };
        let mut session = provider
            .connect(config, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(next(&mut session.events).await, RtEvent::Ready);
        session.audio.send(vec![0.1; 320]).await.unwrap();
        assert_eq!(
            next(&mut session.events).await,
            RtEvent::UserTranscript("what's the weather".into())
        );
        let RtEvent::Audio { pcm, rate } = next(&mut session.events).await else {
            panic!("audio");
        };
        assert_eq!((pcm.len(), rate), (480, 24_000));
        assert_eq!(
            next(&mut session.events).await,
            RtEvent::ModelTranscript("Sunny.".into())
        );
        assert_eq!(next(&mut session.events).await, RtEvent::ModelTurnDone);
        assert_eq!(
            next(&mut session.events).await,
            RtEvent::Usage(Usage {
                input_tokens: 50,
                output_tokens: 9,
                cached_tokens: 0
            })
        );
        assert_eq!(next(&mut session.events).await, RtEvent::Resumed);
        assert_eq!(next(&mut session.events).await, RtEvent::Ready);
        session.audio.send(vec![0.1; 320]).await.unwrap();
        let RtEvent::ToolCall { id, name, args } = next(&mut session.events).await else {
            panic!("a tool call");
        };
        assert_eq!(
            (id.as_str(), name.as_str(), args["name"].as_str()),
            ("f1", "apps__open", Some("Paint"))
        );
        session
            .control
            .send(Control::ToolResult {
                id,
                name,
                output: "{\"ok\":true}".into(),
            })
            .unwrap();
        session.control.send(Control::Close).unwrap();
        assert_eq!(next(&mut session.events).await, RtEvent::Closed(None));
        server.await.unwrap();

        let seen = seen.lock().unwrap();
        // The key travels in a header, not the URL.
        assert!(
            seen[0].0.ends_with("BidiGenerateContent g-key"),
            "{}",
            seen[0].0
        );
        let first = &seen[0].1["setup"];
        assert_eq!(first["model"], format!("models/{GEMINI_MODEL}"));
        assert_eq!(first["sessionResumption"], json!({}));
        assert!(first["contextWindowCompression"]["slidingWindow"].is_object());
        assert_eq!(
            first["tools"][0]["functionDeclarations"][0]["name"],
            "apps__open"
        );
        assert_eq!(
            seen[1].1["realtimeInput"]["audio"]["mimeType"],
            "audio/pcm;rate=16000"
        );
        // The renewed session resumes with the handle.
        let second = seen
            .iter()
            .filter(|(_, v)| v.get("setup").is_some())
            .nth(1)
            .unwrap();
        assert_eq!(second.1["setup"]["sessionResumption"]["handle"], "h-42");
        let result = seen
            .iter()
            .find(|(_, v)| v.get("toolResponse").is_some())
            .unwrap();
        assert_eq!(result.1["toolResponse"]["functionResponses"][0]["id"], "f1");
        assert_eq!(
            result.1["toolResponse"]["functionResponses"][0]["response"]["ok"],
            true
        );
    }

    #[test]
    fn a_new_session_without_a_handle_carries_the_conversation() {
        let said = vec![
            (true, "let's talk about Rome".to_owned()),
            (false, "Happy to. Ancient or modern?".to_owned()),
        ];
        let carried = carry(&said);
        assert_eq!(
            carried,
            "User: let's talk about Rome\nYou: Happy to. Ancient or modern?"
        );
        let d = GeminiDialect {
            key: kivo_core::Secret::new("k".into()),
            ws_base: String::new(),
            handle: None,
            heard: String::new(),
        };
        let setup: Value =
            serde_json::from_str(&d.setup(&RealtimeConfig::default(), &carried)[0]).unwrap();
        let text = setup["setup"]["systemInstruction"]["parts"][0]["text"]
            .as_str()
            .unwrap();
        assert!(text.contains("User: let's talk about Rome"));
    }
}
