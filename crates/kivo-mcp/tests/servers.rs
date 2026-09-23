//! The MCP client against the test ground's server (`testenv/mcp`), well-behaved and hostile
//! (TOOL-34/35/36, TOOL-41): tools become namespaced KIVO tools at Medium risk; a server can't take
//! a KIVO tool's name, flood the context, hang a call, give orders through its results or change a
//! description unnoticed.

use kivo_core::Capability;
use kivo_core::tool::Risk;
use kivo_mcp::config::{McpServer, Transport};
use kivo_mcp::{Connection, McpError, McpTool, spec_for};
use kivo_tools::Tool;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

fn fixture() -> PathBuf {
    // target/<profile>/deps/<test>.exe → target/<profile>/kivo-test-mcp.exe
    let mut dir = std::env::current_exe().expect("test binary");
    dir.pop();
    dir.pop();
    let exe = dir.join(format!("kivo-test-mcp{}", std::env::consts::EXE_SUFFIX));
    assert!(
        exe.is_file(),
        "{} is missing: `cargo test --workspace` builds it",
        exe.display()
    );
    exe
}

fn server(id: &str, hostile: bool) -> McpServer {
    McpServer {
        id: id.into(),
        name: id.into(),
        transport: Transport::Stdio {
            command: fixture().display().to_string(),
            args: if hostile {
                vec!["--hostile".into()]
            } else {
                Vec::new()
            },
            env: BTreeMap::new(),
            cwd: None,
        },
        enabled: true,
        source: None,
        tools: BTreeMap::new(),
    }
}

/// No secrets, no sign-ins.
struct Empty;

struct NoTokens;

impl kivo_mcp::auth::TokenStore for NoTokens {
    fn load(&self) -> Option<String> {
        None
    }
    fn save(&self, _json: &str) -> Result<(), String> {
        Ok(())
    }
    fn clear(&self) {}
}

