//! M6, end to end through the real runtime pieces on fakes: other apps' MCP setups imported
//! without being changed (DISC-09, M6-X2); a server's tools waiting for review, then used by a
//! brain through the permission engine (TOOL-34/35); a description that changes after approval
//! switched off (TOOL-36, TOOL-41); KIVO's own MCP server sharing memory only with allowed agents
//! (TOOL-37, CONV-24); a connector signed in in "the browser" (INT-04) and local ones found
//! (DISC-08); skills found, reviewed and put into brain requests (CONV-32, DISC-10).

#![cfg(windows)]

use kivo_brain::PrivacyClass;
use kivo_brain::testing::{Script, ScriptedBrain};
use kivo_core::{Capability, SessionState};
use kivo_ipc::protocol::{ConnectorView, McpFoundView, McpServerView, SkillView};
use kivo_platform::{SecretHandle, Secrets};
use kivo_runtime::scripted::{self, Rig};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn worker() -> PathBuf {
    let mut dir = std::env::current_exe().expect("test binary");
    dir.pop();
    dir.pop();
    dir.join("kivo-infer.exe")
}

fn fixture() -> PathBuf {
    let mut dir = std::env::current_exe().expect("test binary");
    dir.pop();
    dir.pop();
    let exe = dir.join("kivo-test-mcp.exe");
    assert!(
        exe.is_file(),
        "{} is missing: `cargo test --workspace` builds it",
        exe.display()
    );
    exe
}

static ONE_AT_A_TIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn until(what: &str, timeout: Duration, mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("timed out waiting for {what}");
}

struct Running {
    rig: Rig,
    worker: tokio::task::JoinHandle<()>,
    pump: tokio::task::JoinHandle<()>,
}

impl Running {
    async fn stop(self) {
        for s in self.rig.mcp.servers() {
            self.rig.mcp.remove(&s.id).await;
        }
        self.rig.core.quit();
        let _ = tokio::time::timeout(Duration::from_secs(5), self.worker).await;
        self.pump.abort();
    }

    async fn rpc(&self, method: &str, params: Value) -> Result<Value, String> {
        self.rig
            .extensions
            .call(method, params)
            .await
            .expect("an M6 request")
            .map_err(|e| e.message)
    }
}

fn start() -> Running {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(std::env::var("KIVO_TEST_LOG").unwrap_or_else(|_| "warn".into()))
        .with_test_writer()
        .try_init();
    let (rig, worker, pump) = scripted::rig(worker(), Vec::new(), PathBuf::from("no-model"));
    rig.core.update_config(|c| {
        c.capabilities.set(Capability::SpeakResponses, false);
        c.capabilities.set(Capability::McpServers, true);
        c.capabilities.set(Capability::Integrations, true);
        c.voice.speak_typed_replies = false;
    });
    Running { rig, worker, pump }
}

