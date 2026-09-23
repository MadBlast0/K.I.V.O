//! KIVO as an Agent Client Protocol client (BRAINS §4, BRAIN-13/14/15): CLI agents (Claude Code
//! through `claude-agent-acp`, Gemini CLI `--acp`, Codex through `codex-acp`, OpenCode, …) run as
//! child processes speaking JSON-RPC 2.0, one message per line, over stdio.
//!
//! - KIVO starts sessions (`session/new`, or `session/load` to resume a stored id, CONV-02),
//!   sends prompts, cancels, and switches modes.
//! - The agent streams `session/update`s: message and thought chunks, tool activity, file
//!   diffs, plans. Thoughts are kept apart and never shown (BRAIN-09).
//! - The agent's `session/request_permission` goes to KIVO's permission engine through a
//!   `PermissionHandler` (BRAIN-15); the agent does its own file and terminal work, which KIVO
//!   shows as activity (so the client offers no fs or terminal capabilities).
//! - If the agent process dies, every waiting call and stream ends with an error; KIVO keeps
//!   running (M3-X3).

use crate::types::NormalizedError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

pub const PROTOCOL_VERSION: u64 = 1;

/// How to start an agent in ACP mode.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// Extra environment (never secrets from KIVO's store: agents sign in themselves).
    #[serde(default)]
    pub env: Vec<(String, String)>,
}

/// An MCP server offered to the agent for this session (KIVO's own tools, BRAINS §4).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServer {
    pub name: String,
    pub command: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<EnvVar>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvVar {
    pub name: String,
    pub value: String,
}

/// What the agent said about itself at `initialize`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentHello {
    pub protocol_version: u64,
    /// It can resume stored sessions (`session/load`).
    pub load_session: bool,
    /// Its sign-in methods, when it needs one (`authenticate`).
    pub auth_methods: Vec<(String, String)>,
}

/// A mode the agent offers (Ask, Code, Architect, bypass …).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMode {
    pub id: String,
    pub name: String,
}

/// A step of the agent's plan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanEntry {
    pub content: String,
    pub status: String,
}

/// What an agent session streams.
#[derive(Clone, Debug, PartialEq)]
pub enum AgentEvent {
    Message(String),
    /// Hidden reasoning (BRAIN-09).
    Thought(String),
    Plan(Vec<PlanEntry>),
    /// The agent's own tool work (reading, editing, running), for Activity.
    Tool {
        id: String,
        title: String,
        kind: String,
        status: String,
    },
    /// A file the agent changed.
    FileDiff {
        path: String,
    },
    ModeChanged(String),
    /// The prompt finished: `end_turn`, `max_tokens`, `refusal`, `cancelled`, …
    Done(String),
    Error(NormalizedError),
}

/// The agent asks to do something (BRAIN-15).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionAsk {
    pub session_id: String,
    pub tool_call_id: String,
    pub title: String,
    /// `read`, `edit`, `delete`, `move`, `search`, `execute`, `fetch`, `think`, `other`.
    pub kind: String,
    /// The agent's choices: id, label, and `allow_once` / `allow_always` / `reject_once` /
    /// `reject_always`.
    pub options: Vec<(String, String, String)>,
}

/// KIVO's answer: one of the agent's option ids, or nothing (the turn was cancelled).
pub type PermissionAnswer = Option<String>;

/// Answers the agent's permission requests; the runtime wires this to its permission engine.
#[async_trait::async_trait]
pub trait PermissionHandler: Send + Sync {
    async fn ask(&self, ask: PermissionAsk) -> PermissionAnswer;
}

