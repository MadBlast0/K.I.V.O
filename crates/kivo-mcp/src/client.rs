//! The MCP client (TOOL-34): `rmcp` over stdio (a program on this PC) or Streamable HTTP (a
//! remote server). A connection lists the server's tools, calls them with a timeout and
//! cancellation, and reports `tools/list_changed` so KIVO can compare the tools again (TOOL-36).

use crate::config::{EnvValue, McpServer, Transport};
use crate::hash::tool_hash;
use rmcp::model::{
    CallToolRequestParams, ClientCapabilities, ClientConfig, ContentBlock, Implementation,
};
use rmcp::service::{NotificationContext, RunningService};
use rmcp::{ClientHandler, RoleClient, ServiceExt};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

/// How long connecting (starting the program, the handshake) may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// The most text a tool's result may bring back; the rest is cut (a hostile server can't flood
/// the model's context, TOOL-41).
pub const MAX_RESULT_CHARS: usize = 64_000;
/// The most a tool's description may say to the model (longer descriptions are cut).
pub const MAX_DESCRIPTION_CHARS: usize = 1_000;

#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("{0}")]
    Connect(String),
    #[error("{0}")]
    Call(String),
    #[error("the server took too long")]
    Timeout,
    #[error("cancelled")]
    Cancelled,
    #[error("a secret it needs isn't in Credential Manager: {0}")]
    MissingSecret(String),
    #[error("it needs you to sign in again")]
    SignIn,
}

/// One of a server's tools, as it describes itself (untrusted text).
#[derive(Clone, Debug, PartialEq)]
pub struct RemoteTool {
    pub name: String,
    pub title: Option<String>,
    pub description: String,
    pub schema: Value,
    /// Its fingerprint (name, description, schema), compared with what the user approved.
    pub hash: String,
    /// The server marks it read-only (a hint only; KIVO never lowers the risk because of it).
    pub read_only_hint: bool,
}

/// What a call returned.
#[derive(Clone, Debug, PartialEq)]
pub struct CallOutcome {
    /// The text content (and structured content as JSON), cut at [`MAX_RESULT_CHARS`].
    pub text: String,
    pub truncated: bool,
    /// The server said the call failed.
    pub is_error: bool,
    /// Images and other media left out (counted, not passed on).
    pub media: usize,
}

/// Where a connection's secrets come from (Credential Manager in KIVO).
pub trait Vault: Send + Sync {
    /// A secret by its entry name (an env variable's value, a bearer token).
    fn secret(&self, name: &str) -> Option<String>;
    /// The store for a signed-in server's OAuth tokens.
    fn tokens(&self, name: &str) -> Arc<dyn crate::auth::TokenStore>;
}

/// Counts `tools/list_changed` notifications.
struct Handler {
    changed: watch::Sender<u64>,
}

impl ClientHandler for Handler {
    fn on_tool_list_changed(
        &self,
        _context: NotificationContext<RoleClient>,
    ) -> impl Future<Output = ()> + Send + '_ {
        self.changed.send_modify(|n| *n += 1);
        std::future::ready(())
    }

    fn get_info(&self) -> ClientConfig {
        ClientConfig::new(
            ClientCapabilities::default(),
            Implementation::new("kivo", env!("CARGO_PKG_VERSION")),
        )
    }
}

/// A live connection to a server.
pub struct Connection {
    pub server: String,
    service: tokio::sync::Mutex<Option<RunningService<RoleClient, Handler>>>,
    peer: rmcp::Peer<RoleClient>,
    changed: watch::Receiver<u64>,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection")
            .field("server", &self.server)
            .finish_non_exhaustive()
    }
}

/// A program as Windows starts it: a `.cmd` or `.bat` (npx, uvx shims) runs through `cmd /c`,
/// found on PATH like a shell would.
fn program(command: &str, args: &[String]) -> (PathBuf, Vec<String>) {
    let found = find_on_path(command).unwrap_or_else(|| PathBuf::from(command));
    let is_script = found
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"));
    if cfg!(windows) && is_script {
        let mut all = vec![
            "/d".to_owned(),
            "/c".to_owned(),
            found.display().to_string(),
        ];
        all.extend(args.iter().cloned());
        (PathBuf::from("cmd.exe"), all)
    } else {
        (found, args.to_vec())
    }
}

/// `command` on PATH, trying Windows' program extensions when it has none.
pub fn find_on_path(command: &str) -> Option<PathBuf> {
    let direct = PathBuf::from(command);
    if direct.components().count() > 1 {
        return direct.is_file().then_some(direct);
    }
    let exts: Vec<String> = if cfg!(windows) && direct.extension().is_none() {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
            .split(';')
            .filter(|e| !e.is_empty())
            .map(str::to_owned)
            .collect()
    } else {
        vec![String::new()]
    };
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        exts.iter().find_map(|ext| {
            let p = dir.join(format!("{command}{ext}"));
            p.is_file().then_some(p)
        })
    })
}