impl kivo_mcp::client::Vault for Empty {
    fn secret(&self, _name: &str) -> Option<String> {
        None
    }
    fn tokens(&self, _name: &str) -> Arc<dyn kivo_mcp::auth::TokenStore> {
        Arc::new(NoTokens)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_well_behaved_server_becomes_kivo_tools() {
    let s = server("calc", false);
    let conn = Connection::connect(&s, &Empty, &CancellationToken::new())
        .await
        .unwrap();
    let tools = conn.tools().await.unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["echo", "add"]);
    let add = tools.iter().find(|t| t.name == "add").unwrap();
    let spec = spec_for(&s, add, None, Capability::McpServers);
    assert_eq!(spec.id, "mcp.calc.add");
    assert_eq!(spec.risk, Risk::Medium, "Medium unless the user lowers it");
    assert!(!spec.data_egress, "a program on this PC");
    assert!(
        spec.description.contains("MCP server"),
        "{}",
        spec.description
    );
    // Called through the KIVO tool, off the async threads like the executor does.
    let tool = Arc::new(McpTool::new(
        spec,
        "add".into(),
        "calc".into(),
        Arc::clone(&conn),
        tokio::runtime::Handle::current(),
    ));
    let out = tokio::task::spawn_blocking(move || tool.run(&json!({ "a": 2, "b": 3 })))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(out.data["text"], "5");
    assert!(
        out.source.is_some(),
        "a server's result is untrusted content"
    );
    conn.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hostile_server_is_contained() {
    let s = server("evil", true);
    let conn = Connection::connect(&s, &Empty, &CancellationToken::new())
        .await
        .unwrap();
    let tools = conn.tools().await.unwrap();
    let spec = |name: &str| {
        let t = tools.iter().find(|t| t.name == name).expect(name);
        spec_for(&s, t, None, Capability::McpServers)
    };

    // It can't take a KIVO tool's name.
    assert_eq!(spec("shell.run").id, "mcp.evil.shell_run");
    // Its description is labelled as the server's words, and cut.
    let poisoned = spec("poisoned");
    assert!(poisoned.description.starts_with("[From the MCP server"));
    assert!(poisoned.description.chars().count() <= 1_200);
    // The card's title is KIVO's, never the server's text.
    assert_eq!(poisoned.title, "Use poisoned (evil)");
    // A schema that isn't an object takes no arguments.
    assert_eq!(spec("bad_schema").params["type"], "object");

    let never = CancellationToken::new();
    // A flood is cut.
    let flood = conn
        .call("flood", &json!({}), Duration::from_secs(20), &never)
        .await
        .unwrap();
    assert!(flood.truncated);
    assert_eq!(
        flood.text.chars().count(),
        kivo_mcp::client::MAX_RESULT_CHARS
    );
    // A call that never ends times out…
    let slow = conn
        .call("slow", &json!({}), Duration::from_millis(300), &never)
        .await;
    assert!(matches!(slow, Err(McpError::Timeout)), "{slow:?}");
    // …and can be cancelled.
    let cancel = CancellationToken::new();
    let c = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        c.cancel();
    });
    let stopped = conn
        .call("slow", &json!({}), Duration::from_secs(60), &cancel)
        .await;
    assert!(matches!(stopped, Err(McpError::Cancelled)), "{stopped:?}");
    // A failing call is a failure; its words stay out of what KIVO says.
    let tool = Arc::new(McpTool::new(
        spec("fail"),
        "fail".into(),
        "evil".into(),
        Arc::clone(&conn),
        tokio::runtime::Handle::current(),
    ));
    let e = tokio::task::spawn_blocking(move || tool.run(&json!({})))
        .await
        .unwrap()
        .unwrap_err();
    assert!(!e.message.contains("password"), "{}", e.message);
    // Orders in a result are data from an untrusted source.
    let tool = Arc::new(McpTool::new(
        spec("inject"),
        "inject".into(),
        "evil".into(),
        Arc::clone(&conn),
        tokio::runtime::Handle::current(),
    ));
    let out = tokio::task::spawn_blocking(move || tool.run(&json!({})))
        .await
        .unwrap()
        .unwrap();
    assert!(out.source.as_deref().is_some_and(|s| s.contains("evil")));

    // A description changed after approval is noticed: the server announces it and the
    // fingerprint no longer matches.
    let before = tools
        .iter()
        .find(|t| t.name == "rug_pull")
        .unwrap()
        .hash
        .clone();
    let mut changed = conn.tools_changed();
    let seen = *changed.borrow_and_update();
    conn.call("rug_pull", &json!({}), Duration::from_secs(10), &never)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), changed.changed())
        .await
        .expect("tools/list_changed arrives")
        .unwrap();
    assert!(*changed.borrow() > seen);
    let after = conn.tools().await.unwrap();
    let pulled = after.iter().find(|t| t.name == "rug_pull").unwrap();
    assert_ne!(pulled.hash, before);
    assert!(pulled.description.contains("attacker"));
    conn.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_missing_program_or_secret_is_a_plain_error() {
    let mut s = server("gone", false);
    s.transport = Transport::Stdio {
        command: "C:\\nowhere\\no-such-server.exe".into(),
        args: Vec::new(),
        env: BTreeMap::new(),
        cwd: None,
    };
    assert!(matches!(
        Connection::connect(&s, &Empty, &CancellationToken::new()).await,
        Err(McpError::Connect(_))
    ));
    let mut s = server("keyed", false);
    if let Transport::Stdio { env, .. } = &mut s.transport {
        env.insert(
            "API_KEY".into(),
            kivo_mcp::EnvValue::Secret("mcp.keyed.API_KEY".into()),
        );
    }
    assert!(matches!(
        Connection::connect(&s, &Empty, &CancellationToken::new()).await,
        Err(McpError::MissingSecret(name)) if name == "mcp.keyed.API_KEY"
    ));
}
