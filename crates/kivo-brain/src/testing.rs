//! Test doubles for brains: a small HTTP server that plays recorded provider responses to the
//! real adapters (contract tests), and `ScriptedBrain`, a provider that answers from a script for
//! the runtime's end-to-end tests. Neither reaches the network.

use crate::provider::{BrainProvider, BrainStream, channel};
use crate::types::{
    BrainEvent, ChatRequest, ModelInfo, NormalizedError, PrivacyClass, ProviderInfo, ProviderKind,
    StopReason,
};
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

/// A request the mock server received.
#[derive(Clone, Debug)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Value,
}

impl Recorded {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// What the mock server answers.
#[derive(Clone, Debug)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    /// Sent in these pieces, `pause` apart.
    pub chunks: Vec<Vec<u8>>,
    pub pause: Duration,
    /// Keep the connection open after the last chunk (a stream that never ends).
    pub hang: bool,
}

impl Reply {
    pub fn json(status: u16, body: &Value) -> Self {
        Self {
            status,
            headers: vec![("content-type".into(), "application/json".into())],
            chunks: vec![body.to_string().into_bytes()],
            pause: Duration::ZERO,
            hang: false,
        }
    }

    /// Server-sent events, one chunk per event.
    pub fn sse(events: &[&str]) -> Self {
        Self {
            status: 200,
            headers: vec![("content-type".into(), "text/event-stream".into())],
            chunks: events
                .iter()
                .map(|e| format!("{e}\n\n").into_bytes())
                .collect(),
            pause: Duration::from_millis(5),
            hang: false,
        }
    }
}

pub struct MockServer {
    pub url: String,
    requests: Arc<Mutex<Vec<Recorded>>>,
}

impl MockServer {
    /// Starts a server on a free localhost port; `route` answers each request.
    pub async fn start(route: impl Fn(&Recorded) -> Reply + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a local port");
        let url = format!("http://{}", listener.local_addr().expect("an address"));
        let requests: Arc<Mutex<Vec<Recorded>>> = Arc::default();
        let seen = Arc::clone(&requests);
        let route = Arc::new(route);
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let seen = Arc::clone(&seen);
                let route = Arc::clone(&route);
                tokio::spawn(async move {
                    let Some(request) = read_request(&mut socket).await else {
                        return;
                    };
                    seen.lock().expect("requests").push(request.clone());
                    let reply = route(&request);
                    let mut head = format!("HTTP/1.1 {} X\r\nconnection: close\r\n", reply.status);
                    for (n, v) in &reply.headers {
                        head.push_str(&format!("{n}: {v}\r\n"));
                    }
                    head.push_str("\r\n");
                    if socket.write_all(head.as_bytes()).await.is_err() {
                        return;
                    }
                    for chunk in &reply.chunks {
                        if socket.write_all(chunk).await.is_err() {
                            return;
                        }
                        let _ = socket.flush().await;
                        tokio::time::sleep(reply.pause).await;
                    }
                    if reply.hang {
                        // Until the client goes away.
                        let mut buf = [0u8; 64];
                        while matches!(socket.read(&mut buf).await, Ok(n) if n > 0) {}
                    }
                });
            }
        });
        Self { url, requests }
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().expect("requests").clone()
    }
}

