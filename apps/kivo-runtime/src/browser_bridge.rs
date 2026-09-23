//! The browser bridge (TOOLS_AND_CONTROL §5, TOOL-24, SEC-30): KIVO's extension in the user's own
//! browser talks to the runtime through a native messaging host.
//!
//! - Chrome, Edge or Brave start `kivo-runtime chrome-extension://<id>/` (the host) when the
//!   extension connects. The host accepts only KIVO's own extension, reads the session token,
//!   connects to the runtime's user-only `…-browser` pipe, says hello with the token, and then
//!   relays frames both ways. Chrome frames messages exactly like KIVO's IPC (a little-endian
//!   `u32` length, then JSON), and every frame is limited to 1 MiB.
//! - The runtime side is a JSON-RPC peer: it asks the extension for tabs, a page excerpt, the
//!   selection, and clicks or typing; `BrowserBridge` is the `kivo_tools::Browser` the tools use.
//! - The host manifest is registered per user, for the three browsers, only while "Browser: read &
//!   act on pages" is on.

use futures_util::{SinkExt, StreamExt};
use kivo_core::text;
use kivo_core::tool::{ToolError, ToolErrorCode};
use kivo_ipc::{Incoming, Peer, SessionToken, frame_codec};
use kivo_tools::browser::{Browser, Page, PageTarget, Tab};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio_util::codec::{FramedRead, FramedWrite};
use tokio_util::sync::CancellationToken;

/// KIVO's extension (the id follows from the public key in `extensions/browser/manifest.json`).
pub const EXTENSION_ID: &str = "ohaefaipbiknncgdjdpkjgaiocpnlloc";
/// The native messaging host's name.
pub const HOST_NAME: &str = "com.kivo.bridge";
/// How long a request to the extension may take.
const CALL_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a new connection has to say hello with the token.
const HELLO_TIMEOUT: Duration = Duration::from_secs(3);

/// The only origin allowed to start the host.
pub fn origin() -> String {
    format!("chrome-extension://{EXTENSION_ID}/")
}

/// The bridge's pipe, beside the UI pipe.
pub fn pipe_name(endpoint: &str) -> String {
    format!("{endpoint}-browser")
}

/// The runtime's end: the extension, when connected.
pub struct BrowserBridge {
    peer: RwLock<Option<Peer>>,
    handle: tokio::runtime::Handle,
}

impl BrowserBridge {
    /// Listens on the bridge pipe until `shutdown`.
    #[cfg(windows)]
    pub fn start(endpoint: &str, token: SessionToken, shutdown: CancellationToken) -> Arc<Self> {
        let bridge = Arc::new(Self {
            peer: RwLock::new(None),
            handle: tokio::runtime::Handle::current(),
        });
        let name = pipe_name(endpoint);
        let me = Arc::clone(&bridge);
        tokio::spawn(async move {
            let mut first = true;
            loop {
                let server = match kivo_ipc::transport::create_server(&name, first) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!(%e, "the browser bridge couldn't open its pipe");
                        return;
                    }
                };
                first = false;
                tokio::select! {
                    () = shutdown.cancelled() => return,
                    connected = server.connect() => {
                        if connected.is_err() {
                            continue;
                        }
                    }
                }
                let me = Arc::clone(&me);
                let token = token.clone();
                tokio::spawn(async move { me.serve(server, &token).await });
            }
        });
        bridge
    }

    /// A bridge with no extension (tests, other platforms).
    pub fn disconnected() -> Arc<Self> {
        Arc::new(Self {
            peer: RwLock::new(None),
            handle: tokio::runtime::Handle::current(),
        })
    }

    /// One host connection: hello with the token first, then the extension is the peer.
    pub async fn serve<S>(self: &Arc<Self>, stream: S, token: &SessionToken)
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + 'static,
    {
        let (peer, mut incoming) = Peer::spawn(stream);
        let hello = tokio::time::timeout(HELLO_TIMEOUT, incoming.recv()).await;
        let ok = matches!(
            hello,
            Ok(Some(Incoming::Notification(n)))
                if n.method == "bridge.hello"
                    && n.params["token"].as_str().is_some_and(|t| token.matches(t))
                    && n.params["origin"].as_str() == Some(origin().as_str())
        );
        if !ok {
            tracing::warn!("refused a browser bridge connection without the session token");
            return;
        }
        tracing::info!("the browser extension connected");
        *self
            .peer
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(peer.clone());
        // The extension only answers; anything it asks is refused.
        while let Some(message) = incoming.recv().await {
            if let Incoming::Request(r) = message {
                let _ = peer.respond(
                    r.id,
                    Err(kivo_ipc::RpcError {
                        code: kivo_ipc::RpcError::METHOD_NOT_FOUND,
                        message: "not allowed".into(),
                    }),
                );
            }
        }
        let mut current = self
            .peer
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if current.as_ref().is_some_and(Peer::is_closed) {
            *current = None;
        }
        tracing::info!("the browser extension disconnected");
    }

    /// A request to the extension, from a tool's blocking thread.
    fn call(&self, method: &str, params: Value) -> Result<Value, ToolError> {
        let peer = self
            .peer
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .filter(|p| !p.is_closed())
            .ok_or_else(kivo_tools::browser::no_extension)?;
        let method = method.to_owned();
        let answer = self.handle.block_on(async move {
            tokio::time::timeout(CALL_TIMEOUT, peer.request(&method, params)).await
        });
        match answer {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(kivo_ipc::PeerError::Remote(e))) => Err(page_error(&e.message)),
            Ok(Err(kivo_ipc::PeerError::Closed)) => Err(kivo_tools::browser::no_extension()),
            Err(_) => Err(ToolError::new(
                ToolErrorCode::Timeout,
                text::t("error.timeout"),
            )),
        }
    }
}

