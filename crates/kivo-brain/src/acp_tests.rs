//! ACP client tests: a fake agent speaking the protocol over in-memory pipes, a real agent
//! process that dies mid-prompt, and (when installed and asked for) the real Gemini CLI's
//! handshake.

use crate::acp::{
    AcpClient, AgentCommand, AgentEvent, PermissionAnswer, PermissionAsk, PermissionHandler, pick,
};
use crate::types::NormalizedError;
use serde_json::{Value, json};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio_util::sync::CancellationToken;

/// Records what the agent asked and answers with a fixed decision.
struct Answers {
    allow: bool,
    asked: Mutex<Vec<PermissionAsk>>,
}

#[async_trait::async_trait]
impl PermissionHandler for Answers {
    async fn ask(&self, ask: PermissionAsk) -> PermissionAnswer {
        let answer = pick(&ask, self.allow, false);
        self.asked.lock().unwrap().push(ask);
        answer
    }
}

/// The `mcpServers` each fake agent's `session/new` was given.
static SEEN_SERVERS: Mutex<Vec<Value>> = Mutex::new(Vec::new());

/// A fake agent: answers `initialize`, `session/new`, `session/load`, `session/set_mode`, and a
/// prompt with a thought, a plan, a tool call that asks permission and edits a file, then a
/// message. A prompt "wait" waits for `session/cancel`.
async fn fake_agent(stream: tokio::io::DuplexStream) {
    let (read, mut write) = tokio::io::split(stream);
    let mut lines = BufReader::new(read).lines();
    let mut next_id = 1000;
    let mut pending_prompt: Option<Value> = None;
    let mut permission_reply: Option<Value> = None;
    while let Ok(Some(line)) = lines.next_line().await {
        let msg: Value = serde_json::from_str(&line).unwrap();
        let mut out: Vec<Value> = Vec::new();
        let id = msg["id"].clone();
        match msg["method"].as_str() {
            Some("initialize") => out.push(json!({ "jsonrpc": "2.0", "id": id, "result": {
                "protocolVersion": 1,
                "agentCapabilities": { "loadSession": true },
                "authMethods": [{ "id": "oauth-personal", "name": "Log in with Google" }],
            }})),
            Some("session/new") => {
                SEEN_SERVERS
                    .lock()
                    .unwrap()
                    .push(msg["params"]["mcpServers"].clone());
                out.push(json!({ "jsonrpc": "2.0", "id": id, "result": {
                "sessionId": "s1",
                "modes": { "currentModeId": "default", "availableModes": [
                    { "id": "default", "name": "Default" }, { "id": "bypassPermissions", "name": "Bypass" }
                ]},
            }}));
            }
            Some("session/load") => {
                out.push(json!({ "jsonrpc": "2.0", "id": id, "result": {} }));
            }
            Some("session/set_mode") => {
                out.push(json!({ "jsonrpc": "2.0", "id": id, "result": {} }))
            }
            Some("session/cancel") => {
                if let Some(prompt_id) = pending_prompt.take() {
                    out.push(json!({ "jsonrpc": "2.0", "id": prompt_id, "result": { "stopReason": "cancelled" } }));
                }
            }
            Some("session/prompt") => {
                let text = msg["params"]["prompt"][0]["text"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let session = msg["params"]["sessionId"].clone();
                let update = |u: Value| json!({ "jsonrpc": "2.0", "method": "session/update", "params": { "sessionId": session, "update": u } });
                if text == "wait" {
                    out.push(update(json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "Working" } })));
                    pending_prompt = Some(id);
                } else {
                    out.push(update(json!({ "sessionUpdate": "agent_thought_chunk", "content": { "type": "text", "text": "secret plan" } })));
                    out.push(update(json!({ "sessionUpdate": "plan", "entries": [{ "content": "Add tests", "priority": "high", "status": "in_progress" }] })));
                    out.push(update(json!({ "sessionUpdate": "tool_call", "toolCallId": "t1", "title": "Edit src/lib.rs", "kind": "edit", "status": "pending" })));
                    next_id += 1;
                    out.push(json!({ "jsonrpc": "2.0", "id": next_id, "method": "session/request_permission", "params": {
                        "sessionId": session,
                        "toolCall": { "toolCallId": "t1", "title": "Edit src/lib.rs", "kind": "edit" },
                        "options": [
                            { "optionId": "yes", "name": "Allow", "kind": "allow_once" },
                            { "optionId": "always", "name": "Always", "kind": "allow_always" },
                            { "optionId": "no", "name": "Reject", "kind": "reject_once" },
                        ],
                    }}));
                    pending_prompt = Some(id);
                }
            }
            None if msg.get("result").is_some() => {
                // KIVO answered the permission request: finish the prompt accordingly.
                permission_reply = Some(msg["result"].clone());
                let chosen = msg["result"]["outcome"]["optionId"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let update = |u: Value| json!({ "jsonrpc": "2.0", "method": "session/update", "params": { "sessionId": "s1", "update": u } });
                if chosen == "yes" {
                    out.push(update(json!({ "sessionUpdate": "tool_call_update", "toolCallId": "t1", "status": "completed",
                        "content": [{ "type": "diff", "path": "src/lib.rs", "oldText": "a", "newText": "b" }] })));
                    out.push(update(json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "Added the tests." } })));
                } else {
                    out.push(update(json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "Okay, I won't." } })));
                }
                if let Some(prompt_id) = pending_prompt.take() {
                    out.push(json!({ "jsonrpc": "2.0", "id": prompt_id, "result": { "stopReason": "end_turn" } }));
                }
            }
            _ => {
                if !id.is_null() {
                    out.push(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": "nope" } }));
                }
            }
        }
        let _ = &permission_reply;
        for m in out {
            write.write_all(format!("{m}\n").as_bytes()).await.unwrap();
        }
    }
}