async fn read_request(socket: &mut tokio::net::TcpStream) -> Option<Recorded> {
    let mut data = Vec::new();
    let mut buf = [0u8; 4096];
    let head_end = loop {
        let n = socket.read(&mut buf).await.ok()?;
        if n == 0 {
            return None;
        }
        data.extend_from_slice(&buf[..n]);
        if let Some(i) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let head = String::from_utf8_lossy(&data[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let method = first.next()?.to_owned();
    let path = first.next()?.to_owned();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_lowercase(), v.trim().to_owned()))
        .collect();
    let length = headers
        .iter()
        .find(|(n, _)| n == "content-length")
        .and_then(|(_, v)| v.parse::<usize>().ok())
        .unwrap_or(0);
    while data.len() < head_end + length {
        let n = socket.read(&mut buf).await.ok()?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
    }
    let body = serde_json::from_slice(&data[head_end..]).unwrap_or(Value::Null);
    Some(Recorded {
        method,
        path,
        headers,
        body,
    })
}

/// One scripted answer: events to stream, optionally slowly, or never ending.
#[derive(Clone, Debug)]
pub struct Script {
    pub events: Vec<BrainEvent>,
    pub pause: Duration,
    pub hang: bool,
}

impl Script {
    /// A plain text answer in a few pieces.
    pub fn text(text: &str) -> Self {
        let mut events: Vec<BrainEvent> = text
            .split_inclusive(' ')
            .map(|w| BrainEvent::TextDelta(w.to_owned()))
            .collect();
        events.push(BrainEvent::Usage(crate::types::Usage {
            input_tokens: 100,
            output_tokens: 20,
            cached_tokens: 0,
        }));
        events.push(BrainEvent::Done(StopReason::EndTurn));
        Self {
            events,
            pause: Duration::from_millis(2),
            hang: false,
        }
    }

    /// A tool call, then (on the next request) whatever comes next in the script.
    pub fn tool(name: &str, args: Value) -> Self {
        Self {
            events: vec![
                BrainEvent::ToolCall {
                    id: format!("call_{name}"),
                    name: name.to_owned(),
                    args,
                },
                BrainEvent::Done(StopReason::ToolUse),
            ],
            pause: Duration::ZERO,
            hang: false,
        }
    }

    /// Several tool calls in one round (the brain asks for them together).
    pub fn parallel(calls: Vec<(&str, Value)>) -> Self {
        let mut events: Vec<BrainEvent> = calls
            .into_iter()
            .enumerate()
            .map(|(i, (name, args))| BrainEvent::ToolCall {
                id: format!("call_{i}_{name}"),
                name: name.to_owned(),
                args,
            })
            .collect();
        events.push(BrainEvent::Done(StopReason::ToolUse));
        Self {
            events,
            pause: Duration::ZERO,
            hang: false,
        }
    }

    pub fn error(error: NormalizedError) -> Self {
        Self {
            events: vec![BrainEvent::Error(error)],
            pause: Duration::ZERO,
            hang: false,
        }
    }
}

/// A brain that answers from a script, recording what it was asked.
pub struct ScriptedBrain {
    pub info: ProviderInfo,
    script: Mutex<VecDeque<Script>>,
    pub requests: Mutex<Vec<ChatRequest>>,
    /// Its model takes images (for the screenshot journeys).
    pub vision: std::sync::atomic::AtomicBool,
}

impl ScriptedBrain {
    pub fn new(id: &str, privacy: PrivacyClass, script: Vec<Script>) -> Self {
        Self {
            info: ProviderInfo {
                id: id.into(),
                name: id.into(),
                kind: if privacy == PrivacyClass::Local {
                    ProviderKind::Local
                } else {
                    ProviderKind::Api
                },
                privacy,
                free: privacy == PrivacyClass::Local,
            },
            script: Mutex::new(script.into()),
            requests: Mutex::default(),
            vision: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub fn push(&self, script: Script) {
        self.script.lock().expect("script").push_back(script);
    }

    pub fn push_all(&self, scripts: Vec<Script>) {
        self.script.lock().expect("script").extend(scripts);
    }
}

#[async_trait::async_trait]
impl BrainProvider for ScriptedBrain {
    fn info(&self) -> ProviderInfo {
        self.info.clone()
    }

    async fn models(&self) -> Result<Vec<ModelInfo>, NormalizedError> {
        let mut model = ModelInfo::named("scripted");
        model.vision = self.vision.load(std::sync::atomic::Ordering::SeqCst);
        Ok(vec![model])
    }

    fn chat(&self, request: ChatRequest, cancel: CancellationToken) -> BrainStream {
        self.requests.lock().expect("requests").push(request);
        let script = self
            .script
            .lock()
            .expect("script")
            .pop_front()
            .unwrap_or_else(|| Script::text("(no more answers)"));
        let (tx, rx) = channel();
        tokio::spawn(async move {
            for event in script.events {
                tokio::select! {
                    () = tokio::time::sleep(script.pause) => {}
                    () = cancel.cancelled() => {
                        let _ = tx.send(BrainEvent::Error(NormalizedError::Cancelled)).await;
                        return;
                    }
                }
                if tx.send(event).await.is_err() {
                    return;
                }
            }
            if script.hang {
                cancel.cancelled().await;
                let _ = tx.send(BrainEvent::Error(NormalizedError::Cancelled)).await;
            }
        });
        rx
    }
}
