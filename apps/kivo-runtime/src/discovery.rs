//! Discovery (DISCOVERY §1–3): what already exists on this PC — CLI agents and local model
//! servers — found by detectors in the runtime, on a low-priority task, cached in the store so
//! the Brains page shows results at once, and refreshed per section. Detection only *suggests*:
//! nothing is switched on, nothing is sent anywhere, and sign-in files are checked for existence
//! only, never read (DISC-03).

use crate::brains::{Brains, FoundAgent};
use kivo_brain::catalog::{self, CLI_TOOLS, CliTool};
use kivo_store::Database;
use kivo_store::brains::Discovered;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The Brains page's sections.
pub const CLI: &str = "cli";
pub const LOCAL: &str = "local";

/// Re-detect when a page opens and the results are older than this (DISC-15).
pub const STALE_AFTER: Duration = Duration::from_secs(600);
/// A version probe gives up after this long.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// One thing a detector found.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Found {
    pub id: String,
    pub data: Value,
}

/// A detector: one kind of thing to look for (DISC-01).
#[async_trait::async_trait]
pub trait Detector: Send + Sync {
    fn id(&self) -> &'static str;
    /// The section it belongs to (`cli`, `local`).
    fn scope(&self) -> &'static str;
    async fn run(&self) -> Vec<Found>;
}

/// Where to look for programs.
pub type PathSource = Arc<dyn Fn() -> Vec<PathBuf> + Send + Sync>;

/// CLI agents on `PATH` and in the usual install folders (DISC-04).
pub struct CliDetector {
    /// `PATH` as Windows has it now, plus [`install_folders`].
    pub path: PathSource,
    /// Where sign-in files are looked for.
    pub home: PathBuf,
}

/// Where CLIs are installed besides `PATH`: npm's global folder, the winget and Scoop shims,
/// pnpm, Bun and per-user bins.
pub fn install_folders(home: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(a) = std::env::var_os("APPDATA").map(PathBuf::from) {
        dirs.push(a.join("npm"));
    }
    if let Some(l) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        dirs.push(l.join("Microsoft").join("WinGet").join("Links"));
        dirs.push(l.join("pnpm"));
    }
    dirs.push(home.join("scoop").join("shims"));
    dirs.push(home.join(".local").join("bin"));
    dirs.push(home.join(".bun").join("bin"));
    dirs
}

impl CliDetector {
    /// The folders to search, each once.
    fn folders(&self) -> Vec<PathBuf> {
        let mut dirs = (self.path)();
        let mut seen = std::collections::HashSet::new();
        dirs.retain(|d| seen.insert(d.to_string_lossy().to_lowercase()));
        dirs
    }

    fn find(&self, folders: &[PathBuf], name: &str) -> Option<PathBuf> {
        let exts: &[&str] = if cfg!(windows) {
            &[".exe", ".cmd", ".bat", ""]
        } else {
            &[""]
        };
        folders.iter().find_map(|dir| {
            exts.iter()
                .map(|ext| dir.join(format!("{name}{ext}")))
                .find(|p| p.is_file())
        })
    }

    async fn detect(&self, tool: &CliTool, folders: &[PathBuf]) -> Option<Found> {
        let cli = tool.commands.iter().find_map(|c| self.find(folders, c));
        let adapter = tool.adapters.iter().find_map(|a| self.find(folders, a));
        if cli.is_none() && adapter.is_none() {
            return None;
        }
        let version = match &cli {
            Some(program) => version(program).await,
            None => None,
        };
        let signed_in = if tool.signed_in_files.is_empty() {
            None
        } else {
            Some(
                tool.signed_in_files
                    .iter()
                    .any(|f| self.home.join(f).is_file()),
            )
        };
        // The program KIVO starts in ACP mode: the adapter, or the CLI itself.
        let program = if tool.adapters.is_empty() {
            cli.clone()
        } else {
            adapter.clone()
        };
        let name = catalog::entry(tool.id).map_or(tool.id, |e| e.name);
        Some(Found {
            id: tool.id.to_owned(),
            data: json!({
                "name": name,
                "cli": cli,
                "program": program,
                "version": version,
                "signedIn": signed_in,
                // The CLI is here but its ACP adapter isn't: offer to install it.
                "needsAdapter": !tool.adapters.is_empty() && adapter.is_none(),
                "adapterInstall": tool.adapter_install,
                "free": catalog::entry(tool.id).and_then(|e| e.free),
            }),
        })
    }
}

#[async_trait::async_trait]
impl Detector for CliDetector {
    fn id(&self) -> &'static str {
        "cli-agents"
    }
    fn scope(&self) -> &'static str {
        CLI
    }
    async fn run(&self) -> Vec<Found> {
        let folders = self.folders();
        let mut out = Vec::new();
        for tool in CLI_TOOLS {
            if let Some(found) = self.detect(tool, &folders).await {
                out.push(found);
            }
        }
        out
    }
}