/// The option to pick for an allow or a deny.
pub fn pick(ask: &PermissionAsk, allow: bool, always: bool) -> PermissionAnswer {
    let wanted: &[&str] = match (allow, always) {
        (true, true) => &["allow_always", "allow_once"],
        (true, false) => &["allow_once", "allow_always"],
        (false, true) => &["reject_always", "reject_once"],
        (false, false) => &["reject_once", "reject_always"],
    };
    wanted.iter().find_map(|kind| {
        ask.options
            .iter()
            .find(|(_, _, k)| k == kind)
            .map(|(id, _, _)| id.clone())
    })
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, NormalizedError>>>>>;
type Sessions = Arc<Mutex<HashMap<String, mpsc::UnboundedSender<AgentEvent>>>>;

/// A running agent: one connection, any number of sessions.
pub struct AcpClient {
    outgoing: mpsc::UnboundedSender<String>,
    pending: Pending,
    sessions: Sessions,
    next_id: AtomicU64,
    /// Fires when the connection ends (the agent exited or was stopped).
    closed: CancellationToken,
    child: Mutex<Option<Child>>,
    pub hello: Mutex<AgentHello>,
}

impl AcpClient {
    /// Starts the agent process in `cwd` and says hello.
    pub async fn spawn(
        command: &AgentCommand,
        cwd: &Path,
        permissions: Arc<dyn PermissionHandler>,
    ) -> Result<Arc<Self>, NormalizedError> {
        let mut cmd = command_for(&command.program);
        cmd.args(&command.args)
            .current_dir(cwd)
            .envs(command.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        let mut child = cmd
            .spawn()
            .map_err(|e| NormalizedError::ProviderDown(format!("couldn't start the agent: {e}")))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| NormalizedError::Other("no stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| NormalizedError::Other("no stdout".into()))?;
        let client = Self::connect(stdout, stdin, permissions);
        *client
            .child
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(child);
        client.initialize().await?;
        Ok(client)
    }

    /// Talks ACP over any byte streams (tests use in-memory pipes).
    pub fn connect(
        reader: impl AsyncRead + Unpin + Send + 'static,
        mut writer: impl AsyncWrite + Unpin + Send + 'static,
        permissions: Arc<dyn PermissionHandler>,
    ) -> Arc<Self> {
        let (outgoing, mut lines) = mpsc::unbounded_channel::<String>();
        let closed = CancellationToken::new();
        let client = Arc::new(Self {
            outgoing: outgoing.clone(),
            pending: Arc::default(),
            sessions: Arc::default(),
            next_id: AtomicU64::new(1),
            closed: closed.clone(),
            child: Mutex::new(None),
            hello: Mutex::default(),
        });
        // Writer: one JSON message per line.
        let writer_closed = closed.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    line = lines.recv() => {
                        let Some(line) = line else { break };
                        if writer.write_all(line.as_bytes()).await.is_err()
                            || writer.write_all(b"\n").await.is_err()
                            || writer.flush().await.is_err()
                        {
                            break;
                        }
                    }
                    () = writer_closed.cancelled() => break,
                }
            }
        });
        // Reader: responses, notifications and the agent's own requests.
        let pending = Arc::clone(&client.pending);
        let sessions = Arc::clone(&client.sessions);
        tokio::spawn(async move {
            let mut reader = BufReader::new(reader).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let Ok(message) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                dispatch(message, &pending, &sessions, &outgoing, &permissions);
            }
            // The agent is gone: fail everything that waits on it.
            closed.cancel();
            let error = NormalizedError::ProviderDown("the agent stopped".into());
            for (_, waiter) in lock(&pending).drain() {
                let _ = waiter.send(Err(error.clone()));
            }
            for (_, events) in lock(&sessions).drain() {
                let _ = events.send(AgentEvent::Error(error.clone()));
            }
        });
        client
    }

    pub fn is_closed(&self) -> bool {
        self.closed.is_cancelled()
    }

    /// Calls a method and waits for its result.
    pub async fn request(&self, method: &str, params: Value) -> Result<Value, NormalizedError> {
        if self.is_closed() {
            return Err(NormalizedError::ProviderDown("the agent stopped".into()));
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        lock(&self.pending).insert(id, tx);
        let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        if self.outgoing.send(message.to_string()).is_err() {
            lock(&self.pending).remove(&id);
            return Err(NormalizedError::ProviderDown("the agent stopped".into()));
        }
        tokio::select! {
            r = rx => r.unwrap_or_else(|_| Err(NormalizedError::ProviderDown("the agent stopped".into()))),
            () = self.closed.cancelled() => Err(NormalizedError::ProviderDown("the agent stopped".into())),
        }
    }

    fn notify(&self, method: &str, params: Value) {
        let message = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        let _ = self.outgoing.send(message.to_string());
    }

    async fn initialize(&self) -> Result<(), NormalizedError> {
        let result = self
            .request(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "clientCapabilities": { "fs": { "readTextFile": false, "writeTextFile": false }, "terminal": false },
                    "clientInfo": { "name": "kivo", "version": env!("CARGO_PKG_VERSION") },
                }),
            )
            .await?;
        let hello = AgentHello {
            protocol_version: result["protocolVersion"].as_u64().unwrap_or(0),
            load_session: result
                .pointer("/agentCapabilities/loadSession")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            auth_methods: result["authMethods"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|m| {
                    Some((
                        m["id"].as_str()?.to_owned(),
                        m["name"].as_str().unwrap_or_default().to_owned(),
                    ))
                })
                .collect(),
        };
        *lock(&self.hello) = hello;
        Ok(())
    }

    /// Starts a session in `cwd`, or resumes `resume` when the agent can (CONV-02).
    pub async fn session(
        self: &Arc<Self>,
        cwd: &Path,
        mcp_servers: &[McpServer],
        resume: Option<&str>,
    ) -> Result<AcpSession, NormalizedError> {
        let servers = serde_json::to_value(mcp_servers).unwrap_or_else(|_| json!([]));
        let can_load = lock(&self.hello).load_session;
        let (result, id) = match resume.filter(|_| can_load) {
            Some(id) => {
                // Register first: `session/load` replays history as updates.
                let (tx, rx) = mpsc::unbounded_channel();
                lock(&self.sessions).insert(id.to_owned(), tx);
                let result = self
                    .request(
                        "session/load",
                        json!({ "sessionId": id, "cwd": cwd, "mcpServers": servers }),
                    )
                    .await;
                drop(rx);
                (result?, id.to_owned())
            }
            None => {
                let result = self
                    .request("session/new", json!({ "cwd": cwd, "mcpServers": servers }))
                    .await?;
                let id = result["sessionId"]
                    .as_str()
                    .ok_or_else(|| NormalizedError::Other("no session id".into()))?
                    .to_owned();
                (result, id)
            }
        };
        let modes = result
            .pointer("/modes/availableModes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|m| {
                Some(AgentMode {
                    id: m["id"].as_str()?.to_owned(),
                    name: m["name"].as_str().unwrap_or_default().to_owned(),
                })
            })
            .collect();
        let mode = result
            .pointer("/modes/currentModeId")
            .and_then(Value::as_str)
            .map(str::to_owned);
        Ok(AcpSession {
            client: Arc::clone(self),
            id,
            modes,
            mode: Mutex::new(mode),
        })
    }

    /// Stops the agent process.
    pub fn stop(&self) {
        self.closed.cancel();
        if let Some(mut child) = lock(&self.child).take() {
            let _ = child.start_kill();
        }
    }
}