/// The extension's error codes, worded for people.
fn page_error(code: &str) -> ToolError {
    let (c, key) = match code {
        "password" => (ToolErrorCode::AccessDenied, "error.uia.password"),
        "notFound" => (ToolErrorCode::NotFound, "error.browser.noElement"),
        "notAWebPage" => (ToolErrorCode::Unsupported, "error.browser.notAWebPage"),
        "noTab" => (ToolErrorCode::NotFound, "error.browser.noTab"),
        "tooLarge" => (ToolErrorCode::Failed, "error.browser.tooLarge"),
        _ => (ToolErrorCode::Failed, "error.failed"),
    };
    ToolError::new(c, text::t(key)).with_detail(code.to_owned())
}

fn parse<T: serde::de::DeserializeOwned>(v: Value) -> Result<T, ToolError> {
    serde_json::from_value(v).map_err(|e| {
        ToolError::new(ToolErrorCode::Failed, text::t("error.failed")).with_detail(e.to_string())
    })
}

impl Browser for BrowserBridge {
    fn connected(&self) -> bool {
        self.peer
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .is_some_and(|p| !p.is_closed())
    }
    fn tabs(&self) -> Result<Vec<Tab>, ToolError> {
        parse(self.call("tabs", json!({}))?)
    }
    fn read(&self, tab: Option<i64>) -> Result<Page, ToolError> {
        parse(self.call("read", json!({ "tab": tab }))?)
    }
    fn selection(&self, tab: Option<i64>) -> Result<String, ToolError> {
        parse(self.call("selection", json!({ "tab": tab }))?)
    }
    fn click(&self, tab: Option<i64>, target: &PageTarget) -> Result<String, ToolError> {
        let v = self.call(
            "click",
            json!({ "tab": tab, "selector": target.selector, "text": target.text }),
        )?;
        Ok(v["clicked"].as_str().unwrap_or_default().to_owned())
    }
    fn type_text(
        &self,
        tab: Option<i64>,
        target: &PageTarget,
        value: &str,
        submit: bool,
    ) -> Result<(), ToolError> {
        self.call(
            "type",
            json!({ "tab": tab, "selector": target.selector, "text": target.text, "value": value, "submit": submit }),
        )
        .map(|_| ())
    }
}

/// Where KIVO's extension files are (to load it unpacked until it's in the stores): beside the
/// runtime when installed, else in the source tree (development).
pub fn extension_folder() -> Option<PathBuf> {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|d| d.join("extensions").join("browser")));
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../extensions/browser");
    [beside, Some(source)]
        .into_iter()
        .flatten()
        .find(|d| d.join("manifest.json").is_file())
        .map(|d| d.canonicalize().unwrap_or(d))
}

