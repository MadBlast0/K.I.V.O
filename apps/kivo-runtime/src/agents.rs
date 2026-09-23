//! CLI agents over ACP (BRAINS §4, §7; CONVERSATION §5.1): one agent process per agent and
//! workspace, started on first use and kept for follow-ups ("Tell Claude …", CONV-12), with its
//! session id stored so a restarted agent resumes the same session (CONV-02). An agent that dies
//! is forgotten and started again next time; it never takes the runtime down (invariant 9).

use kivo_brain::NormalizedError;
use kivo_brain::acp::PermissionHandler;
use kivo_brain::acp::{AcpClient, AcpSession, AgentCommand, PermissionAnswer, PermissionAsk};
use kivo_brain::catalog;
use kivo_store::Database;
use kivo_store::brains::{AgentSessionRow, now_ms};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

/// A live agent in a workspace.
struct Live {
    client: Arc<AcpClient>,
    session: Arc<AcpSession>,
}

pub struct Agents {
    db: Arc<Mutex<Database>>,
    live: tokio::sync::Mutex<HashMap<(String, PathBuf), Live>>,
    /// Which agent a session id belongs to (for permission requests).
    owners: Mutex<HashMap<String, String>>,
    /// Commands put in directly (tests), instead of the catalog's program.
    commands: RwLock<HashMap<String, AgentCommand>>,
    /// Where agents work when no folder is chosen.
    workspace: RwLock<PathBuf>,
    /// Answers the agents' permission requests (the turn engine, set once it exists).
    permissions: RwLock<Option<Arc<dyn PermissionHandler>>>,
    /// Sessions a background task is driving: their requests go to the task (BRAIN-32).
    routed: Mutex<HashMap<String, Arc<dyn PermissionHandler>>>,
    /// KIVO's own MCP server for an agent's sessions (BRAIN-16), when the user allowed the agent.
    kivo_server: RwLock<Option<KivoServerFor>>,
}

/// KIVO's MCP server for an agent (by id), if it may use it.
pub type KivoServerFor = Arc<dyn Fn(&str) -> Option<kivo_brain::acp::McpServer> + Send + Sync>;

/// KIVO's MCP server as an agent starts it: this program with `--mcp-server --agent <id>`
/// (TOOL-37), which relays to the running KIVO.
pub fn kivo_server(agent: &str, exe: &Path) -> kivo_brain::acp::McpServer {
    kivo_brain::acp::McpServer {
        name: "kivo".into(),
        command: exe.to_path_buf(),
        args: vec!["--mcp-server".into(), "--agent".into(), agent.to_owned()],
        env: Vec::new(),
    }
}

/// Hands permission requests to whatever answers them now.
struct Relay(Arc<Agents>);

#[async_trait::async_trait]
impl PermissionHandler for Relay {
    async fn ask(&self, ask: PermissionAsk) -> PermissionAnswer {
        let routed = lock(&self.0.routed).get(&ask.session_id).cloned();
        if let Some(task) = routed {
            return task.ask(ask).await;
        }
        let handler = read(&self.0.permissions).clone();
        match handler {
            Some(h) => h.ask(ask).await,
            // Nobody to ask: refuse.
            None => kivo_brain::acp::pick(&ask, false, false),
        }
    }
}