async fn connected(allow: bool) -> (Arc<AcpClient>, Arc<Answers>) {
    let (ours, theirs) = tokio::io::duplex(64 * 1024);
    tokio::spawn(fake_agent(theirs));
    let answers = Arc::new(Answers {
        allow,
        asked: Mutex::default(),
    });
    let (read, write) = tokio::io::split(ours);
    let client = AcpClient::connect(read, write, answers.clone());
    (client, answers)
}

async fn drain(mut rx: tokio::sync::mpsc::UnboundedReceiver<AgentEvent>) -> Vec<AgentEvent> {
    let mut out = Vec::new();
    while let Some(e) = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("the agent answered")
    {
        let last = matches!(e, AgentEvent::Done(_) | AgentEvent::Error(_));
        out.push(e);
        if last {
            break;
        }
    }
    out
}

#[tokio::test]
async fn a_session_streams_updates_and_asks_kivo_for_permission() {
    let (client, answers) = connected(true).await;
    let hello = client
        .request("initialize", json!({ "protocolVersion": 1 }))
        .await
        .unwrap();
    assert_eq!(hello["protocolVersion"], 1);
    let session = client
        .session(Path::new("C:/work"), &[], None)
        .await
        .unwrap();
    assert_eq!(session.id(), "s1");
    assert_eq!(session.modes.len(), 2);
    assert_eq!(session.mode().as_deref(), Some("default"));

    let events = drain(session.prompt("add tests", CancellationToken::new())).await;
    assert!(
        events.contains(&AgentEvent::Thought("secret plan".into())),
        "reasoning is kept apart"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AgentEvent::Plan(p) if p[0].content == "Add tests"))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AgentEvent::Tool { kind, .. } if kind == "edit"))
    );
    assert!(events.contains(&AgentEvent::FileDiff {
        path: "src/lib.rs".into()
    }));
    assert!(events.contains(&AgentEvent::Message("Added the tests.".into())));
    assert_eq!(events.last(), Some(&AgentEvent::Done("end_turn".into())));
    let asked = answers.asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 1);
    assert_eq!(
        (asked[0].kind.as_str(), asked[0].title.as_str()),
        ("edit", "Edit src/lib.rs")
    );

    session.set_mode("bypassPermissions").await.unwrap();
    assert_eq!(session.mode().as_deref(), Some("bypassPermissions"));
}