/// The host manifest's path.
pub fn manifest_path(local: &Path) -> PathBuf {
    local
        .join("native-messaging")
        .join(format!("{HOST_NAME}.json"))
}

/// Registers the host for Chrome, Edge and Brave (per user), or removes it.
#[cfg(windows)]
pub fn set_registered(on: bool, local: &Path) -> std::io::Result<()> {
    use kivo_platform_windows::native_messaging::{BROWSER_KEYS, manifest, register, unregister};
    let path = manifest_path(local);
    if on {
        let program = std::env::current_exe()?;
        let json = manifest(HOST_NAME, "KIVO", &program, &origin());
        register(BROWSER_KEYS, HOST_NAME, &path, &json)
    } else {
        unregister(BROWSER_KEYS, HOST_NAME, &path)
    }
}

/// The native messaging host (`kivo-runtime chrome-extension://<id>/`): relays frames between the
/// browser (stdin/stdout) and the runtime's bridge pipe. Exits when either side closes.
#[cfg(windows)]
pub async fn run_host(caller: &str, endpoint: &str, token_file: &Path) -> std::process::ExitCode {
    use std::process::ExitCode;
    // Only KIVO's own extension (SEC-30); the manifest allows no other, and this checks again.
    if caller != origin() {
        eprintln!("kivo-runtime: refusing native messaging from {caller}");
        return ExitCode::from(3);
    }
    let Ok(token) = SessionToken::read(token_file) else {
        // KIVO isn't running: the extension retries later.
        return ExitCode::from(4);
    };
    let Ok(pipe) = kivo_ipc::transport::connect(&pipe_name(endpoint)).await else {
        return ExitCode::from(4);
    };
    if relay(
        tokio::io::stdin(),
        tokio::io::stdout(),
        pipe,
        &token,
        caller,
    )
    .await
    .is_err()
    {
        return ExitCode::from(4);
    }
    ExitCode::SUCCESS
}

