//! MCP servers in the runtime (TOOLS_AND_CONTROL §9, TOOL-34/35/36; DISCOVERY §1.2, DISC-09/16).
//!
//! - Servers live in the store (their setup and the user's decisions per tool); secrets live in
//!   Credential Manager under `kivo-mcp`.
//! - Enabled servers are connected at start, in the background. A tool joins KIVO's tools only
//!   once the user approved it: a new tool waits for review, and a tool whose name, description or
//!   schema changed after it was approved (its fingerprint no longer matches) is switched off
//!   until it is reviewed again. `tools/list_changed` triggers the comparison.
//! - Other apps' MCP setups are found and imported by copying; the originals are never written.
//! - KIVO's own MCP server shares a chosen set of tools with the agents the user allowed
//!   (TOOL-37, CONV-24); see [`Mcp::shared_tools`].

use crate::core::Core;
use kivo_core::event::{EventKind, SystemEvent};
use kivo_core::text;
use kivo_core::{Capability, Event};
use kivo_mcp::config::{EnvValue, McpServer, ToolSetting, Transport, server_id};
use kivo_mcp::imports::{Found, Places};
use kivo_mcp::{Connection, McpTool, RemoteTool, spec_for};
use kivo_platform::{SecretHandle, Secrets};
use kivo_store::Database;
use kivo_tools::{Registry, Tool};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// The Credential Manager "provider" MCP secrets are kept under.
pub const SECRETS: &str = "kivo-mcp";
/// The tools KIVO's MCP server can share with agents (TOOL-37): the memory tools (CONV-24).
pub const SHAREABLE: &[&str] = &["memory.search", "memory.get", "memory.add", "memory.tags"];

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub use kivo_ipc::protocol::{
    McpFoundView as FoundView, McpServerView as ServerView, McpToolView as ToolView,
};

/// Secrets and sign-ins in Credential Manager, under [`SECRETS`].
pub struct SecretVault(pub Arc<dyn Secrets>);

fn handle(name: &str) -> SecretHandle {
    SecretHandle {
        provider: SECRETS.into(),
        name: name.into(),
    }
}

struct SecretTokens {
    secrets: Arc<dyn Secrets>,
    name: String,
}

impl kivo_mcp::auth::TokenStore for SecretTokens {
    fn load(&self) -> Option<String> {
        self.secrets
            .get(&handle(&self.name))
            .ok()
            .flatten()
            .map(|s| s.expose().to_owned())
    }

    fn save(&self, json: &str) -> Result<(), String> {
        self.secrets
            .set(
                &handle(&self.name),
                kivo_platform::Secret::new(json.to_owned()),
            )
            .map_err(|e| e.to_string())
    }

    fn clear(&self) {
        let _ = self.secrets.delete(&handle(&self.name));
    }
}

impl kivo_mcp::client::Vault for SecretVault {
    fn secret(&self, name: &str) -> Option<String> {
        self.0
            .get(&handle(name))
            .ok()
            .flatten()
            .map(|s| s.expose().to_owned())
    }

    fn tokens(&self, name: &str) -> Arc<dyn kivo_mcp::auth::TokenStore> {
        Arc::new(SecretTokens {
            secrets: Arc::clone(&self.0),
            name: name.to_owned(),
        })
    }
}

#[derive(Default)]
struct Live {
    conns: HashMap<String, Arc<Connection>>,
    tools: HashMap<String, Vec<RemoteTool>>,
    status: HashMap<String, (String, Option<String>)>,
}

pub struct Mcp {
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
    core: Arc<Core>,
    db: Arc<Mutex<Database>>,
    registry: Arc<Registry>,
    secrets: Arc<dyn Secrets>,
    live: Mutex<Live>,
    places: Mutex<Places>,
}

impl Mcp {
    pub fn new(
        core: Arc<Core>,
        db: Arc<Mutex<Database>>,
        registry: Arc<Registry>,
        secrets: Arc<dyn Secrets>,
        places: Places,
    ) -> Arc<Self> {
        Arc::new(Self {
            watcher: Mutex::default(),
            core,
            db,
            registry,
            secrets,
            live: Mutex::default(),
            places: Mutex::new(places),
        })
    }