#[tokio::test]
async fn a_refusal_goes_back_to_the_agent() {
    let (client, _) = connected(false).await;
    let session = client
        .session(Path::new("C:/work"), &[], None)
        .await
        .unwrap();
    let events = drain(session.prompt("add tests", CancellationToken::new())).await;
    assert!(events.contains(&AgentEvent::Message("Okay, I won't.".into())));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AgentEvent::FileDiff { .. }))
    );
}

#[tokio::test]
async fn cancelling_a_prompt_tells_the_agent_and_ends_the_stream() {
    let (client, _) = connected(true).await;
    let session = client
        .session(Path::new("C:/work"), &[], None)
        .await
        .unwrap();
    let cancel = CancellationToken::new();
    let mut rx = session.prompt("wait", cancel.clone());
    assert_eq!(rx.recv().await, Some(AgentEvent::Message("Working".into())));
    let started = std::time::Instant::now();
    cancel.cancel();
    let events = drain(rx).await;
    assert_eq!(events.last(), Some(&AgentEvent::Done("cancelled".into())));
    assert!(started.elapsed() < Duration::from_millis(500));
}

/// BRAIN-16: the MCP servers KIVO passes (its own server) reach the agent in `session/new`.
#[tokio::test]
async fn a_new_session_carries_kivos_mcp_server() {
    let (client, _) = connected(true).await;
    client.request("initialize", json!({})).await.unwrap();
    let kivo = crate::acp::McpServer {
        name: "kivo-brain16".into(),
        command: "C:/KIVO/kivo-runtime.exe".into(),
        args: vec!["--mcp-server".into(), "--agent".into(), "gemini".into()],
        env: Vec::new(),
    };
    client
        .session(Path::new("C:/work"), &[kivo], None)
        .await
        .unwrap();
    let seen = SEEN_SERVERS.lock().unwrap().clone();
    let ours = seen
        .iter()
        .filter_map(Value::as_array)
        .flatten()
        .find(|s| s["name"] == "kivo-brain16")
        .expect("the agent got KIVO's server");
    assert_eq!(ours["command"], "C:/KIVO/kivo-runtime.exe");
    assert_eq!(ours["args"], json!(["--mcp-server", "--agent", "gemini"]));
    assert_eq!(ours["env"], json!([]));
}

#[tokio::test]
async fn a_stored_session_is_resumed_when_the_agent_can() {
    let (client, _) = connected(true).await;
    client.request("initialize", json!({})).await.unwrap();
    // `connect` doesn't say hello by itself; `spawn` does. Mark resumable as the hello would.
    client.hello.lock().unwrap().load_session = true;
    let session = client
        .session(Path::new("C:/work"), &[], Some("stored-42"))
        .await
        .unwrap();
    assert_eq!(session.id(), "stored-42");
}

#[test]
fn allow_and_deny_map_onto_the_agents_options() {
    let ask = PermissionAsk {
        session_id: "s".into(),
        tool_call_id: "t".into(),
        title: "Run tests".into(),
        kind: "execute".into(),
        options: vec![
            ("a".into(), "Allow".into(), "allow_once".into()),
            ("aa".into(), "Always".into(), "allow_always".into()),
            ("r".into(), "Reject".into(), "reject_once".into()),
        ],
    };
    assert_eq!(pick(&ask, true, false).as_deref(), Some("a"));
    assert_eq!(pick(&ask, true, true).as_deref(), Some("aa"));
    assert_eq!(
        pick(&ask, false, true).as_deref(),
        Some("r"),
        "falls back to reject once"
    );
}