async fn answered(rig: &Rig) -> kivo_ipc::protocol::TurnView {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        let done = rig.core.state().borrow().session == SessionState::Idle
            && rig
                .core
                .turn_view()
                .is_some_and(|t| t.answer.is_some() || t.error.is_some());
        if done {
            return rig.core.turn_view().unwrap();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!(
        "the turn didn't end: {:?} {:#?}",
        rig.core.state().borrow().session,
        rig.core.turn_view()
    );
}

fn brain(script: Vec<Script>) -> Arc<ScriptedBrain> {
    let mut b = ScriptedBrain::new("openai", PrivacyClass::Cloud, script);
    b.info.name = "OpenAI".into();
    Arc::new(b)
}

fn claude_desktop_config(r: &Running, hostile: bool) -> PathBuf {
    let dir = r.rig.appdata.join("Claude");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("claude_desktop_config.json");
    let args = if hostile {
        json!(["--hostile"])
    } else {
        json!([])
    };
    std::fs::write(
        &file,
        json!({ "mcpServers": { "Test Tools": {
            "command": fixture(), "args": args, "env": { "TEST_API_KEY": "sk-not-real-42", "MODE": "demo" }
        } } })
        .to_string(),
    )
    .unwrap();
    file
}

async fn view(r: &Running, id: &str) -> McpServerView {
    let list: Vec<McpServerView> =
        serde_json::from_value(r.rpc("mcp.list", json!({})).await.unwrap()).unwrap();
    list.into_iter().find(|s| s.id == id).expect("the server")
}

/// DISC-09, M6-X2, TOOL-34/35: Claude Desktop's server is found and imported by copying — the
/// file is untouched, the key goes to Credential Manager — and its tools wait for review. Once
/// approved, a brain uses one through the permission engine, and its result is untrusted.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_imported_server_is_reviewed_then_used_by_a_brain() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start();
    let file = claude_desktop_config(&r, false);
    let before = std::fs::read(&file).unwrap();

    let found: Vec<McpFoundView> =
        serde_json::from_value(r.rpc("mcp.found", json!({})).await.unwrap()).unwrap();
    let desktop = found
        .iter()
        .find(|f| f.app == "claude-desktop")
        .expect("found");
    assert_eq!(desktop.new, ["Test Tools"]);
    r.rpc("mcp.import", json!({ "file": desktop.file }))
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(&file).unwrap(),
        before,
        "the other app's file is never changed"
    );
    // The key went to Credential Manager; the copied setup only names it.
    let key = r.rig.secrets.get(&SecretHandle {
        provider: "kivo-mcp".into(),
        name: "mcp.test-tools.TEST_API_KEY".into(),
    });
    assert_eq!(
        key.unwrap().map(|s| s.expose().to_owned()).as_deref(),
        Some("sk-not-real-42")
    );
    let body = r
        .rig
        .db
        .lock()
        .unwrap()
        .mcp_server("test-tools")
        .unwrap()
        .unwrap();
    assert!(!body.contains("sk-not-real-42"), "{body}");
    // Imported again: nothing new.
    let found: Vec<McpFoundView> =
        serde_json::from_value(r.rpc("mcp.found", json!({})).await.unwrap()).unwrap();
    assert!(found.iter().all(|f| f.new.is_empty()));

    // Its tools wait for review: none of them is a KIVO tool yet.
    until("the server to connect", Duration::from_secs(20), || {
        r.rig
            .mcp
            .view("test-tools")
            .is_some_and(|v| v.status == "needsReview")
    })
    .await;
    let v = view(&r, "test-tools").await;
    assert!(v.tools.iter().all(|t| t.state == "new" && !t.enabled));
    let caps = r.rig.core.config().capabilities;
    assert!(r.rig.registry.get("mcp.test-tools.add", &caps).is_none());

    // Approve `add` only.
    r.rpc("mcp.approve", json!({ "id": "test-tools", "on": ["add"] }))
        .await
        .unwrap();
    let v = view(&r, "test-tools").await;
    assert_eq!(v.status, "running");
    assert!(r.rig.registry.get("mcp.test-tools.add", &caps).is_some());
    assert!(
        r.rig.registry.get("mcp.test-tools.echo", &caps).is_none(),
        "reviewed but left off"
    );

    // A brain uses it: Medium in Auto, so it runs; the result reaches the brain as untrusted.
    let b = brain(vec![
        Script::tool("mcp__test-tools__add", json!({ "a": 19, "b": 23 })),
        Script::text("It's 42."),
    ]);
    r.rig.brains.insert(b.clone());
    r.rig
        .engine
        .say("please add two numbers, 19 and 23")
        .await
        .unwrap();
    // An AI-suggested Medium action asks first; the card names the server, in KIVO's words.
    until("the question", Duration::from_secs(20), || {
        r.rig.core.turn_view().and_then(|t| t.confirm).is_some()
    })
    .await;
    let confirm = r.rig.core.turn_view().unwrap().confirm.unwrap();
    assert_eq!(confirm.tool, "mcp.test-tools.add");
    assert_eq!(confirm.action, "Use add (Test Tools)");
    r.rig
        .engine
        .answer_confirmation(&confirm.call_id, true, false)
        .await
        .unwrap();
    let turn = answered(&r.rig).await;
    assert_eq!(turn.answer.as_deref(), Some("It's 42."));
    let second = format!("{:?}", b.requests.lock().unwrap()[1].messages);
    assert!(second.contains("42"), "{second}");
    assert!(
        second.contains("untrusted"),
        "a server's result is fenced: {second}"
    );
    assert!(
        r.rig
            .recorder
            .audit_rows(10)
            .iter()
            .any(|a| a.tool == "mcp.test-tools.add"),
        "audited like any tool"
    );
    // The user lowers its risk, then switches it off.
    r.rpc(
        "mcp.setTool",
        json!({ "id": "test-tools", "tool": "add", "risk": "low" }),
    )
    .await
    .unwrap();
    assert_eq!(
        view(&r, "test-tools")
            .await
            .tools
            .iter()
            .find(|t| t.name == "add")
            .unwrap()
            .risk,
        kivo_core::tool::Risk::Low
    );
    r.rpc(
        "mcp.setTool",
        json!({ "id": "test-tools", "tool": "add", "enabled": false }),
    )
    .await
    .unwrap();
    assert!(r.rig.registry.get("mcp.test-tools.add", &caps).is_none());
    // Removed: its secret goes too.
    r.rpc("mcp.remove", json!({ "id": "test-tools" }))
        .await
        .unwrap();
    assert!(
        r.rig
            .secrets
            .get(&SecretHandle {
                provider: "kivo-mcp".into(),
                name: "mcp.test-tools.TEST_API_KEY".into(),
            })
            .unwrap()
            .is_none()
    );
    r.stop().await;
}