    /// Project folders to look in for `.mcp.json` and `.vscode/mcp.json` (the workspaces).
    pub fn set_projects(&self, projects: Vec<std::path::PathBuf>) {
        lock(&self.places).projects = projects;
    }

    /// Credential Manager, for connections made outside the manager (a connector's probe).
    pub fn secrets(&self) -> Arc<dyn Secrets> {
        Arc::clone(&self.secrets)
    }

    /// The store for a connector's sign-in (its OAuth tokens).
    pub fn tokens(&self, name: &str) -> Arc<dyn kivo_mcp::auth::TokenStore> {
        kivo_mcp::client::Vault::tokens(&SecretVault(Arc::clone(&self.secrets)), name)
    }

    fn put_secret(&self, name: &str, value: &str) -> Result<(), String> {
        self.secrets
            .set(
                &SecretHandle {
                    provider: SECRETS.into(),
                    name: name.into(),
                },
                kivo_platform::Secret::new(value.to_owned()),
            )
            .map_err(|e| e.to_string())
    }

    /// Watches other apps' MCP setups (DISC-16): an edited file shows up on the page by itself.
    /// Only read; nothing is imported until the user says so.
    pub fn watch(self: &Arc<Self>) {
        use notify::{RecursiveMode, Watcher};
        let weak = Arc::downgrade(self);
        let places = lock(&self.places).clone();
        let files: Vec<std::path::PathBuf> = kivo_mcp::imports::candidates(&places)
            .into_iter()
            .map(|(_, _, file)| file)
            .collect();
        let wanted = files.clone();
        let Ok(mut watcher) =
            notify::recommended_watcher(move |r: notify::Result<notify::Event>| {
                if let Ok(event) = r
                    && event.paths.iter().any(|p| wanted.contains(p))
                    && let Some(this) = weak.upgrade()
                {
                    this.changed();
                }
            })
        else {
            return;
        };
        // The folders that exist (a file can't be watched before it's made); events are
        // filtered to the setup files themselves.
        let mut folders: Vec<&std::path::Path> = files
            .iter()
            .filter_map(|f| f.parent())
            .filter(|d| d.is_dir())
            .collect();
        folders.sort();
        folders.dedup();
        for dir in folders {
            let _ = watcher.watch(dir, RecursiveMode::NonRecursive);
        }
        *lock(&self.watcher) = Some(watcher);
    }

