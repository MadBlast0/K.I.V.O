//! KIVO's own MCP server (TOOL-37, CONV-24): the tools KIVO chooses to share with a CLI agent it
//! runs (Claude Code, Codex, …) — the user's memory first. It holds no power of its own: every
//! list and call goes to a [`Backend`] (the running KIVO, over its secured IPC), where the call is
//! decided by the permission engine like any other.

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// A tool KIVO shares.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SharedTool {
    pub name: String,
    pub description: String,
    pub schema: Value,
}

/// What a shared call returned: text for the agent, and whether it failed (refused, denied).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedResult {
    pub text: String,
    pub is_error: bool,
}

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// Where the server's lists and calls go.
pub trait Backend: Send + Sync + 'static {
    fn list(&self) -> BoxFuture<Result<Vec<SharedTool>, String>>;
    fn call(&self, name: String, args: Value) -> BoxFuture<Result<SharedResult, String>>;
}

/// The server, over whatever transport it is served on (stdio for agents).
#[derive(Clone)]
pub struct KivoServer {
    backend: Arc<dyn Backend>,
}

impl KivoServer {
    pub fn new(backend: Arc<dyn Backend>) -> Self {
        Self { backend }
    }
}

impl ServerHandler for KivoServer {
    fn get_info(&self) -> ServerConfig {
        let mut info = ServerConfig::new(ServerCapabilities::builder().enable_tools().build());
        info.server_info = Implementation::new("kivo", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "KIVO is the user's desktop assistant. Its tools reach the user's memory and notes; \
             every call is checked by KIVO's permission engine and may be refused."
                .into(),
        );
        info
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + Send + '_ {
        let backend = Arc::clone(&self.backend);
        async move {
            let tools = backend
                .list()
                .await
                .map_err(|e| ErrorData::internal_error(e, None))?;
            let tools = tools
                .into_iter()
                .map(|t| {
                    let schema = match t.schema {
                        Value::Object(m) => m,
                        _ => serde_json::Map::new(),
                    };
                    Tool::new(t.name, t.description, schema)
                })
                .collect();
            Ok(ListToolsResult::with_all_items(tools))
        }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResponse, ErrorData>> + Send + '_ {
        let backend = Arc::clone(&self.backend);
        async move {
            let args = request.arguments.map_or(Value::Null, Value::Object);
            let result = backend
                .call(request.name.into_owned(), args)
                .await
                .map_err(|e| ErrorData::internal_error(e, None))?;
            let content = vec![ContentBlock::text(result.text)];
            let result = if result.is_error {
                CallToolResult::error(content)
            } else {
                CallToolResult::success(content)
            };
            Ok(result.into())
        }
    }
}