/// M3-X3: an agent process that dies mid-prompt ends the stream with an error; the caller goes on.
#[tokio::test]
async fn an_agent_that_dies_mid_prompt_is_an_error_not_a_crash() {
    let Some(node) = which("node") else {
        eprintln!("node isn't installed; skipping");
        return;
    };
    // A tiny agent: says hello, starts a session, then exits in the middle of the prompt.
    let script = r#"
const rl = require('readline').createInterface({ input: process.stdin });
const send = m => process.stdout.write(JSON.stringify(m) + '\n');
rl.on('line', l => {
  const m = JSON.parse(l);
  if (m.method === 'initialize') send({ jsonrpc: '2.0', id: m.id, result: { protocolVersion: 1, agentCapabilities: {} } });
  else if (m.method === 'session/new') send({ jsonrpc: '2.0', id: m.id, result: { sessionId: 'x' } });
  else if (m.method === 'session/prompt') {
    send({ jsonrpc: '2.0', method: 'session/update', params: { sessionId: 'x', update: { sessionUpdate: 'agent_message_chunk', content: { type: 'text', text: 'Start' } } } });
    setTimeout(() => process.exit(3), 50);
  }
});"#;
    let command = AgentCommand {
        program: node,
        args: vec!["-e".into(), script.into()],
        env: Vec::new(),
    };
    let answers = Arc::new(Answers {
        allow: true,
        asked: Mutex::default(),
    });
    let client = AcpClient::spawn(&command, &std::env::temp_dir(), answers)
        .await
        .expect("the agent starts");
    let session = client
        .session(&std::env::temp_dir(), &[], None)
        .await
        .unwrap();
    let events = drain(session.prompt("go", CancellationToken::new())).await;
    assert_eq!(events.first(), Some(&AgentEvent::Message("Start".into())));
    assert!(
        matches!(
            events.last(),
            Some(AgentEvent::Error(NormalizedError::ProviderDown(_)))
        ),
        "{events:?}"
    );
    assert!(client.is_closed());
    // Later calls fail cleanly too.
    assert!(
        client
            .session(&std::env::temp_dir(), &[], None)
            .await
            .is_err()
    );
}

/// BRAIN-14: the real Gemini CLI starts in ACP mode and says hello (no prompt is sent, so no
/// quota is used). Runs with KIVO_TEST_REAL_AGENTS=1 where `gemini` is installed.
#[tokio::test]
async fn the_gemini_cli_speaks_acp() {
    if std::env::var_os("KIVO_TEST_REAL_AGENTS").is_none() {
        eprintln!("KIVO_TEST_REAL_AGENTS not set; skipping");
        return;
    }
    let Some(gemini) = which("gemini.cmd").or_else(|| which("gemini")) else {
        eprintln!("gemini isn't installed; skipping");
        return;
    };
    let answers = Arc::new(Answers {
        allow: false,
        asked: Mutex::default(),
    });
    let command = AgentCommand {
        program: gemini,
        args: vec!["--acp".into()],
        env: Vec::new(),
    };
    let client = tokio::time::timeout(
        Duration::from_secs(60),
        AcpClient::spawn(&command, &std::env::temp_dir(), answers),
    )
    .await
    .expect("it answers within a minute")
    .expect("it speaks ACP");
    let hello = client.hello.lock().unwrap().clone();
    eprintln!("gemini ACP hello: {hello:?}");
    assert_eq!(hello.protocol_version, 1);
    client.stop();
}

/// BRAIN-14: OpenCode speaks ACP itself (`opencode acp`), with its own free models.
#[tokio::test]
async fn opencode_speaks_acp() {
    real_agent(&["opencode.cmd", "opencode"], &["acp"]).await;
}