    /// Every server in KIVO's setup.
    pub fn servers(&self) -> Vec<McpServer> {
        lock(&self.db)
            .mcp_servers()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(_, body)| serde_json::from_str(&body).ok())
            .collect()
    }

    pub fn server(&self, id: &str) -> Option<McpServer> {
        lock(&self.db)
            .mcp_server(id)
            .ok()
            .flatten()
            .and_then(|b| serde_json::from_str(&b).ok())
    }

    fn save(&self, server: &McpServer) -> Result<(), String> {
        let body = serde_json::to_string(server).map_err(|e| e.to_string())?;
        lock(&self.db)
            .save_mcp_server(&server.id, &body)
            .map_err(|e| e.to_string())
    }

    fn changed(&self) {
        self.core.bus.publish(Event::new(EventKind::System(
            SystemEvent::DiscoveryChanged {
                section: "mcp".into(),
            },
        )));
    }

    /// An id not used by another server.
    fn free_id(&self, wanted: &str) -> String {
        let taken: Vec<String> = self.servers().into_iter().map(|s| s.id).collect();
        let base = server_id(wanted);
        if !taken.contains(&base) {
            return base;
        }
        (2..)
            .map(|n| format!("{base}-{n}"))
            .find(|id| !taken.contains(id))
            .unwrap_or(base)
    }

    /// Adds a server (a custom one, an import, a connector) with its secrets, and connects it.
    pub async fn add(
        self: &Arc<Self>,
        mut server: McpServer,
        secrets: Vec<(String, String)>,
    ) -> Result<ServerView, String> {
        let wanted = server.id.clone();
        server.id = self.free_id(&wanted);
        if server.id != wanted {
            // Secret names carry the id.
            let rename =
                |n: &str| n.replacen(&format!("mcp.{wanted}."), &format!("mcp.{}.", server.id), 1);
            if let Transport::Stdio { env, .. } = &mut server.transport {
                for v in env.values_mut() {
                    if let EnvValue::Secret(n) = v {
                        *n = rename(n);
                    }
                }
            }
            if let Transport::Http { token: Some(t), .. } = &mut server.transport {
                *t = rename(t);
            }
            for (name, value) in &secrets {
                self.put_secret(&rename(name), value)?;
            }
        } else {
            for (name, value) in &secrets {
                self.put_secret(name, value)?;
            }
        }
        self.save(&server)?;
        if server.enabled {
            self.connect(&server.id).await;
        }
        self.changed();
        self.view(&server.id).ok_or_else(|| text::t("mcp.notFound"))
    }

    /// Removes a server, its tools and its secrets.
    pub async fn remove(&self, id: &str) -> bool {
        self.disconnect(id).await;
        if let Some(server) = self.server(id) {
            let mut names: Vec<String> = Vec::new();
            match &server.transport {
                Transport::Stdio { env, .. } => {
                    names.extend(env.values().filter_map(|v| match v {
                        EnvValue::Secret(n) => Some(n.clone()),
                        EnvValue::Plain(_) => None,
                    }))
                }
                Transport::Http { token, oauth, .. } => {
                    names.extend(token.clone());
                    names.extend(oauth.clone());
                }
            }
            for n in names {
                let _ = self.secrets.delete(&SecretHandle {
                    provider: SECRETS.into(),
                    name: n,
                });
            }
        }
        let removed = lock(&self.db).delete_mcp_server(id).unwrap_or(false);
        self.changed();
        removed
    }

    /// Switches a server on (connects it) or off.
    pub async fn enable(self: &Arc<Self>, id: &str, on: bool) -> Result<ServerView, String> {
        let mut server = self.server(id).ok_or_else(|| text::t("mcp.notFound"))?;
        server.enabled = on;
        self.save(&server)?;
        if on {
            self.connect(id).await;
        } else {
            self.disconnect(id).await;
        }
        self.changed();
        self.view(id).ok_or_else(|| text::t("mcp.notFound"))
    }

    async fn disconnect(&self, id: &str) {
        let conn = {
            let mut live = lock(&self.live);
            live.tools.remove(id);
            live.status.insert(id.to_owned(), ("stopped".into(), None));
            live.conns.remove(id)
        };
        self.registry.set_live(&format!("mcp.{id}."), Vec::new());
        if let Some(conn) = conn {
            conn.close().await;
        }
    }

    /// Connects a server, lists its tools, registers the approved ones, and follows
    /// `tools/list_changed`.
    pub async fn connect(self: &Arc<Self>, id: &str) {
        let Some(server) = self.server(id) else {
            return;
        };
        self.disconnect(id).await;
        lock(&self.live)
            .status
            .insert(id.to_owned(), ("connecting".into(), None));
        let vault = SecretVault(Arc::clone(&self.secrets));
        let shutdown = self.core.shutdown();
        let conn = match Connection::connect(&server, &vault, &shutdown).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(server = %server.id, %e, "couldn't connect to an MCP server");
                lock(&self.live)
                    .status
                    .insert(id.to_owned(), ("error".into(), Some(e.to_string())));
                self.changed();
                return;
            }
        };
        lock(&self.live)
            .conns
            .insert(id.to_owned(), Arc::clone(&conn));
        self.refresh(id).await;
        // Follow the server's changes (TOOL-36, DISC-16).
        let this = Arc::clone(self);
        let id = id.to_owned();
        let mut changes = conn.tools_changed();
        let shutdown = self.core.shutdown();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    r = changes.changed() => {
                        if r.is_err() { break }
                        let still = lock(&this.live).conns.get(&id).is_some_and(|c| Arc::ptr_eq(c, &conn));
                        if !still { break }
                        this.refresh(&id).await;
                    }
                    () = shutdown.cancelled() => break,
                }
            }
        });
    }

    /// Lists a connected server's tools again and registers the ones the user approved and that
    /// haven't changed since.
    pub async fn refresh(&self, id: &str) {
        let Some(conn) = lock(&self.live).conns.get(id).cloned() else {
            return;
        };
        let Some(server) = self.server(id) else {
            return;
        };
        match conn.tools().await {
            Ok(tools) => {
                let capability = if server
                    .source
                    .as_deref()
                    .is_some_and(|s| s.starts_with("connector:"))
                {
                    Capability::Integrations
                } else {
                    Capability::McpServers
                };
                let handle = tokio::runtime::Handle::current();
                let mut registered: Vec<Arc<dyn Tool>> = Vec::new();
                let mut review = false;
                for t in &tools {
                    let setting = server.tools.get(&t.name);
                    match setting {
                        Some(s) if s.approved == t.hash => {
                            if s.enabled {
                                registered.push(Arc::new(McpTool::new(
                                    spec_for(&server, t, Some(s), capability),
                                    t.name.clone(),
                                    server.name.clone(),
                                    Arc::clone(&conn),
                                    handle.clone(),
                                )));
                            }
                        }
                        _ => review = true,
                    }
                }
                self.registry.set_live(&format!("mcp.{id}."), registered);
                let status = if review { "needsReview" } else { "running" };
                let mut live = lock(&self.live);
                live.tools.insert(id.to_owned(), tools);
                live.status.insert(id.to_owned(), (status.into(), None));
            }
            Err(e) => {
                lock(&self.live)
                    .status
                    .insert(id.to_owned(), ("error".into(), Some(e.to_string())));
            }
        }
        self.changed();
    }

    /// Approves a server's tools as they are now (the user reviewed them), switching on those in
    /// `on` and off the rest of the ones reviewed.
    pub async fn approve(self: &Arc<Self>, id: &str, on: &[String]) -> Result<ServerView, String> {
        let mut server = self.server(id).ok_or_else(|| text::t("mcp.notFound"))?;
        let tools = lock(&self.live).tools.get(id).cloned().unwrap_or_default();
        for t in &tools {
            let entry = server.tools.entry(t.name.clone()).or_default();
            entry.approved.clone_from(&t.hash);
            entry.enabled = on.contains(&t.name);
        }
        self.save(&server)?;
        self.refresh(id).await;
        self.view(id).ok_or_else(|| text::t("mcp.notFound"))
    }

    /// Changes one approved tool: on or off, and its risk.
    pub async fn set_tool(
        self: &Arc<Self>,
        id: &str,
        tool: &str,
        enabled: Option<bool>,
        risk: Option<Option<kivo_core::tool::Risk>>,
    ) -> Result<ServerView, String> {
        let mut server = self.server(id).ok_or_else(|| text::t("mcp.notFound"))?;
        let entry: &mut ToolSetting = server
            .tools
            .get_mut(tool)
            .ok_or_else(|| text::t("mcp.reviewFirst"))?;
        if let Some(on) = enabled {
            entry.enabled = on;
        }
        if let Some(r) = risk {
            entry.risk = r;
        }
        self.save(&server)?;
        self.refresh(id).await;
        self.view(id).ok_or_else(|| text::t("mcp.notFound"))
    }

    /// How a server looks on the page.
    pub fn view(&self, id: &str) -> Option<ServerView> {
        let server = self.server(id)?;
        let live = lock(&self.live);
        let (status, error) = live.status.get(id).cloned().unwrap_or_else(|| {
            (
                if server.enabled {
                    "connecting"
                } else {
                    "stopped"
                }
                .into(),
                None,
            )
        });
        let tools = live
            .tools
            .get(id)
            .map(|tools| {
                tools
                    .iter()
                    .map(|t| {
                        let s = server.tools.get(&t.name);
                        let state = match s {
                            Some(s) if s.approved == t.hash => "approved",
                            Some(s) if !s.approved.is_empty() => "changed",
                            _ => "new",
                        };
                        let (description, _) = kivo_mcp::client::clip(&t.description, 2_000);
                        ToolView {
                            name: t.name.clone(),
                            description,
                            risk: s
                                .and_then(|s| s.risk)
                                .unwrap_or(kivo_core::tool::Risk::Medium),
                            enabled: state == "approved" && s.is_some_and(|s| s.enabled),
                            state: state.into(),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(ServerView {
            id: server.id.clone(),
            name: server.name.clone(),
            kind: if server.remote() { "remote" } else { "local" }.into(),
            source: server.source.clone(),
            enabled: server.enabled,
            status,
            error,
            tools,
        })
    }

    pub fn views(&self) -> Vec<ServerView> {
        self.servers()
            .iter()
            .filter_map(|s| self.view(&s.id))
            .collect()
    }

    /// Connects every enabled server (at start, in the background).
    pub async fn start(self: &Arc<Self>) {
        for s in self.servers().into_iter().filter(|s| s.enabled) {
            self.connect(&s.id).await;
        }
    }

    /// Other apps' MCP setups (DISC-09), with which of their servers aren't imported yet.
    pub fn found(&self) -> Vec<(Found, Vec<String>)> {
        let places = lock(&self.places).clone();
        let mine = self.servers();
        kivo_mcp::imports::find_all(&places)
            .into_iter()
            .map(|f| {
                let new = f
                    .servers
                    .iter()
                    .filter(|i| {
                        !mine
                            .iter()
                            .any(|m| m.name == i.server.name && m.source == i.server.source)
                    })
                    .map(|i| i.server.name.clone())
                    .collect();
                (f, new)
            })
            .collect()
    }

    pub fn found_views(&self) -> Vec<FoundView> {
        self.found()
            .into_iter()
            .map(|(f, new)| FoundView {
                app: f.app,
                name: f.name,
                file: f.file.display().to_string(),
                servers: f.servers.iter().map(|s| s.server.name.clone()).collect(),
                new,
            })
            .collect()
    }

    /// Imports the servers named `names` (all not yet imported when empty) from the setup in
    /// `file`: copied into KIVO, secrets into Credential Manager, tools waiting for review. The
    /// other app's file is only read.
    pub async fn import(
        self: &Arc<Self>,
        file: &str,
        names: &[String],
    ) -> Result<Vec<ServerView>, String> {
        let (found, new) = self
            .found()
            .into_iter()
            .find(|(f, _)| f.file.display().to_string() == file)
            .ok_or_else(|| text::t("mcp.notFound"))?;
        let wanted: Vec<String> = if names.is_empty() {
            new
        } else {
            names.to_vec()
        };
        let mut out = Vec::new();
        for imported in found.servers {
            if !wanted.contains(&imported.server.name) {
                continue;
            }
            out.push(self.add(imported.server, imported.secrets).await?);
        }
        Ok(out)
    }

    /// The tools KIVO's MCP server shares (TOOL-37): the shareable ones whose capability is on.
    pub fn shared_tools(&self) -> Vec<kivo_mcp::server::SharedTool> {
        let caps = self.core.config().capabilities;
        self.registry
            .available(&caps)
            .filter(|s| SHAREABLE.contains(&s.id.as_str()))
            .map(|s| kivo_mcp::server::SharedTool {
                // MCP clients want names without dots.
                name: s.id.replace('.', "_"),
                description: s.description,
                schema: s.params,
            })
            .collect()
    }
}

/// The KIVO tool a shared tool's name stands for (`memory_search` → `memory.search`).
pub fn shared_tool_id(name: &str) -> Option<&'static str> {
    SHAREABLE
        .iter()
        .copied()
        .find(|id| id.replace('.', "_") == name)
}

/// Removes sensitive items from a memory tool's result for an agent that may not see them
/// (CONV-24: sensitive memories never go to cloud agents unless the user allows it).
pub fn without_sensitive(mut data: serde_json::Value) -> serde_json::Value {
    if let Some(items) = data
        .get_mut("items")
        .and_then(serde_json::Value::as_array_mut)
    {
        items.retain(|i| i["sensitive"] != true);
    }
    if data["sensitive"] == true {
        return serde_json::json!({ "withheld": true });
    }
    data
}