/// `program --version`, first line, or `None`.
async fn version(program: &Path) -> Option<String> {
    let is_script = program
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"));
    let mut cmd = if is_script {
        let mut c = tokio::process::Command::new("cmd");
        c.arg("/d").arg("/c").arg(program);
        c
    } else {
        tokio::process::Command::new(program)
    };
    cmd.arg("--version")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let out = tokio::time::timeout(PROBE_TIMEOUT, cmd.output())
        .await
        .ok()?
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(|l| l.chars().take(80).collect())
}

/// Model servers on this PC (DISC-05): Ollama, LM Studio, llama.cpp server.
pub struct LocalServerDetector {
    /// `(catalog id, base URL)`.
    pub servers: Vec<(String, String)>,
}

impl Default for LocalServerDetector {
    fn default() -> Self {
        Self {
            servers: ["ollama", "lmstudio", "llamacpp"]
                .iter()
                .filter_map(|id| catalog::entry(id))
                .map(|e| (e.id.to_owned(), e.base_url.to_owned()))
                .collect(),
        }
    }
}

#[async_trait::async_trait]
impl Detector for LocalServerDetector {
    fn id(&self) -> &'static str {
        "local-servers"
    }
    fn scope(&self) -> &'static str {
        LOCAL
    }
    async fn run(&self) -> Vec<Found> {
        let http = kivo_brain::http::Http::new();
        let cancel = tokio_util::sync::CancellationToken::new();
        let mut out = Vec::new();
        for (id, base) in &self.servers {
            let url = format!("{}/models", base.trim_end_matches('/'));
            // A closed port answers at once; a busy server gets a second.
            let Ok(Ok(body)) =
                tokio::time::timeout(Duration::from_secs(1), http.get_json(&url, &[], &cancel))
                    .await
            else {
                continue;
            };
            let models: Vec<String> = body["data"]
                .as_array()
                .or_else(|| body["models"].as_array())
                .into_iter()
                .flatten()
                .filter_map(|m| m["id"].as_str().or_else(|| m["name"].as_str()))
                .map(str::to_owned)
                .collect();
            let name = catalog::entry(id).map_or(id.as_str(), |e| e.name);
            out.push(Found {
                id: id.clone(),
                data: json!({ "name": name, "url": base, "models": models }),
            });
        }
        out
    }
}

/// Runs the detectors and keeps their results (DISC-02).
pub struct Discovery {
    db: Arc<Mutex<Database>>,
    detectors: Vec<Arc<dyn Detector>>,
    brains: Arc<Brains>,
    /// Sections being refreshed now, so a second Refresh doesn't run them twice.
    running: tokio::sync::Mutex<()>,
}

/// A section as the Brains page shows it: "Checked 2 min ago · Refresh" (DISC-12).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub section: String,
    pub items: Vec<Item>,
    /// When it was last checked (ms), if ever.
    pub checked_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub data: Value,
    /// Found since the user last looked (the "New" label).
    pub new: bool,
}

impl Discovery {
    pub fn new(
        db: Arc<Mutex<Database>>,
        detectors: Vec<Arc<dyn Detector>>,
        brains: Arc<Brains>,
    ) -> Arc<Self> {
        let discovery = Arc::new(Self {
            db,
            detectors,
            brains,
            running: tokio::sync::Mutex::new(()),
        });
        // What was found last time is known at once.
        discovery.apply();
        discovery
    }

    /// The cached results of `section`.
    pub fn section(&self, section: &str) -> Section {
        let (items, checked_at) = lock(&self.db).discovery(section).unwrap_or_default();
        Section {
            section: section.to_owned(),
            items: items
                .into_iter()
                .map(|d: Discovered| Item {
                    id: d.id,
                    data: d.data,
                    new: !d.viewed,
                })
                .collect(),
            checked_at,
        }
    }

    /// The user has seen the section: its New labels go (DISC-12).
    pub fn viewed(&self, section: &str) {
        let _ = lock(&self.db).mark_discovery_viewed(section);
    }

    /// Re-runs only `section`'s detectors (Refresh, DISC-12). Returns whether anything changed.
    pub async fn refresh(&self, section: &str) -> bool {
        let _one = self.running.lock().await;
        let before = self.section(section).items;
        let mut found = Vec::new();
        for d in self.detectors.iter().filter(|d| d.scope() == section) {
            found.extend(d.run().await);
        }
        let items: Vec<(String, Value)> = found.into_iter().map(|f| (f.id, f.data)).collect();
        if let Err(e) = lock(&self.db).save_discovery(section, &items) {
            tracing::warn!(%e, section, "couldn't save what discovery found");
        }
        self.apply();
        let after = self.section(section).items;
        before
            .iter()
            .map(|i| (&i.id, &i.data))
            .ne(after.iter().map(|i| (&i.id, &i.data)))
    }