/// An agent that never answers (one waiting for its first sign-in) fails to open a session
/// after `SETUP_LIMIT` instead of hanging, so it can't hold every later request to it.
#[tokio::test(start_paused = true)]
async fn an_agent_that_never_answers_fails_instead_of_hanging() {
    let (ours, theirs) = tokio::io::duplex(64 * 1024);
    // It reads everything and says nothing.
    tokio::spawn(async move {
        let mut lines = BufReader::new(theirs).lines();
        while let Ok(Some(_)) = lines.next_line().await {}
    });
    let answers = Arc::new(Answers {
        allow: false,
        asked: Mutex::default(),
    });
    let (read, write) = tokio::io::split(ours);
    let client = AcpClient::connect(read, write, answers);
    let started = tokio::time::Instant::now();
    let failed = client
        .session(Path::new("."), &[], None)
        .await
        .err()
        .expect("no session");
    assert!(
        matches!(failed, NormalizedError::ProviderDown(ref m) if m.contains("sign in")),
        "{failed:?}"
    );
    assert!(started.elapsed() >= crate::acp::SETUP_LIMIT);
}

/// Starts a real, installed agent over ACP and checks the handshake; with
/// `KIVO_TEST_REAL_AGENT_PROMPTS=1` it also opens a session in an empty folder and sends one
/// one-word prompt (a live run, BRAIN-14: it spends a request of the user's own plan).
async fn real_agent(names: &[&str], args: &[&str]) {
    if std::env::var_os("KIVO_TEST_REAL_AGENTS").is_none() {
        eprintln!("KIVO_TEST_REAL_AGENTS not set; skipping");
        return;
    }
    let Some(program) = names.iter().find_map(|n| which(n)) else {
        eprintln!("{} isn't installed; skipping", names[0]);
        return;
    };
    let answers = Arc::new(Answers {
        allow: false,
        asked: Mutex::default(),
    });
    let command = AgentCommand {
        program,
        args: args.iter().map(ToString::to_string).collect(),
        env: Vec::new(),
    };
    let dir = std::env::temp_dir().join(format!("kivo-acp-{}-{}", names[0], std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let client = tokio::time::timeout(
        Duration::from_secs(60),
        AcpClient::spawn(&command, &dir, answers),
    )
    .await
    .expect("it answers within a minute")
    .expect("it speaks ACP");
    let hello = client.hello.lock().unwrap().clone();
    eprintln!("{} ACP hello: {hello:?}", names[0]);
    assert_eq!(hello.protocol_version, 1);
    if std::env::var_os("KIVO_TEST_REAL_AGENT_PROMPTS").is_some() {
        let session = match client.session(&dir, &[], None).await {
            Ok(s) => s,
            Err(e) => {
                client.stop();
                panic!("{}: no session: {e}", names[0]);
            }
        };
        let mut events = session.prompt(
            "Reply with exactly one word: ready. Don't use any tools.",
            CancellationToken::new(),
        );
        let mut text = String::new();
        let ended = tokio::time::timeout(Duration::from_secs(120), async {
            while let Some(event) = events.recv().await {
                match event {
                    AgentEvent::Message(t) => text.push_str(&t),
                    AgentEvent::Done(reason) => return Ok(reason),
                    AgentEvent::Error(e) => return Err(e),
                    _ => {}
                }
            }
            Err(NormalizedError::Other("the stream ended".into()))
        })
        .await
        .expect("it answered within two minutes");
        eprintln!("{} answered {text:?}: {ended:?}", names[0]);
        assert_eq!(ended.as_deref(), Ok("end_turn"));
        assert!(text.to_lowercase().contains("ready"), "{text}");
    }
    client.stop();
    let _ = std::fs::remove_dir_all(&dir);
}

/// BRAIN-14: Claude Code through Zed's adapter (`claude-agent-acp`).
#[tokio::test]
async fn claude_code_speaks_acp() {
    real_agent(&["claude-agent-acp.cmd", "claude-agent-acp"], &[]).await;
}

/// BRAIN-14: Codex through Zed's adapter (`codex-acp`).
#[tokio::test]
async fn codex_speaks_acp() {
    real_agent(&["codex-acp.cmd", "codex-acp"], &[]).await;
}

fn which(name: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
        .or_else(|| {
            std::env::split_paths(&std::env::var_os("PATH")?)
                .map(|dir| dir.join(format!("{name}.exe")))
                .find(|p| p.is_file())
        })
}
