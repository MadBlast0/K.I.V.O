//! `kivo-runtime --mcp-server --agent <id>`: KIVO's MCP server for a CLI agent (TOOL-37,
//! BRAIN-16). The agent starts it as a stdio MCP server; it holds nothing itself and relays each
//! list and call to the running KIVO over its secured IPC, where the user's sharing choice and the
//! permission engine decide.

use kivo_ipc::protocol::method;
use kivo_mcp::server::{Backend, BoxFuture, KivoServer, SharedResult, SharedTool};
use rmcp::ServiceExt;
use serde_json::{Value, json};
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

struct Relay {
    client: kivo_ipc::Client,
    agent: String,
}

impl Backend for Relay {
    fn list(&self) -> BoxFuture<Result<Vec<SharedTool>, String>> {
        let client = self.client.clone();
        let agent = self.agent.clone();
        Box::pin(async move {
            let v = client
                .request(method::MCP_SHARED_LIST, json!({ "agent": agent }))
                .await
                .map_err(|e| e.to_string())?;
            serde_json::from_value(v).map_err(|e| e.to_string())
        })
    }

    fn call(&self, name: String, args: Value) -> BoxFuture<Result<SharedResult, String>> {
        let client = self.client.clone();
        let agent = self.agent.clone();
        Box::pin(async move {
            let v = client
                .request(
                    method::MCP_SHARED_CALL,
                    json!({ "agent": agent, "name": name, "args": args }),
                )
                .await
                .map_err(|e| e.to_string())?;
            serde_json::from_value(v).map_err(|e| e.to_string())
        })
    }
}

/// Serves MCP on stdin/stdout until the agent closes it.
pub async fn run(agent: &str, endpoint: &str, token_file: &Path) -> ExitCode {
    let Ok(token) = kivo_ipc::SessionToken::read(token_file) else {
        eprintln!("kivo-runtime: KIVO isn't running");
        return ExitCode::FAILURE;
    };
    let connection =
        match kivo_ipc::client::connect(endpoint, token.as_str(), &format!("kivo-mcp:{agent}"))
            .await
        {
            Ok(c) => c,
            Err(e) => {
                eprintln!("kivo-runtime: couldn't reach KIVO: {e}");
                return ExitCode::FAILURE;
            }
        };
    let relay = Relay {
        client: connection.client,
        agent: agent.to_owned(),
    };
    let server = KivoServer::new(Arc::new(relay));
    match server.serve(rmcp::transport::stdio()).await {
        Ok(service) => {
            let _ = service.waiting().await;
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("kivo-runtime: MCP handshake failed: {e}");
            ExitCode::FAILURE
        }
    }
}