    /// Every section.
    pub async fn refresh_all(&self) -> bool {
        let mut changed = false;
        for section in [CLI, LOCAL] {
            changed |= self.refresh(section).await;
        }
        changed
    }

    /// Refreshes `section` when its results are older than `max_age` (page open, DISC-15).
    pub async fn refresh_if_older(&self, section: &str, max_age: Duration) -> bool {
        let checked = self.section(section).checked_at;
        let age = checked.map(|t| kivo_store::brains::now_ms() - t);
        let old = age.is_none_or(|a| a >= i64::try_from(max_age.as_millis()).unwrap_or(i64::MAX));
        old && self.refresh(section).await
    }

    /// Tells the brains which CLI agents are here and how to start them.
    fn apply(&self) {
        let agents: BTreeMap<String, FoundAgent> = self
            .section(CLI)
            .items
            .into_iter()
            .filter_map(|i| {
                let program = i.data["program"].as_str()?;
                Some((
                    i.id,
                    FoundAgent {
                        program: PathBuf::from(program),
                        version: i.data["version"].as_str().map(str::to_owned),
                        signed_in: i.data["signedIn"].as_bool(),
                    },
                ))
            })
            .collect();
        self.brains.set_agents(agents);
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kivo_brain::testing::{MockServer, Reply};

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("kivo-discovery-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn hub(db: &Arc<Mutex<Database>>) -> Arc<Brains> {
        Arc::new(Brains::new(
            Arc::clone(db),
            Arc::new(kivo_testkit::FakeSecrets::default()),
            0,
        ))
    }

    #[tokio::test]
    async fn cli_agents_are_found_with_version_and_sign_in_state_without_reading_credentials() {
        let bin = temp("bin");
        let home = temp("home");
        // A fake `gemini` (speaks ACP itself) and a fake `claude` without its ACP adapter.
        std::fs::write(bin.join("gemini.cmd"), "@echo 0.9.1\r\n").unwrap();
        std::fs::write(bin.join("claude.cmd"), "@echo 2.1.0 (Claude Code)\r\n").unwrap();
        std::fs::create_dir_all(home.join(".gemini")).unwrap();
        // The file exists; its content is never read (it isn't even valid).
        std::fs::write(home.join(".gemini").join("oauth_creds.json"), "not json").unwrap();
        let path_dir = bin.clone();
        let detector = CliDetector {
            path: Arc::new(move || vec![path_dir.clone()]),
            home: home.clone(),
        };
        let found = detector.run().await;
        let gemini = found.iter().find(|f| f.id == "gemini-cli").expect("gemini");
        assert_eq!(gemini.data["version"], "0.9.1");
        assert_eq!(gemini.data["signedIn"], true);
        assert_eq!(gemini.data["needsAdapter"], false);
        assert!(
            gemini.data["free"].as_str().is_some(),
            "labelled free (CONV-08)"
        );
        let claude = found
            .iter()
            .find(|f| f.id == "claude-code")
            .expect("claude");
        assert_eq!(claude.data["signedIn"], false);
        assert_eq!(claude.data["needsAdapter"], true);
        assert!(claude.data["program"].is_null(), "no ACP program yet");
        assert!(!found.iter().any(|f| f.id == "codex"));

        // Discovery caches it and tells the brains; only gemini can be started.
        let db = Arc::new(Mutex::new(Database::in_memory().unwrap()));
        let brains = hub(&db);
        let discovery = Discovery::new(
            Arc::clone(&db),
            vec![Arc::new(detector)],
            Arc::clone(&brains),
        );
        assert!(discovery.section(CLI).checked_at.is_none());
        assert!(discovery.refresh(CLI).await, "something new");
        let section = discovery.section(CLI);
        assert!(section.checked_at.is_some());
        assert!(section.items.iter().all(|i| i.new));
        discovery.viewed(CLI);
        assert!(discovery.section(CLI).items.iter().all(|i| !i.new));
        assert!(brains.agent("gemini-cli").is_some());
        assert!(brains.agent("claude-code").is_none());
        assert!(!discovery.refresh(CLI).await, "nothing changed");
        // Not old enough to re-run on page open.
        assert!(!discovery.refresh_if_older(CLI, STALE_AFTER).await);
        let _ = std::fs::remove_dir_all(&bin);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn local_servers_are_found_with_their_models() {
        let ollama = MockServer::start(|r| {
            assert_eq!(r.path, "/v1/models");
            Reply::json(
                200,
                &json!({"object": "list", "data": [{"id": "llama3.2:3b"}, {"id": "qwen3:8b"}]}),
            )
        })
        .await;
        let detector = LocalServerDetector {
            servers: vec![
                ("ollama".into(), format!("{}/v1", ollama.url)),
                // Nothing listens here.
                ("lmstudio".into(), "http://127.0.0.1:9/v1".into()),
            ],
        };
        let found = detector.run().await;
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "ollama");
        assert_eq!(found[0].data["models"], json!(["llama3.2:3b", "qwen3:8b"]));
    }
}
