//! The M6 requests: MCP servers, connectors and skills (the Extensions page, UX-27), and the
//! two requests KIVO's MCP server bridge makes for an agent (TOOL-37, CONV-24).

use crate::connectors::Connectors;
use crate::core::Core;
use crate::engine::Engine;
use crate::mcp::{Mcp, shared_tool_id, without_sensitive};
use crate::skills::Skills;
use kivo_core::text;
use kivo_ipc::RpcError;
use kivo_ipc::protocol::method;
use kivo_mcp::config::{McpServer, Transport, server_id};
use kivo_mcp::server::SharedResult;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;

pub struct ExtensionsRpc {
    pub core: Arc<Core>,
    pub engine: Arc<Engine>,
    pub mcp: Arc<Mcp>,
    pub connectors: Arc<Connectors>,
    pub skills: Arc<Skills>,
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RpcError> {
    serde_json::from_value(params).map_err(RpcError::invalid_params)
}

fn ok<T: serde::Serialize>(value: &T) -> Result<Value, RpcError> {
    serde_json::to_value(value).map_err(|e| RpcError::new(RpcError::INTERNAL, e.to_string()))
}

fn refuse(message: String) -> RpcError {
    RpcError::new(RpcError::REFUSED, message)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Id {
    id: String,
}

impl ExtensionsRpc {
    /// The connector directory, with when KIVO last used each connected one's tools (INT-03).
    fn connectors_with_use(&self) -> Vec<kivo_ipc::protocol::ConnectorView> {
        let uses = self.engine.recorder.last_tool_uses();
        let mut list = self.connectors.list();
        for c in &mut list {
            if let Some(server) = &c.server {
                let prefix = format!("mcp.{server}.");
                c.last_used = uses
                    .iter()
                    .filter(|(tool, _)| tool.starts_with(&prefix))
                    .map(|(_, ts)| *ts)
                    .max();
            }
        }
        list
    }

    /// Handles `name` if it is one of these requests.
    #[allow(clippy::too_many_lines, reason = "one table of requests")]
    pub async fn call(&self, name: &str, params: Value) -> Option<Result<Value, RpcError>> {
        Some(match name {
            // ---- MCP servers (TOOL-34/35/36, DISC-09) -------------------------------------------
            method::MCP_LIST => ok(&self.mcp.views()),
            method::MCP_FOUND => ok(&self.mcp.found_views()),
            method::MCP_IMPORT => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    file: String,
                    #[serde(default)]
                    servers: Vec<String>,
                }
                match parse::<P>(params) {
                    Ok(p) => self
                        .mcp
                        .import(&p.file, &p.servers)
                        .await
                        .map_err(refuse)
                        .and_then(|v| ok(&v)),
                    Err(e) => Err(e),
                }
            }
            method::MCP_ADD => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    name: String,
                    #[serde(default)]
                    url: Option<String>,
                    #[serde(default)]
                    command: Option<String>,
                    #[serde(default)]
                    args: Vec<String>,
                }
                match parse::<P>(params) {
                    Ok(p) => self.add(p.name, p.url, p.command, p.args).await,
                    Err(e) => Err(e),
                }
            }
            method::MCP_REMOVE => match parse::<Id>(params) {
                Ok(Id { id }) => ok(&self.mcp.remove(&id).await),
                Err(e) => Err(e),
            },
            method::MCP_ENABLE => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    id: String,
                    on: bool,
                }
                match parse::<P>(params) {
                    Ok(p) => self
                        .mcp
                        .enable(&p.id, p.on)
                        .await
                        .map_err(refuse)
                        .and_then(|v| ok(&v)),
                    Err(e) => Err(e),
                }
            }
            method::MCP_APPROVE => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    id: String,
                    on: Vec<String>,
                }
                match parse::<P>(params) {
                    Ok(p) => self
                        .mcp
                        .approve(&p.id, &p.on)
                        .await
                        .map_err(refuse)
                        .and_then(|v| ok(&v)),
                    Err(e) => Err(e),
                }
            }
            method::MCP_SET_TOOL => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    id: String,
                    tool: String,
                    #[serde(default)]
                    enabled: Option<bool>,
                    /// A risk, or `null` to go back to the default (Medium).
                    #[serde(default)]
                    risk: Option<Value>,
                }
                let parsed = parse::<P>(params).and_then(|p| {
                    let risk = match p.risk {
                        None => None,
                        Some(Value::Null) => Some(None),
                        Some(v) => Some(Some(
                            serde_json::from_value::<kivo_core::tool::Risk>(v)
                                .map_err(RpcError::invalid_params)?,
                        )),
                    };
                    Ok((p.id, p.tool, p.enabled, risk))
                });
                match parsed {
                    Ok((id, tool, enabled, risk)) => self
                        .mcp
                        .set_tool(&id, &tool, enabled, risk)
                        .await
                        .map_err(refuse)
                        .and_then(|v| ok(&v)),
                    Err(e) => Err(e),
                }
            }

            // ---- KIVO's MCP server, for agents (TOOL-37, CONV-24) --------------------------------
            method::MCP_SHARED_LIST => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    agent: String,
                }
                parse::<P>(params).map(|p| {
                    let allowed = self
                        .core
                        .config()
                        .tools
                        .share_with_agents
                        .contains(&p.agent);
                    if allowed {
                        json!(self.mcp.shared_tools())
                    } else {
                        json!([])
                    }
                })
            }
            method::MCP_SHARED_CALL => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    agent: String,
                    name: String,
                    #[serde(default)]
                    args: Value,
                }
                match parse::<P>(params) {
                    Ok(p) => ok(&self.shared_call(&p.agent, &p.name, p.args).await),
                    Err(e) => Err(e),
                }
            }
            method::MCP_SHARE => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    agent: String,
                    on: bool,
                    #[serde(default)]
                    sensitive: Option<bool>,
                }
                parse::<P>(params).map(|p| {
                    let config = self.core.update_config(|c| {
                        c.tools.share_with_agents.retain(|a| a != &p.agent);
                        if p.on {
                            c.tools.share_with_agents.push(p.agent.clone());
                        }
                        if let Some(s) = p.sensitive {
                            c.tools.share_sensitive = s;
                        }
                    });
                    self.engine.recorder.user_action(
                        "mcp.share",
                        &format!("{} {}", p.agent, if p.on { "on" } else { "off" }),
                    );
                    json!({ "agents": config.tools.share_with_agents, "sensitive": config.tools.share_sensitive })
                })
            }

            // ---- Connectors (INT-02/03/04, DISC-08) ---------------------------------------------
            method::CONNECTORS_LIST => ok(&self.connectors_with_use()),
            method::CONNECTORS_CONNECT => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    #[serde(default)]
                    id: Option<String>,
                    #[serde(default)]
                    url: Option<String>,
                    #[serde(default)]
                    name: Option<String>,
                }
                match parse::<P>(params) {
                    Ok(p) => {
                        // A local connector switched back on.
                        if let Some(id) = &p.id
                            && kivo_mcp::connectors::catalog()
                                .iter()
                                .any(|c| &c.id == id && c.kind == kivo_mcp::connectors::Kind::Local)
                        {
                            self.connectors.turn_on(id);
                            ok(&self.connectors.list())
                        } else {
                            let custom = p
                                .url
                                .as_deref()
                                .map(|u| (u, p.name.as_deref().unwrap_or_default()));
                            self.connectors
                                .connect(p.id.as_deref(), custom)
                                .await
                                .map_err(refuse)
                                .and_then(|v| ok(&v))
                        }
                    }
                    Err(e) => Err(e),
                }
            }
            method::CONNECTORS_DISCONNECT => match parse::<Id>(params) {
                Ok(Id { id }) => self
                    .connectors
                    .disconnect(&id)
                    .await
                    .map_err(refuse)
                    .and_then(|()| ok(&self.connectors.list())),
                Err(e) => Err(e),
            },

            // ---- Skills (CONV-32, DISC-10) ------------------------------------------------------
            method::SKILLS_LIST => ok(&self.skills.list()),
            method::SKILLS_ENABLE => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    id: String,
                    on: bool,
                }
                parse::<P>(params).and_then(|p| {
                    self.skills
                        .enable(&p.id, p.on)
                        .map_err(refuse)
                        .and_then(|()| ok(&self.skills.list()))
                })
            }
            method::SKILLS_READ => {
                parse::<Id>(params).and_then(|Id { id }| self.skills.read(&id).map_err(refuse))
            }
            method::SKILLS_REMOVE => parse::<Id>(params).and_then(|Id { id }| {
                self.skills
                    .remove(&id)
                    .map_err(refuse)
                    .and_then(|()| ok(&self.skills.list()))
            }),
            method::SKILLS_IMPORT => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    path: String,
                }
                parse::<P>(params).and_then(|p| {
                    self.skills
                        .import(std::path::Path::new(&p.path))
                        .map_err(refuse)
                        .map(|name| json!({ "name": name }))
                })
            }
            method::EXTENSIONS_REFRESH => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    section: String,
                }
                match parse::<P>(params) {
                    Ok(p) => {
                        match p.section.as_str() {
                            "skills" => self.skills.scan(),
                            "connectors" => self.connectors.refresh().await,
                            _ => {
                                for s in self.mcp.servers().into_iter().filter(|s| s.enabled) {
                                    self.mcp.refresh(&s.id).await;
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    Err(e) => Err(e),
                }
            }
            _ => return None,
        })
    }

    /// Adds a custom server: a remote MCP URL, or a program on this PC.
    async fn add(
        &self,
        name: String,
        url: Option<String>,
        command: Option<String>,
        args: Vec<String>,
    ) -> Result<Value, RpcError> {
        let transport = match (url, command) {
            (Some(url), _) => {
                let parsed =
                    url::Url::parse(&url).map_err(|_| refuse(text::t("connectors.badUrl")))?;
                let local = matches!(parsed.host_str(), Some("127.0.0.1" | "localhost"));
                if parsed.scheme() != "https" && !local {
                    return Err(refuse(text::t("connectors.badUrl")));
                }
                Transport::Http {
                    url,
                    token: None,
                    oauth: None,
                }
            }
            (None, Some(command)) if !command.trim().is_empty() => Transport::Stdio {
                command,
                args,
                env: BTreeMap::new(),
                cwd: None,
            },
            _ => return Err(refuse(text::t("mcp.needsAddress"))),
        };
        let server = McpServer {
            id: server_id(&name),
            name,
            transport,
            enabled: true,
            source: None,
            tools: BTreeMap::new(),
        };
        self.mcp
            .add(server, Vec::new())
            .await
            .map_err(refuse)
            .and_then(|v| ok(&v))
    }

    /// A call from an agent through KIVO's MCP server: only for agents the user allowed, only the
    /// shared tools, through the permission engine; sensitive memories are left out unless the
    /// user allows them.
    async fn shared_call(&self, agent: &str, name: &str, args: Value) -> SharedResult {
        let config = self.core.config();
        if !config.tools.share_with_agents.iter().any(|a| a == agent) {
            return SharedResult {
                text: text::tf("mcp.notShared", &[("agent", &agent)]),
                is_error: true,
            };
        }
        let Some(tool) = shared_tool_id(name) else {
            return SharedResult {
                text: text::t("mcp.notShared"),
                is_error: true,
            };
        };
        match self.engine.shared_call(agent, tool, args).await {
            Ok(output) => {
                let data = if config.tools.share_sensitive {
                    output.data
                } else {
                    without_sensitive(output.data)
                };
                SharedResult {
                    text: data.to_string(),
                    is_error: false,
                }
            }
            Err(message) => SharedResult {
                text: message,
                is_error: true,
            },
        }
    }
}
