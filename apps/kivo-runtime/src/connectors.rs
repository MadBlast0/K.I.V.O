//! Connectors (INTEGRATIONS §0–1, INT-02/03/04; DISCOVERY §1.2, DISC-08).
//!
//! - **Remote** connectors are the vendors' remote MCP servers. Connect signs in in the system
//!   browser (MCP authorization with PKCE and KIVO registering itself; no keys), keeps the tokens
//!   in Credential Manager, and adds the server to KIVO's MCP servers as `connector:<id>` (its
//!   tools wait for review like any server's). A custom connector is any remote MCP URL; one
//!   that needs no sign-in connects directly.
//! - **Local** connectors are found on this PC: the GitHub CLI signed in, an app installed, KIVO's
//!   browser extension. They need no account; a switch lets the user keep KIVO from using one.
//! - **Built-in** ones are always there.

use crate::core::Core;
use crate::mcp::Mcp;
use kivo_core::Event;
use kivo_core::event::{EventKind, SystemEvent};
use kivo_core::text;
use kivo_ipc::protocol::ConnectorView;
use kivo_mcp::config::{McpServer, Transport, server_id};
use kivo_mcp::connectors::{Connector, Kind, catalog, gh_account};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// What a local detector found: (found, detail).
type Found = HashMap<String, (bool, Option<String>)>;

/// Opens a page in the system browser.
pub type Opener = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

pub struct Connectors {
    core: Arc<Core>,
    mcp: Arc<Mcp>,
    commands: Option<Arc<dyn kivo_platform::CommandRunner>>,
    installed: Arc<dyn Fn() -> Vec<kivo_platform::AppEntry> + Send + Sync>,
    apps: Arc<kivo_tools::appreg::AppRegistry>,
    browser: Option<Arc<dyn kivo_tools::Browser>>,
    /// Opens the sign-in page in the system browser.
    open: Opener,
    found: Mutex<Found>,
    /// Sign-ins in progress.
    connecting: Mutex<Vec<String>>,
}

impl Connectors {
    #[allow(clippy::too_many_arguments, reason = "the connectors' sources")]
    pub fn new(
        core: Arc<Core>,
        mcp: Arc<Mcp>,
        commands: Option<Arc<dyn kivo_platform::CommandRunner>>,
        installed: Arc<dyn Fn() -> Vec<kivo_platform::AppEntry> + Send + Sync>,
        apps: Arc<kivo_tools::appreg::AppRegistry>,
        browser: Option<Arc<dyn kivo_tools::Browser>>,
        open: Opener,
    ) -> Arc<Self> {
        Arc::new(Self {
            core,
            mcp,
            commands,
            installed,
            apps,
            browser,
            open,
            found: Mutex::default(),
            connecting: Mutex::default(),
        })
    }

    fn changed(&self) {
        self.core.bus.publish(Event::new(EventKind::System(
            SystemEvent::DiscoveryChanged {
                section: "connectors".into(),
            },
        )));
    }

    /// Runs the local detectors again (DISC-08): `gh auth status`, the installed apps, the
    /// browser extension.
    pub async fn refresh(&self) {
        let mut found = Found::new();
        let installed = (self.installed)();
        for c in catalog().into_iter().filter(|c| c.kind == Kind::Local) {
            let result = match c.detect.as_deref() {
                Some("gh") => match self.gh().await {
                    Some(account) => (
                        true,
                        Some(text::tf("connectors.ghSignedIn", &[("account", &account)])),
                    ),
                    None => (false, None),
                },
                Some("app") => {
                    let entry = c.app.as_deref().and_then(|a| self.apps.get(a));
                    let hit = entry.is_some_and(|e| {
                        installed.iter().any(|i| {
                            e.is_named(&i.name)
                                || i.exe.as_deref().is_some_and(|x| {
                                    e.matches
                                        .exe
                                        .iter()
                                        .any(|m| x.to_lowercase().ends_with(&m.to_lowercase()))
                                })
                                || e.matches.aumid.iter().any(|a| i.id.eq_ignore_ascii_case(a))
                        })
                    });
                    (hit, hit.then(|| text::t("connectors.appFound")))
                }
                Some("browser") => {
                    let on = self.browser.as_ref().is_some_and(|b| b.connected());
                    (on, on.then(|| text::t("connectors.extensionOn")))
                }
                _ => (false, None),
            };
            found.insert(c.id, result);
        }
        *lock(&self.found) = found;
        self.changed();
    }