/// TOOL-36, TOOL-41: a tool whose description changes after it was approved is switched off at
/// once and waits for review; the hostile server's other tricks stay contained.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_description_changed_after_approval_is_switched_off() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start();
    r.rpc(
        "mcp.add",
        json!({ "name": "evil", "command": fixture(), "args": ["--hostile"] }),
    )
    .await
    .unwrap();
    until("the server to connect", Duration::from_secs(20), || {
        r.rig
            .mcp
            .view("evil")
            .is_some_and(|v| v.status == "needsReview")
    })
    .await;
    let v = view(&r, "evil").await;
    let names: Vec<&str> = v.tools.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"shell.run"));
    r.rpc(
        "mcp.approve",
        json!({ "id": "evil", "on": ["rug_pull", "shell.run"] }),
    )
    .await
    .unwrap();
    let caps = r.rig.core.config().capabilities;
    // Its "shell.run" is its own namespaced tool, never KIVO's shell.
    assert!(r.rig.registry.get("mcp.evil.shell_run", &caps).is_some());
    assert_eq!(
        r.rig.registry.known("shell.run").unwrap().capability,
        Capability::Shell
    );
    let rug = r
        .rig
        .registry
        .get("mcp.evil.rug_pull", &caps)
        .expect("approved");
    tokio::task::spawn_blocking(move || rug.run(&json!({})))
        .await
        .unwrap()
        .unwrap();
    // It announced a changed description: the tool is off until it's reviewed again.
    until("the change to be noticed", Duration::from_secs(10), || {
        r.rig.registry.get("mcp.evil.rug_pull", &caps).is_none()
    })
    .await;
    let v = view(&r, "evil").await;
    assert_eq!(v.status, "needsReview");
    let pulled = v.tools.iter().find(|t| t.name == "rug_pull").unwrap();
    assert_eq!(pulled.state, "changed");
    assert!(pulled.description.contains("attacker"), "shown for review");
    assert!(
        r.rig.registry.get("mcp.evil.shell_run", &caps).is_some(),
        "unchanged tools stay on"
    );
    r.stop().await;
}