impl Drop for AcpClient {
    fn drop(&mut self) {
        self.stop();
    }
}

/// On Windows, npm installs agents as `.cmd` shims, which must run through `cmd /c`.
fn command_for(program: &Path) -> Command {
    let is_script = program
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"));
    if cfg!(windows) && is_script {
        let mut cmd = Command::new("cmd");
        cmd.arg("/d").arg("/c").arg(program);
        cmd
    } else {
        Command::new(program)
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn dispatch(
    message: Value,
    pending: &Pending,
    sessions: &Sessions,
    outgoing: &mpsc::UnboundedSender<String>,
    permissions: &Arc<dyn PermissionHandler>,
) {
    let method = message.get("method").and_then(Value::as_str);
    let id = message.get("id").cloned();
    match (method, id) {
        // A response to one of KIVO's calls.
        (None, Some(id)) => {
            let Some(id) = id.as_u64() else { return };
            if let Some(waiter) = lock(pending).remove(&id) {
                let result = match message.get("error") {
                    Some(error) => Err(rpc_error(error)),
                    None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
                };
                let _ = waiter.send(result);
            }
        }
        // The agent asks KIVO something.
        (Some(method), Some(id)) => {
            let params = message.get("params").cloned().unwrap_or(Value::Null);
            if method == "session/request_permission" {
                let ask = permission_ask(&params);
                let permissions = Arc::clone(permissions);
                let outgoing = outgoing.clone();
                tokio::spawn(async move {
                    let outcome = match permissions.ask(ask).await {
                        Some(option) => json!({ "outcome": "selected", "optionId": option }),
                        None => json!({ "outcome": "cancelled" }),
                    };
                    let reply =
                        json!({ "jsonrpc": "2.0", "id": id, "result": { "outcome": outcome } });
                    let _ = outgoing.send(reply.to_string());
                });
            } else {
                // KIVO offers no fs or terminal: the agent does that work itself.
                let reply = json!({
                    "jsonrpc": "2.0", "id": id,
                    "error": { "code": -32601, "message": format!("{method} isn't supported by KIVO") },
                });
                let _ = outgoing.send(reply.to_string());
            }
        }
        (Some("session/update"), None) => {
            let params = message.get("params").cloned().unwrap_or(Value::Null);
            let Some(session) = params["sessionId"].as_str() else {
                return;
            };
            let events = update_events(&params["update"]);
            if let Some(tx) = lock(sessions).get(session) {
                for e in events {
                    let _ = tx.send(e);
                }
            }
        }
        _ => {}
    }
}

fn rpc_error(error: &Value) -> NormalizedError {
    let message = error["message"].as_str().unwrap_or("error").to_owned();
    let lower = message.to_lowercase();
    match error["code"].as_i64() {
        // ACP's "authentication required".
        Some(-32000) => NormalizedError::Auth,
        _ if lower.contains("auth") || lower.contains("sign in") || lower.contains("login") => {
            NormalizedError::Auth
        }
        _ if lower.contains("rate limit") || lower.contains("quota") => {
            NormalizedError::RateLimited { retry_after: None }
        }
        _ => NormalizedError::Other(message),
    }
}

fn permission_ask(params: &Value) -> PermissionAsk {
    let call = &params["toolCall"];
    PermissionAsk {
        session_id: params["sessionId"].as_str().unwrap_or_default().to_owned(),
        tool_call_id: call["toolCallId"].as_str().unwrap_or_default().to_owned(),
        title: call["title"]
            .as_str()
            .unwrap_or("The agent wants to act")
            .to_owned(),
        kind: call["kind"].as_str().unwrap_or("other").to_owned(),
        options: params["options"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|o| {
                Some((
                    o["optionId"].as_str()?.to_owned(),
                    o["name"].as_str().unwrap_or_default().to_owned(),
                    o["kind"].as_str().unwrap_or_default().to_owned(),
                ))
            })
            .collect(),
    }
}

fn text_of(content: &Value) -> Option<String> {
    (content["type"] == "text")
        .then(|| content["text"].as_str().map(str::to_owned))
        .flatten()
}

/// The events one `session/update` carries.
pub fn update_events(update: &Value) -> Vec<AgentEvent> {
    let mut out = Vec::new();
    match update["sessionUpdate"].as_str().unwrap_or_default() {
        "agent_message_chunk" => out.extend(text_of(&update["content"]).map(AgentEvent::Message)),
        "agent_thought_chunk" => out.extend(text_of(&update["content"]).map(AgentEvent::Thought)),
        "plan" => out.push(AgentEvent::Plan(
            update["entries"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|e| PlanEntry {
                    content: e["content"].as_str().unwrap_or_default().to_owned(),
                    status: e["status"].as_str().unwrap_or("pending").to_owned(),
                })
                .collect(),
        )),
        kind @ ("tool_call" | "tool_call_update") => {
            let id = update["toolCallId"].as_str().unwrap_or_default().to_owned();
            if kind == "tool_call"
                || update.get("title").is_some()
                || update.get("status").is_some()
            {
                out.push(AgentEvent::Tool {
                    id,
                    title: update["title"].as_str().unwrap_or_default().to_owned(),
                    kind: update["kind"].as_str().unwrap_or("other").to_owned(),
                    status: update["status"].as_str().unwrap_or("pending").to_owned(),
                });
            }
            for item in update["content"].as_array().into_iter().flatten() {
                if item["type"] == "diff"
                    && let Some(path) = item["path"].as_str()
                {
                    out.push(AgentEvent::FileDiff {
                        path: path.to_owned(),
                    });
                }
            }
        }
        "current_mode_update" => {
            if let Some(mode) = update["currentModeId"].as_str() {
                out.push(AgentEvent::ModeChanged(mode.to_owned()));
            }
        }
        _ => {}
    }
    out
}

/// A session with a CLI agent (the `AgentSession` of BRAINS §3).
pub struct AcpSession {
    client: Arc<AcpClient>,
    id: String,
    pub modes: Vec<AgentMode>,
    mode: Mutex<Option<String>>,
}

impl AcpSession {
    /// The agent's session id, stored so KIVO can resume it (CONV-02).
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn mode(&self) -> Option<String> {
        lock(&self.mode).clone()
    }

    /// Sends a prompt; the answer streams on the returned channel and ends with `Done` or
    /// `Error`. Cancelling `cancel` sends `session/cancel`; the agent then ends with
    /// `Done("cancelled")`.
    pub fn prompt(
        &self,
        text: &str,
        cancel: CancellationToken,
    ) -> mpsc::UnboundedReceiver<AgentEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        lock(&self.client.sessions).insert(self.id.clone(), tx.clone());
        let client = Arc::clone(&self.client);
        let id = self.id.clone();
        let params = json!({ "sessionId": id, "prompt": [{ "type": "text", "text": text }] });
        tokio::spawn(async move {
            let call = client.request("session/prompt", params);
            tokio::pin!(call);
            let result = tokio::select! {
                r = &mut call => r,
                () = cancel.cancelled() => {
                    client.notify("session/cancel", json!({ "sessionId": id }));
                    // The agent answers the prompt with `cancelled`; don't wait forever for it.
                    match tokio::time::timeout(std::time::Duration::from_secs(5), call).await {
                        Ok(r) => r,
                        Err(_) => Ok(json!({ "stopReason": "cancelled" })),
                    }
                }
            };
            let last = match result {
                Ok(v) => {
                    AgentEvent::Done(v["stopReason"].as_str().unwrap_or("end_turn").to_owned())
                }
                Err(e) => AgentEvent::Error(e),
            };
            let _ = tx.send(last);
        });
        rx
    }

    /// Switches the agent's mode (BRAINS §3 `set_mode`).
    pub async fn set_mode(&self, mode: &str) -> Result<(), NormalizedError> {
        self.client
            .request(
                "session/set_mode",
                json!({ "sessionId": self.id, "modeId": mode }),
            )
            .await?;
        *lock(&self.mode) = Some(mode.to_owned());
        Ok(())
    }
}