    /// The account `gh` is signed in to github.com with, if the GitHub CLI is here.
    async fn gh(&self) -> Option<String> {
        let runner = self.commands.clone()?;
        let spec = kivo_platform::CommandSpec {
            command: "gh auth status --hostname github.com".into(),
            shell: kivo_platform::ShellKind::Cmd,
            cwd: std::env::temp_dir(),
            timeout: Duration::from_secs(10),
            env: Vec::new(),
            max_memory_mb: 256,
            max_cpu_percent: 25,
        };
        let out = tokio::task::spawn_blocking(move || runner.run(&spec, &|| false))
            .await
            .ok()?
            .ok()?;
        gh_account(&format!("{}\n{}", out.stdout, out.stderr))
    }

    /// The directory as the Connectors page shows it.
    pub fn list(&self) -> Vec<ConnectorView> {
        let servers = self.mcp.views();
        let off = self.core.config().tools.connectors_off;
        let found = lock(&self.found).clone();
        let connecting = lock(&self.connecting).clone();
        let mut out: Vec<ConnectorView> = catalog()
            .into_iter()
            .map(|c| {
                let server = servers
                    .iter()
                    .find(|s| s.source.as_deref() == Some(&format!("connector:{}", c.id)));
                let (state, detail) = match c.kind {
                    Kind::Remote => match server {
                        _ if connecting.contains(&c.id) => ("connecting".to_owned(), None),
                        Some(s) if s.status == "error" => ("error".into(), s.error.clone()),
                        Some(_) => ("connected".into(), None),
                        None => ("available".into(), None),
                    },
                    Kind::Local => match found.get(&c.id) {
                        Some((true, detail)) if off.contains(&c.id) => {
                            ("off".into(), detail.clone())
                        }
                        Some((true, detail)) => ("ready".into(), detail.clone()),
                        _ => ("available".into(), None),
                    },
                    Kind::BuiltIn => ("ready".into(), None),
                };
                view(&c, state, detail, server)
            })
            .collect();
        // Custom connectors (a pasted URL).
        for s in servers.iter().filter(|s| {
            s.source
                .as_deref()
                .is_some_and(|x| x.starts_with("connector:custom"))
        }) {
            out.push(ConnectorView {
                id: s.id.clone(),
                name: s.name.clone(),
                description: text::t("connectors.custom"),
                access: text::t("connectors.customAccess"),
                kind: "remote".into(),
                state: if s.status == "error" {
                    "error"
                } else {
                    "connected"
                }
                .into(),
                detail: s.error.clone(),
                server: Some(s.id.clone()),
                tools: u32::try_from(s.tools.len()).unwrap_or(u32::MAX),
                badges: vec!["cloud".into()],
                last_used: None,
            });
        }
        out
    }

    /// Connects a directory connector, or a custom remote MCP (`url`), signing in in the browser
    /// when the server asks for it.
    pub async fn connect(
        self: &Arc<Self>,
        id: Option<&str>,
        custom: Option<(&str, &str)>,
    ) -> Result<ConnectorView, String> {
        let (key, name, url, source) = match (id, custom) {
            (Some(id), _) => {
                let c = catalog()
                    .into_iter()
                    .find(|c| c.id == id)
                    .ok_or_else(|| text::t("connectors.unknown"))?;
                let url = c
                    .url
                    .clone()
                    .ok_or_else(|| text::t("connectors.noSignIn"))?;
                (
                    c.id.clone(),
                    c.name.clone(),
                    url,
                    format!("connector:{}", c.id),
                )
            }
            (None, Some((url, name))) => {
                let parsed = url::Url::parse(url).map_err(|_| text::t("connectors.badUrl"))?;
                // Only HTTPS, except a server on this PC.
                let local = matches!(parsed.host_str(), Some("127.0.0.1" | "localhost"));
                if parsed.scheme() != "https" && !local {
                    return Err(text::t("connectors.badUrl"));
                }
                let name = if name.trim().is_empty() {
                    parsed.host_str().unwrap_or("custom").to_owned()
                } else {
                    name.trim().to_owned()
                };
                (
                    format!("custom-{}", server_id(&name)),
                    name,
                    url.to_owned(),
                    "connector:custom".to_owned(),
                )
            }
            _ => return Err(text::t("connectors.unknown")),
        };
        lock(&self.connecting).push(key.clone());
        self.changed();
        let result = self.sign_in_and_add(&key, &name, &url, &source).await;
        lock(&self.connecting).retain(|k| k != &key);
        self.changed();
        result?;
        self.list()
            .into_iter()
            .find(|v| v.id == key || v.server.as_deref() == Some(key.as_str()))
            .ok_or_else(|| text::t("connectors.unknown"))
    }