impl Connection {
    /// Connects to `server`, its secrets from `vault`.
    pub async fn connect(
        server: &McpServer,
        vault: &dyn Vault,
        cancel: &CancellationToken,
    ) -> Result<Arc<Self>, McpError> {
        let secret = |name: &str| vault.secret(name);
        let (tx, rx) = watch::channel(0u64);
        let handler = Handler { changed: tx };
        let connecting = async {
            match &server.transport {
                Transport::Stdio {
                    command,
                    args,
                    env,
                    cwd,
                } => {
                    let (exe, args) = program(command, args);
                    let mut cmd = tokio::process::Command::new(exe);
                    cmd.args(&args).kill_on_drop(true);
                    #[cfg(windows)]
                    {
                        // No console window pops up for the server.
                        cmd.creation_flags(0x0800_0000);
                    }
                    if let Some(dir) = cwd {
                        cmd.current_dir(dir);
                    }
                    for (k, v) in env {
                        let value = match v {
                            EnvValue::Plain(s) => s.clone(),
                            EnvValue::Secret(name) => {
                                secret(name).ok_or_else(|| McpError::MissingSecret(name.clone()))?
                            }
                        };
                        cmd.env(k, value);
                    }
                    let (transport, _stderr) = rmcp::transport::TokioChildProcess::builder(cmd)
                        .stderr(std::process::Stdio::null())
                        .spawn()
                        .map_err(|e| McpError::Connect(e.to_string()))?;
                    handler
                        .serve(transport)
                        .await
                        .map_err(|e| McpError::Connect(e.to_string()))
                }
                Transport::Http { url, token, oauth } => {
                    use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
                    let mut config = StreamableHttpClientTransportConfig::with_uri(url.as_str());
                    if let Some(name) = oauth {
                        // Signed in: the tokens are added to each request and refreshed.
                        let mut manager = crate::auth::manager(url, vault.tokens(name))
                            .await
                            .map_err(|e| McpError::Connect(e.to_string()))?;
                        let signed_in = manager
                            .initialize_from_store()
                            .await
                            .map_err(|e| McpError::Connect(e.to_string()))?;
                        if !signed_in {
                            return Err(McpError::SignIn);
                        }
                        let client =
                            rmcp::transport::auth::AuthClient::new(reqwest::Client::new(), manager);
                        let transport = rmcp::transport::StreamableHttpClientTransport::with_client(
                            client, config,
                        );
                        return handler
                            .serve(transport)
                            .await
                            .map_err(|e| McpError::Connect(e.to_string()));
                    }
                    if let Some(name) = token {
                        let t =
                            secret(name).ok_or_else(|| McpError::MissingSecret(name.clone()))?;
                        config = config.auth_header(t);
                    }
                    let transport =
                        rmcp::transport::StreamableHttpClientTransport::from_config(config);
                    handler
                        .serve(transport)
                        .await
                        .map_err(|e| McpError::Connect(e.to_string()))
                }
            }
        };
        let service = tokio::select! {
            r = tokio::time::timeout(CONNECT_TIMEOUT, connecting) => r.map_err(|_| McpError::Timeout)??,
            () = cancel.cancelled() => return Err(McpError::Cancelled),
        };
        let peer = service.peer().clone();
        Ok(Arc::new(Self {
            server: server.id.clone(),
            service: tokio::sync::Mutex::new(Some(service)),
            peer,
            changed: rx,
        }))
    }

    /// The server's tools now.
    pub async fn tools(&self) -> Result<Vec<RemoteTool>, McpError> {
        let tools = tokio::time::timeout(CONNECT_TIMEOUT, self.peer.list_all_tools())
            .await
            .map_err(|_| McpError::Timeout)?
            .map_err(|e| McpError::Call(e.to_string()))?;
        Ok(tools
            .into_iter()
            .map(|t| {
                let description = t.description.as_deref().unwrap_or_default().to_owned();
                let schema = Value::Object((*t.input_schema).clone());
                RemoteTool {
                    hash: tool_hash(&t.name, &description, &schema),
                    read_only_hint: t
                        .annotations
                        .as_ref()
                        .and_then(|a| a.read_only_hint)
                        .unwrap_or(false),
                    name: t.name.into_owned(),
                    title: t.title,
                    description,
                    schema,
                }
            })
            .collect())
    }

    /// Calls a tool, giving up after `timeout` or when `cancel` fires.
    pub async fn call(
        &self,
        name: &str,
        args: &Value,
        timeout: Duration,
        cancel: &CancellationToken,
    ) -> Result<CallOutcome, McpError> {
        let mut params = CallToolRequestParams::new(name.to_owned());
        params.arguments = args.as_object().cloned();
        let call = self.peer.call_tool(params);
        let result = tokio::select! {
            r = tokio::time::timeout(timeout, call) => r.map_err(|_| McpError::Timeout)?.map_err(|e| McpError::Call(e.to_string()))?,
            () = cancel.cancelled() => return Err(McpError::Cancelled),
        };
        let mut text = String::new();
        let mut media = 0;
        for block in &result.content {
            match block {
                ContentBlock::Text(t) => {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(&t.text);
                }
                _ => media += 1,
            }
        }
        if let Some(structured) = &result.structured_content
            && text.is_empty()
        {
            text = structured.to_string();
        }
        let (text, truncated) = clip(&text, MAX_RESULT_CHARS);
        Ok(CallOutcome {
            text,
            truncated,
            is_error: result.is_error.unwrap_or(false),
            media,
        })
    }

    /// Counts the server's `tools/list_changed` notifications.
    pub fn tools_changed(&self) -> watch::Receiver<u64> {
        self.changed.clone()
    }

    /// Ends the connection (the program is stopped).
    pub async fn close(&self) {
        if let Some(mut service) = self.service.lock().await.take() {
            let _ = service.close().await;
        }
    }
}

/// `text` cut to `max` characters (on a character boundary).
pub fn clip(text: &str, max: usize) -> (String, bool) {
    if text.chars().count() <= max {
        return (text.to_owned(), false);
    }
    (text.chars().take(max).collect(), true)
}