/// TOOL-37, CONV-24: KIVO's MCP server shares memory only with the agents the user allowed,
/// leaves sensitive memories out unless allowed, and a change it asks for needs the user's OK.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn kivos_mcp_server_shares_memory_only_with_allowed_agents() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start();
    {
        let db = r.rig.db.lock().unwrap();
        db.set_preference("rust.testing", "KIVO runs cargo nextest in CI.", "user")
            .unwrap();
        db.set_preference("personal.address", "12 Park Street, cargo town", "user")
            .unwrap();
    }
    let list = r
        .rpc("mcp.shared.list", json!({ "agent": "claude-code" }))
        .await
        .unwrap();
    assert_eq!(list, json!([]), "nothing until the user allows the agent");
    let refused = r
        .rpc("mcp.shared.call", json!({ "agent": "claude-code", "name": "memory_search", "args": { "query": "cargo" } }))
        .await
        .unwrap();
    assert_eq!(refused["isError"], true);

    r.rpc("mcp.share", json!({ "agent": "claude-code", "on": true }))
        .await
        .unwrap();
    let list = r
        .rpc("mcp.shared.list", json!({ "agent": "claude-code" }))
        .await
        .unwrap();
    let names: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert!(
        names.contains(&"memory_search") && names.contains(&"memory_add"),
        "{names:?}"
    );

    // Over MCP itself: KIVO's server on one end, an MCP client (the "agent") on the other.
    struct InProcess(Arc<kivo_runtime::extensions_rpc::ExtensionsRpc>);
    impl kivo_mcp::server::Backend for InProcess {
        fn list(
            &self,
        ) -> kivo_mcp::server::BoxFuture<Result<Vec<kivo_mcp::server::SharedTool>, String>>
        {
            let x = Arc::clone(&self.0);
            Box::pin(async move {
                let v = x
                    .call("mcp.shared.list", json!({ "agent": "claude-code" }))
                    .await
                    .unwrap()
                    .map_err(|e| e.message)?;
                serde_json::from_value(v).map_err(|e| e.to_string())
            })
        }
        fn call(
            &self,
            name: String,
            args: Value,
        ) -> kivo_mcp::server::BoxFuture<Result<kivo_mcp::server::SharedResult, String>> {
            let x = Arc::clone(&self.0);
            Box::pin(async move {
                let v = x
                    .call(
                        "mcp.shared.call",
                        json!({ "agent": "claude-code", "name": name, "args": args }),
                    )
                    .await
                    .unwrap()
                    .map_err(|e| e.message)?;
                serde_json::from_value(v).map_err(|e| e.to_string())
            })
        }
    }
    use rmcp::ServiceExt;
    let (a, b) = tokio::io::duplex(64 * 1024);
    let server =
        kivo_mcp::server::KivoServer::new(Arc::new(InProcess(Arc::clone(&r.rig.extensions))));
    tokio::spawn(async move {
        if let Ok(s) = server.serve(a).await {
            let _ = s.waiting().await;
        }
    });
    let agent = ().serve(b).await.expect("the agent connects");
    let tools = agent.peer().list_all_tools().await.unwrap();
    assert!(tools.iter().any(|t| t.name == "memory_search"));
    let mut params = rmcp::model::CallToolRequestParams::new("memory_search");
    params.arguments = json!({ "query": "cargo" }).as_object().cloned();
    let result = agent.peer().call_tool(params).await.unwrap();
    let text = format!("{:?}", result.content);
    assert!(text.contains("nextest"), "{text}");
    assert!(
        !text.contains("Park Street"),
        "sensitive memories stay out: {text}"
    );
    // Adding a memory is a change: with no KIVO turn to show the question in, it's refused, not
    // done unasked.
    let mut params = rmcp::model::CallToolRequestParams::new("memory_add");
    params.arguments = json!({ "text": "The user likes tabs." })
        .as_object()
        .cloned();
    let result = agent.peer().call_tool(params).await.unwrap();
    assert_eq!(result.is_error, Some(true));
    assert!(format!("{:?}", result.content).contains("needs your OK"));
    // Allowed sensitive memories too.
    r.rpc(
        "mcp.share",
        json!({ "agent": "claude-code", "on": true, "sensitive": true }),
    )
    .await
    .unwrap();
    let all = r
        .rpc(
            "mcp.shared.call",
            json!({ "agent": "claude-code", "name": "memory_search", "args": { "query": "park" } }),
        )
        .await
        .unwrap();
    assert!(all["text"].as_str().unwrap().contains("Park Street"));
    // Agents KIVO runs get the server in their session (BRAIN-16).
    let spec = kivo_runtime::agents::kivo_server(
        "claude-code",
        std::path::Path::new("C:\\KIVO\\kivo-runtime.exe"),
    );
    assert_eq!(spec.args, ["--mcp-server", "--agent", "claude-code"]);
    r.stop().await;
}

