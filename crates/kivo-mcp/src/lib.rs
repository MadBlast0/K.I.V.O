//! MCP for KIVO (TOOLS_AND_CONTROL §9): the client that turns other servers' tools into KIVO tools
//! (TOOL-34/35/36), KIVO's own server for the agents it runs (TOOL-37), and the MCP setups other
//! apps already have, ready to import (DISC-09).

pub mod auth;
pub mod client;
pub mod config;
pub mod connectors;
pub mod hash;
pub mod imports;
pub mod server;
pub mod tool;

pub use client::{CallOutcome, Connection, McpError, RemoteTool};
pub use config::{EnvValue, McpServer, ToolSetting, Transport};
pub use tool::{McpTool, spec_for};