/// Says hello to the runtime with the token, then copies frames both ways until either side
/// closes or sends a frame over 1 MiB.
pub async fn relay<I, O, P>(
    browser_in: I,
    browser_out: O,
    pipe: P,
    token: &SessionToken,
    caller: &str,
) -> std::io::Result<()>
where
    I: tokio::io::AsyncRead + Unpin,
    O: tokio::io::AsyncWrite + Unpin,
    P: tokio::io::AsyncRead + tokio::io::AsyncWrite,
{
    let (pipe_read, pipe_write) = tokio::io::split(pipe);
    let mut to_runtime = FramedWrite::new(pipe_write, frame_codec());
    let mut from_runtime = FramedRead::new(pipe_read, frame_codec());
    let mut from_browser = FramedRead::new(browser_in, frame_codec());
    let mut to_browser = FramedWrite::new(browser_out, frame_codec());
    let hello = json!({
        "jsonrpc": "2.0",
        "method": "bridge.hello",
        "params": { "token": token.as_str(), "origin": caller },
    });
    to_runtime
        .send(bytes::Bytes::from(hello.to_string()))
        .await?;
    loop {
        tokio::select! {
            frame = from_browser.next() => match frame {
                Some(Ok(bytes)) => to_runtime.send(bytes.freeze()).await?,
                // Closed, or a frame over 1 MiB: the connection ends.
                _ => return Ok(()),
            },
            frame = from_runtime.next() => match frame {
                Some(Ok(bytes)) => to_browser.send(bytes.freeze()).await?,
                _ => return Ok(()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token() -> SessionToken {
        SessionToken::generate().unwrap()
    }

    /// The extension's side of a connection, in memory: says hello, then answers requests.
    async fn fake_extension(
        stream: tokio::io::DuplexStream,
        hello_token: String,
        answers: fn(&str, &Value) -> Result<Value, String>,
    ) {
        let (peer, mut incoming) = Peer::spawn(stream);
        peer.notify(
            "bridge.hello",
            json!({ "token": hello_token, "origin": origin() }),
        )
        .unwrap();
        while let Some(m) = incoming.recv().await {
            if let Incoming::Request(r) = m {
                let result = answers(&r.method, &r.params).map_err(|e| kivo_ipc::RpcError {
                    code: -32000,
                    message: e,
                });
                let _ = peer.respond(r.id, result);
            }
        }
    }

    fn answers(method: &str, params: &Value) -> Result<Value, String> {
        match method {
            "tabs" => Ok(
                json!([{ "id": 3, "title": "Flights", "url": "https://flights.example/", "active": true }]),
            ),
            "read" => Ok(
                json!({ "url": "https://flights.example/", "title": "Flights", "text": "Deals", "fields": [], "links": [], "truncated": false }),
            ),
            "type" if params["selector"] == "#pw" => Err("password".into()),
            "type" => Ok(json!({ "typed": true })),
            "click" => Ok(json!({ "clicked": "Search" })),
            _ => Err("unknownMethod".into()),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_extension_answers_through_the_bridge() {
        let bridge = BrowserBridge::disconnected();
        let t = token();
        let (ours, theirs) = tokio::io::duplex(64 * 1024);
        let b = Arc::clone(&bridge);
        let t2 = t.clone();
        tokio::spawn(async move { b.serve(ours, &t2).await });
        tokio::spawn(fake_extension(theirs, t.as_str().to_owned(), answers));
        for _ in 0..200 {
            if bridge.connected() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(bridge.connected());
        let b = Arc::clone(&bridge);
        let (tabs, page, clicked, pw) = tokio::task::spawn_blocking(move || {
            (
                b.tabs().unwrap(),
                b.read(None).unwrap(),
                b.click(
                    None,
                    &PageTarget {
                        selector: None,
                        text: Some("Search".into()),
                    },
                )
                .unwrap(),
                b.type_text(
                    None,
                    &PageTarget {
                        selector: Some("#pw".into()),
                        text: None,
                    },
                    "x",
                    false,
                ),
            )
        })
        .await
        .unwrap();
        assert_eq!(tabs[0].id, 3);
        assert_eq!(page.text, "Deals");
        assert_eq!(clicked, "Search");
        assert_eq!(pw.unwrap_err().code, ToolErrorCode::AccessDenied);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_connection_without_the_token_is_refused() {
        let bridge = BrowserBridge::disconnected();
        let (ours, theirs) = tokio::io::duplex(64 * 1024);
        let b = Arc::clone(&bridge);
        let t = token();
        let served = tokio::spawn(async move { b.serve(ours, &t).await });
        tokio::spawn(fake_extension(theirs, "wrong".into(), answers));
        served.await.unwrap();
        assert!(!bridge.connected());
    }

    /// The whole chain: extension ⇄ (stdin/stdout) host relay ⇄ (pipe) runtime bridge.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_host_relays_between_the_browser_and_the_runtime() {
        let bridge = BrowserBridge::disconnected();
        let t = token();
        // runtime end of the pipe ⇄ host end of the pipe
        let (runtime_end, host_pipe) = tokio::io::duplex(64 * 1024);
        // extension ⇄ host stdio
        let (extension_end, host_stdio) = tokio::io::duplex(64 * 1024);
        let (host_in, host_out) = tokio::io::split(host_stdio);
        let b = Arc::clone(&bridge);
        let t2 = t.clone();
        tokio::spawn(async move { b.serve(runtime_end, &t2).await });
        let t3 = t.clone();
        tokio::spawn(async move { relay(host_in, host_out, host_pipe, &t3, &origin()).await });
        // The extension here doesn't say hello itself: the host does, with the token.
        let (ext, mut incoming) = Peer::spawn(extension_end);
        tokio::spawn(async move {
            while let Some(m) = incoming.recv().await {
                if let Incoming::Request(r) = m {
                    let _ = ext.respond(
                        r.id,
                        answers(&r.method, &r.params).map_err(|e| kivo_ipc::RpcError {
                            code: -32000,
                            message: e,
                        }),
                    );
                }
            }
        });
        for _ in 0..200 {
            if bridge.connected() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let b = Arc::clone(&bridge);
        let tabs = tokio::task::spawn_blocking(move || b.tabs())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(tabs[0].url, "https://flights.example/");
    }

    #[test]
    fn only_kivos_extension_is_named_in_the_manifest() {
        assert_eq!(
            origin(),
            "chrome-extension://ohaefaipbiknncgdjdpkjgaiocpnlloc/"
        );
        assert!(
            manifest_path(Path::new(r"C:\x")).ends_with(r"native-messaging\com.kivo.bridge.json")
        );
    }
}