/// INT-02/03/04, DISC-08: a custom connector (a remote MCP URL) signs in in "the browser" and its
/// tools wait for review; the GitHub CLI signed in is found as a ready local connector, which the
/// user can switch off.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn connectors_sign_in_and_local_ones_are_found() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start();
    let mut child = std::process::Command::new(fixture())
        .arg("--http")
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let port = line.trim().strip_prefix("listening ").unwrap().to_owned();
    let url = format!("http://127.0.0.1:{port}/mcp");

    let connected = r
        .rpc(
            "connectors.connect",
            json!({ "url": url, "name": "Team tracker" }),
        )
        .await
        .unwrap();
    let c: ConnectorView = serde_json::from_value(connected).unwrap();
    assert_eq!(c.state, "connected");
    let page = r.rig.opened.lock().unwrap()[0].clone();
    assert!(
        page.contains("code_challenge="),
        "signed in with PKCE: {page}"
    );
    let server = r.rig.mcp.server(c.server.as_deref().unwrap()).unwrap();
    let kivo_mcp::config::Transport::Http { oauth, token, .. } = &server.transport else {
        panic!("remote")
    };
    assert!(
        oauth.is_some() && token.is_none(),
        "tokens in Credential Manager, not in the setup"
    );
    until(
        "its tools to wait for review",
        Duration::from_secs(20),
        || {
            r.rig
                .mcp
                .view(&server.id)
                .is_some_and(|v| v.status == "needsReview" && !v.tools.is_empty())
        },
    )
    .await;
    assert!(server.remote());

    // Local: the GitHub CLI, signed in.
    r.rig.commands.reply.lock().unwrap().stdout =
        "github.com\n  ✓ Logged in to github.com account MadBlast0 (keyring)".into();
    r.rpc("extensions.refresh", json!({ "section": "connectors" }))
        .await
        .unwrap();
    let list: Vec<ConnectorView> =
        serde_json::from_value(r.rpc("connectors.list", json!({})).await.unwrap()).unwrap();
    let gh = list.iter().find(|c| c.id == "github-cli").unwrap();
    assert_eq!(gh.state, "ready");
    assert!(
        gh.detail
            .as_deref()
            .unwrap_or_default()
            .contains("MadBlast0")
    );
    assert!(
        list.iter()
            .any(|c| c.id == "notion" && c.state == "available" && c.kind == "remote")
    );
    // Switched off: KIVO won't use GitHub's CLI.
    r.rpc("connectors.disconnect", json!({ "id": "github-cli" }))
        .await
        .unwrap();
    let config = r.rig.core.config();
    assert_eq!(
        kivo_runtime::connectors::switched_off(&config, "github-cli").as_deref(),
        Some("GitHub via GitHub CLI")
    );
    r.rpc("connectors.connect", json!({ "id": "github-cli" }))
        .await
        .unwrap();
    assert!(kivo_runtime::connectors::switched_off(&r.rig.core.config(), "github-cli").is_none());
    // Disconnecting the remote one forgets it.
    r.rpc(
        "connectors.disconnect",
        json!({ "id": c.server.as_deref().unwrap() }),
    )
    .await
    .unwrap();
    assert!(r.rig.mcp.server(&server.id).is_none());
    let _ = child.kill();
    let _ = child.wait();
    r.stop().await;
}