impl Agents {
    pub fn new(db: Arc<Mutex<Database>>, workspace: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            db,
            live: tokio::sync::Mutex::default(),
            owners: Mutex::default(),
            commands: RwLock::default(),
            workspace: RwLock::new(workspace),
            permissions: RwLock::new(None),
            routed: Mutex::default(),
            kivo_server: RwLock::new(None),
        })
    }

    /// Offers KIVO's MCP server to the sessions of agents `server` allows (BRAIN-16).
    pub fn set_kivo_server(&self, server: KivoServerFor) {
        *write(&self.kivo_server) = Some(server);
    }

    pub fn set_permissions(&self, handler: Arc<dyn PermissionHandler>) {
        *write(&self.permissions) = Some(handler);
    }

    /// A background task drives `session` for now: its permission requests go to `handler`.
    pub fn route_session(&self, session: &str, handler: Arc<dyn PermissionHandler>) {
        lock(&self.routed).insert(session.to_owned(), handler);
    }

    pub fn unroute_session(&self, session: &str) {
        lock(&self.routed).remove(session);
    }

    /// Uses `command` for agent `id` (tests run a scripted agent this way).
    pub fn set_command(&self, id: &str, command: AgentCommand) {
        write(&self.commands).insert(id.to_owned(), command);
    }

    pub fn workspace(&self) -> PathBuf {
        read(&self.workspace).clone()
    }

    pub fn set_workspace(&self, folder: PathBuf) {
        *write(&self.workspace) = folder;
    }

    /// The agent a session belongs to.
    pub fn owner_of(&self, session: &str) -> Option<String> {
        lock(&self.owners).get(session).cloned()
    }

    /// How to start agent `id`: a command put in directly, or the catalog's ACP program found
    /// on this PC (`program`).
    fn command(&self, id: &str, program: Option<&Path>) -> Option<AgentCommand> {
        if let Some(c) = read(&self.commands).get(id) {
            return Some(c.clone());
        }
        let (_, args) = catalog::entry(id)?.agent?;
        Some(AgentCommand {
            program: program?.to_path_buf(),
            args: args.iter().map(|a| (*a).to_owned()).collect(),
            env: Vec::new(),
        })
    }

    /// The live session for `id` in `cwd`: the running one, or a new agent process with the
    /// stored session resumed when the agent can (CONV-02).
    pub async fn session(
        self: &Arc<Self>,
        id: &str,
        program: Option<&Path>,
        cwd: &Path,
    ) -> Result<Arc<AcpSession>, NormalizedError> {
        let key = (id.to_owned(), cwd.to_path_buf());
        let mut live = self.live.lock().await;
        if let Some(l) = live.get(&key) {
            if !l.client.is_closed() {
                return Ok(Arc::clone(&l.session));
            }
            live.remove(&key);
        }
        let command = self.command(id, program).ok_or_else(|| {
            NormalizedError::ProviderDown("the agent isn't installed on this PC".into())
        })?;
        let relay: Arc<dyn PermissionHandler> = Arc::new(Relay(Arc::clone(self)));
        let client = AcpClient::spawn(&command, cwd, relay).await?;
        let workspace = cwd.to_string_lossy().into_owned();
        let stored = lock(&self.db)
            .agent_session(id, &workspace)
            .ok()
            .flatten()
            .map(|r| r.id);
        let servers: Vec<kivo_brain::acp::McpServer> = read(&self.kivo_server)
            .as_ref()
            .and_then(|f| f(id))
            .into_iter()
            .collect();
        let session = match client.session(cwd, &servers, stored.as_deref()).await {
            Ok(s) => s,
            // A stored session the agent no longer has: start fresh.
            Err(_) if stored.is_some() => client.session(cwd, &servers, None).await?,
            Err(e) => {
                client.stop();
                return Err(e);
            }
        };
        let now = now_ms();
        let _ = lock(&self.db).save_agent_session(&AgentSessionRow {
            id: session.id().to_owned(),
            agent: id.to_owned(),
            workspace,
            conversation_id: None,
            created_at: now,
            last_used: now,
        });
        lock(&self.owners).insert(session.id().to_owned(), id.to_owned());
        let session = Arc::new(session);
        live.insert(
            key,
            Live {
                client,
                session: Arc::clone(&session),
            },
        );
        Ok(session)
    }

    /// Notes that the session was used (and which KIVO thread it belongs to).
    pub fn touched(&self, id: &str, session: &str, cwd: &Path, thread: Option<&str>) {
        let now = now_ms();
        let _ = lock(&self.db).save_agent_session(&AgentSessionRow {
            id: session.to_owned(),
            agent: id.to_owned(),
            workspace: cwd.to_string_lossy().into_owned(),
            conversation_id: thread.map(str::to_owned),
            created_at: now,
            last_used: now,
        });
    }

    /// Stops agent `id` in `cwd` (it died, or the user disconnected it).
    pub async fn forget(&self, id: &str, cwd: &Path) {
        if let Some(l) = self
            .live
            .lock()
            .await
            .remove(&(id.to_owned(), cwd.to_path_buf()))
        {
            l.client.stop();
        }
    }

    /// Stops every agent (quit, or CLI agents turned off).
    pub async fn stop_all(&self) {
        for (_, l) in self.live.lock().await.drain() {
            l.client.stop();
        }
    }

    /// Agents running now, as (agent, workspace).
    pub async fn running(&self) -> Vec<(String, PathBuf)> {
        self.live
            .lock()
            .await
            .iter()
            .filter(|(_, l)| !l.client.is_closed())
            .map(|(k, _)| k.clone())
            .collect()
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn read<T>(l: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    l.read().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn write<T>(l: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    l.write().unwrap_or_else(std::sync::PoisonError::into_inner)
}