    async fn sign_in_and_add(
        &self,
        key: &str,
        name: &str,
        url: &str,
        source: &str,
    ) -> Result<(), String> {
        let mut server = McpServer {
            id: key.to_owned(),
            name: name.to_owned(),
            transport: Transport::Http {
                url: url.to_owned(),
                token: None,
                oauth: None,
            },
            enabled: true,
            source: Some(source.to_owned()),
            tools: std::collections::BTreeMap::new(),
        };
        // A server that needs no sign-in connects as it is.
        let vault = crate::mcp::SecretVault(self.mcp.secrets());
        let probe = kivo_mcp::Connection::connect(&server, &vault, &self.core.shutdown()).await;
        if let Ok(conn) = probe {
            conn.close().await;
        } else {
            let entry = format!("connector.{key}.oauth");
            let open = Arc::clone(&self.open);
            kivo_mcp::auth::sign_in(
                url,
                self.mcp.tokens(&entry),
                |u| open(u),
                &self.core.shutdown(),
            )
            .await
            .map_err(|e| text::tf("connectors.signInFailed", &[("reason", &e)]))?;
            if let Transport::Http { oauth, .. } = &mut server.transport {
                *oauth = Some(entry);
            }
        }
        // Replaces an earlier connection of the same connector.
        if self.mcp.server(key).is_some() {
            self.mcp.remove(key).await;
        }
        self.mcp.add(server, Vec::new()).await.map(drop)
    }

    /// Disconnects a remote connector (its tools go, its sign-in is forgotten) or switches a
    /// local one off.
    pub async fn disconnect(&self, id: &str) -> Result<(), String> {
        let is_local = catalog()
            .iter()
            .any(|c| c.id == id && c.kind == Kind::Local);
        if is_local {
            self.core.update_config(|c| {
                if !c.tools.connectors_off.iter().any(|x| x == id) {
                    c.tools.connectors_off.push(id.to_owned());
                }
            });
            self.changed();
            return Ok(());
        }
        let server = self
            .mcp
            .servers()
            .into_iter()
            .find(|s| s.id == id || s.source.as_deref() == Some(&format!("connector:{id}")))
            .ok_or_else(|| text::t("connectors.unknown"))?;
        self.mcp.remove(&server.id).await;
        self.changed();
        Ok(())
    }

    /// Switches a local connector back on.
    pub fn turn_on(&self, id: &str) {
        self.core
            .update_config(|c| c.tools.connectors_off.retain(|x| x != id));
        self.changed();
    }
}

fn view(
    c: &Connector,
    state: String,
    detail: Option<String>,
    server: Option<&kivo_ipc::protocol::McpServerView>,
) -> ConnectorView {
    ConnectorView {
        id: c.id.clone(),
        name: c.name.clone(),
        description: c.description.clone(),
        access: c.access.clone(),
        kind: match c.kind {
            Kind::Remote => "remote",
            Kind::Local => "local",
            Kind::BuiltIn => "builtIn",
        }
        .into(),
        state,
        detail,
        server: server.map(|s| s.id.clone()),
        tools: server.map_or(0, |s| u32::try_from(s.tools.len()).unwrap_or(u32::MAX)),
        badges: c.badges.clone(),
        last_used: None,
    }
}

/// The local connector an app belongs to, when the user switched it off: KIVO doesn't use that
/// app's commands or links (INT-03).
pub fn switched_off(config: &kivo_core::KivoConfig, app: &str) -> Option<String> {
    catalog()
        .into_iter()
        .find(|c| c.kind == Kind::Local && c.app.as_deref() == Some(app))
        .filter(|c| config.tools.connectors_off.contains(&c.id))
        .map(|c| c.name)
}