/// CONV-32, DISC-10: a skill in Claude Code's folder is found and waits for review; once on, its
/// name and description are in brain requests, and the brain loads its body with `skills.load`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn skills_are_found_reviewed_and_offered_to_brains() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start();
    let folder = r
        .rig
        .home
        .join(".claude")
        .join("skills")
        .join("release-notes");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("SKILL.md"),
        "---\nname: release-notes\ndescription: Turns commits into release notes.\n---\nGroup commits by type.",
    )
    .unwrap();
    r.rpc("extensions.refresh", json!({ "section": "skills" }))
        .await
        .unwrap();
    let list: Vec<SkillView> =
        serde_json::from_value(r.rpc("skills.list", json!({})).await.unwrap()).unwrap();
    let skill = list.iter().find(|s| s.name == "release-notes").unwrap();
    assert!(!skill.enabled && !skill.reviewed);
    let review = r
        .rpc("skills.read", json!({ "id": skill.id }))
        .await
        .unwrap();
    assert!(review["text"].as_str().unwrap().contains("Group commits"));
    r.rpc("skills.enable", json!({ "id": skill.id, "on": true }))
        .await
        .unwrap();

    let b = brain(vec![
        Script::tool("skills__load", json!({ "name": "release-notes" })),
        Script::text("Here are the notes."),
    ]);
    r.rig.brains.insert(b.clone());
    r.rig
        .engine
        .say("write release notes for the last release")
        .await
        .unwrap();
    let turn = answered(&r.rig).await;
    assert_eq!(turn.answer.as_deref(), Some("Here are the notes."));
    let requests = b.requests.lock().unwrap().clone();
    let first = format!("{:?}", requests[0]);
    assert!(
        first.contains("release-notes: Turns commits into release notes."),
        "the index is in the request"
    );
    assert!(
        !first.contains("Group commits"),
        "the body isn't, until it's loaded"
    );
    assert!(format!("{:?}", requests[1].messages).contains("Group commits"));
    r.stop().await;
}

/// DISC-16: a setup another app writes later, and a skill dropped into a watched folder, are
/// noticed without a Refresh.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn other_apps_setups_and_skills_are_watched() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start();
    let claude = r.rig.appdata.join("Claude");
    let skills = r.rig.home.join(".claude").join("skills");
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::create_dir_all(&skills).unwrap();
    let mut events = r.rig.core.bus.subscribe();
    r.rig.mcp.watch();
    r.rig.skills.watch();

    // Claude Desktop's config appears after KIVO started watching.
    std::fs::write(
        claude.join("claude_desktop_config.json"),
        r#"{ "mcpServers": { "later": { "command": "npx", "args": ["later-mcp"] } } }"#,
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut saw_mcp = false;
    while !saw_mcp && Instant::now() < deadline {
        if let Ok(kivo_core::Received::Event(e)) =
            tokio::time::timeout(Duration::from_millis(200), events.recv()).await
        {
            saw_mcp = matches!(
                e.kind,
                kivo_core::EventKind::System(kivo_core::event::SystemEvent::DiscoveryChanged { ref section })
                    if section == "mcp"
            );
        }
    }
    assert!(saw_mcp, "the new config was noticed");
    let found: Vec<McpFoundView> =
        serde_json::from_value(r.rpc("mcp.found", json!({})).await.unwrap()).unwrap();
    assert!(found.iter().any(|f| f.new.contains(&"later".to_owned())));

    // A skill folder dropped in shows up in the list by itself.
    let folder = skills.join("changelog");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("SKILL.md"),
        "---\nname: changelog\ndescription: Keeps CHANGELOG.md.\n---\nAdd a line per change.",
    )
    .unwrap();
    let skills_now = || {
        r.rig
            .skills
            .list()
            .iter()
            .any(|s| s.name == "changelog" && !s.enabled)
    };
    until("the dropped skill", Duration::from_secs(10), skills_now).await;
    r.stop().await;
}
