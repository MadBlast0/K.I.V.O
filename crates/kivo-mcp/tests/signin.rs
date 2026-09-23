//! Signing in to a remote MCP server, for real, against the test ground's server in its HTTP mode
//! (`testenv/mcp --http`, with its own OAuth server): discovery, KIVO registering itself, PKCE,
//! the browser's redirect to KIVO's loopback, the token exchange, and then MCP with the token
//! (INT-04). The "browser" is the test following the authorize redirect.

use kivo_mcp::auth::TokenStore;
use kivo_mcp::client::Vault;
use kivo_mcp::config::{McpServer, Transport};
use kivo_mcp::{Connection, McpError};
use serde_json::json;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

fn fixture() -> PathBuf {
    let mut dir = std::env::current_exe().expect("test binary");
    dir.pop();
    dir.pop();
    dir.join(format!("kivo-test-mcp{}", std::env::consts::EXE_SUFFIX))
}

/// The fixture over HTTP; killed when dropped.
struct Http {
    child: Child,
    base: String,
}

impl Drop for Http {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start(no_auth: bool) -> Http {
    let mut cmd = Command::new(fixture());
    cmd.arg("--http")
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if no_auth {
        cmd.arg("--no-auth");
    }
    let mut child = cmd.spawn().expect("the test MCP server starts");
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let port = line.trim().strip_prefix("listening ").expect("its port");
    Http {
        child,
        base: format!("http://127.0.0.1:{port}"),
    }
}

#[derive(Default)]
struct Memory(Mutex<Option<String>>);

impl TokenStore for Memory {
    fn load(&self) -> Option<String> {
        self.0.lock().unwrap().clone()
    }
    fn save(&self, json: &str) -> Result<(), String> {
        *self.0.lock().unwrap() = Some(json.to_owned());
        Ok(())
    }
    fn clear(&self) {
        *self.0.lock().unwrap() = None;
    }
}

struct Tokens(Arc<Memory>);

impl Vault for Tokens {
    fn secret(&self, _name: &str) -> Option<String> {
        None
    }
    fn tokens(&self, _name: &str) -> Arc<dyn TokenStore> {
        Arc::clone(&self.0) as Arc<dyn TokenStore>
    }
}

fn server(url: String, oauth: Option<&str>) -> McpServer {
    McpServer {
        id: "remote".into(),
        name: "remote".into(),
        transport: Transport::Http {
            url,
            token: None,
            oauth: oauth.map(str::to_owned),
        },
        enabled: true,
        source: Some("connector:custom".into()),
        tools: BTreeMap::new(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn signing_in_in_the_browser_then_using_the_tools() {
    let http = start(false);
    let url = format!("{}/mcp", http.base);
    let store = Arc::new(Memory::default());
    let vault = Tokens(Arc::clone(&store));

    // Without signing in, the server refuses.
    let refused = Connection::connect(
        &server(url.clone(), None),
        &vault,
        &CancellationToken::new(),
    )
    .await;
    assert!(refused.is_err());
    // With a sign-in entry but no tokens yet: sign in first.
    assert!(matches!(
        Connection::connect(
            &server(url.clone(), Some("t")),
            &vault,
            &CancellationToken::new()
        )
        .await,
        Err(McpError::SignIn | McpError::Connect(_))
    ));

    // The "browser": opens the authorization page, which redirects to KIVO's loopback.
    let opened: Arc<Mutex<Vec<String>>> = Arc::default();
    let seen = Arc::clone(&opened);
    let browser = move |page: &str| {
        seen.lock().unwrap().push(page.to_owned());
        let page = page.to_owned();
        tokio::spawn(async move {
            let _ = reqwest::get(page).await;
        });
        Ok(())
    };
    kivo_mcp::auth::sign_in(&url, store.clone(), browser, &CancellationToken::new())
        .await
        .expect("signed in");
    let page = opened.lock().unwrap()[0].clone();
    assert!(page.contains("code_challenge="), "PKCE: {page}");
    assert!(
        page.contains("client_id=kivo-test-client"),
        "registered itself: {page}"
    );
    assert!(
        store
            .load()
            .is_some_and(|t| t.contains("kivo-test-access-token"))
    );

    // Signed in: MCP works with the token.
    let conn = Connection::connect(&server(url, Some("t")), &vault, &CancellationToken::new())
        .await
        .expect("connected with the token");
    let tools = conn.tools().await.unwrap();
    assert!(tools.iter().any(|t| t.name == "add"));
    let sum = conn
        .call(
            "add",
            &json!({ "a": 20, "b": 22 }),
            Duration::from_secs(10),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(sum.text, "42");
    conn.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_server_without_sign_in_connects_directly() {
    let http = start(true);
    let vault = Tokens(Arc::new(Memory::default()));
    let conn = Connection::connect(
        &server(format!("{}/mcp", http.base), None),
        &vault,
        &CancellationToken::new(),
    )
    .await
    .expect("no sign-in needed");
    assert_eq!(conn.tools().await.unwrap().len(), 2);
    conn.close().await;
}
