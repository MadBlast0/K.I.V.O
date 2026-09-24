//! Installing what KIVO needs, only with the user's consent (DISCOVERY §1.1 DISC-07,
//! DISTRIBUTION §4 DIST-14): the CLI agents (Claude Code, Gemini CLI, Codex, OpenCode) and the
//! tools under them (Node.js, Git, Ollama).
//!
//! 1. **Detect** what's there (`--version`) and **explain** what will be installed and why, with
//!    the exact commands (`plan`).
//! 2. **Consent:** nothing runs until the user presses Install on that explanation.
//! 3. **Install** step by step in a visible progress sheet (each command's output kept), through
//!    winget for tools and npm for CLIs, inside a Job Object and never elevated. A tool winget
//!    can't install links to its vendor instead.
//! 4. **Verify** with `--version`; then the CLI's own sign-in and a test are offered.
//!
//! Commands come from the catalog (`kivo_brain::catalog::CLI_TOOLS`, `DEPENDENCIES` here), never
//! from a model or a page. Nothing is installed silently.

use crate::core::Core;
use kivo_core::event::{Event, EventKind, SystemEvent};
use kivo_core::text;
use kivo_ipc::protocol::{InstallPlan, InstallStepView, InstallView};
use kivo_platform::{CommandRunner, CommandSpec, ShellKind};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// A tool KIVO can install (DIST-14).
pub struct Dependency {
    pub id: &'static str,
    pub name: &'static str,
    /// Its command, for `--version`.
    pub command: &'static str,
    /// winget's package id, when winget has it.
    pub winget: Option<&'static str>,
    /// Where to get it otherwise.
    pub vendor: &'static str,
    /// Where it lands, for finding it before the PATH catches up.
    pub folders: &'static [&'static str],
}

pub const DEPENDENCIES: &[Dependency] = &[
    Dependency {
        id: "node",
        name: "Node.js",
        command: "node",
        winget: Some("OpenJS.NodeJS.LTS"),
        vendor: "https://nodejs.org/",
        folders: &[r"%ProgramFiles%\nodejs", r"%LOCALAPPDATA%\Programs\nodejs"],
    },
    Dependency {
        id: "git",
        name: "Git",
        command: "git",
        winget: Some("Git.Git"),
        vendor: "https://git-scm.com/download/win",
        folders: &[
            r"%ProgramFiles%\Git\cmd",
            r"%LOCALAPPDATA%\Programs\Git\cmd",
        ],
    },
    Dependency {
        id: "ollama",
        name: "Ollama",
        command: "ollama",
        winget: Some("Ollama.Ollama"),
        vendor: "https://ollama.com/download/windows",
        folders: &[r"%LOCALAPPDATA%\Programs\Ollama"],
    },
];

/// How long a step may take: winget and npm can download a lot.
const INSTALL_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const CHECK_TIMEOUT: Duration = Duration::from_secs(20);
/// Output lines kept per step for the sheet.
const KEPT_LINES: usize = 40;

pub fn dependency(id: &str) -> Option<&'static Dependency> {
    DEPENDENCIES.iter().find(|d| d.id == id)
}

/// The winget command for a dependency.
pub fn winget_command(id: &str) -> String {
    format!(
        "winget install --id {id} --exact --silent --accept-package-agreements --accept-source-agreements"
    )
}

/// The first version-looking token of `--version` output ("v22.9.0", "git version 2.46.0").
pub fn parse_version(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.'))
        .find(|w| {
            let v = w.trim_start_matches('v');
            v.contains('.') && v.chars().next().is_some_and(|c| c.is_ascii_digit())
        })
        .map(|w| w.trim_start_matches('v').to_owned())
}

/// One step of an install, before it runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub id: String,
    pub title: String,
    /// The exact command (empty for a step the user does: a vendor page).
    pub command: String,
    pub why: String,
}

/// What installing `id` takes on this PC: missing dependencies first, then the thing itself and
/// its adapter. `version` answers "is this installed, and which version".
pub fn plan(
    id: &str,
    version: &dyn Fn(&str) -> Option<String>,
) -> Result<(String, Vec<Step>), String> {
    plan_with(id, version, None)
}

/// `plan`, with a newer signed CLI catalog's commands when there is one (DISC-17).
pub fn plan_with(
    id: &str,
    version: &dyn Fn(&str) -> Option<String>,
    catalog: Option<&serde_json::Value>,
) -> Result<(String, Vec<Step>), String> {
    let mut steps = Vec::new();
    let mut need = |dep: &Dependency, why: String| {
        if version(dep.command).is_some() {
            return;
        }
        steps.push(match dep.winget {
            Some(pkg) => Step {
                id: dep.id.to_owned(),
                title: text::tf("install.step.tool", &[("name", &dep.name)]),
                command: winget_command(pkg),
                why,
            },
            None => Step {
                id: dep.id.to_owned(),
                title: text::tf("install.step.vendor", &[("name", &dep.name)]),
                command: String::new(),
                why: format!("{why} {}", dep.vendor),
            },
        });
    };
    if let Some(cli) = kivo_brain::catalog::cli_tool(id) {
        let name = kivo_brain::catalog::entry(id).map_or(id, |e| e.name);
        let (install, adapter) = crate::catalogs::cli_commands(catalog, id)
            .unwrap_or_else(|| (cli.install.to_owned(), cli.adapter_install.to_owned()));
        if install.starts_with("npm ") {
            let node = dependency("node").expect("node is a dependency");
            need(node, text::tf("install.why.node", &[("name", &name)]));
        }
        steps.push(Step {
            id: id.to_owned(),
            title: text::tf("install.step.cli", &[("name", &name)]),
            command: install,
            why: text::tf("install.why.cli", &[("name", &name)]),
        });
        if !adapter.is_empty() {
            steps.push(Step {
                id: format!("{id}-adapter"),
                title: text::tf("install.step.adapter", &[("name", &name)]),
                command: adapter,
                why: text::t("install.why.adapter"),
            });
        }
        return Ok((name.to_owned(), steps));
    }
    let dep = dependency(id).ok_or_else(|| text::tf("install.unknown", &[("name", &id)]))?;
    need(dep, text::tf("install.why.tool", &[("name", &dep.name)]));
    Ok((dep.name.to_owned(), steps))
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub struct Installer {
    core: Arc<Core>,
    commands: Arc<dyn CommandRunner>,
    /// Where commands run (the user's home).
    home: PathBuf,
    runs: Mutex<BTreeMap<String, InstallView>>,
    cancels: Mutex<BTreeMap<String, CancellationToken>>,
    /// Newer signed catalogs (DISC-17), when KIVO has them.
    catalogs: Mutex<Option<Arc<crate::catalogs::Catalogs>>>,
}

impl Installer {
    pub fn new(core: Arc<Core>, commands: Arc<dyn CommandRunner>, home: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            core,
            commands,
            home,
            runs: Mutex::default(),
            cancels: Mutex::default(),
            catalogs: Mutex::default(),
        })
    }

    pub fn set_catalogs(&self, catalogs: Arc<crate::catalogs::Catalogs>) {
        *lock(&self.catalogs) = Some(catalogs);
    }

    fn spec(&self, command: &str, timeout: Duration, path: Option<&str>) -> CommandSpec {
        CommandSpec {
            command: command.to_owned(),
            shell: ShellKind::Cmd,
            cwd: self.home.clone(),
            timeout,
            env: path
                .map(|p| vec![("PATH".to_owned(), p.to_owned())])
                .unwrap_or_default(),
            max_memory_mb: 4096,
            max_cpu_percent: 80,
        }
    }

    /// `<command> --version`, when the command is on this PC.
    pub fn version(&self, command: &str) -> Option<String> {
        let out = self
            .commands
            .run(
                &self.spec(&format!("{command} --version"), CHECK_TIMEOUT, None),
                &|| false,
            )
            .ok()?;
        (out.exit_code == Some(0))
            .then(|| parse_version(&format!("{}\n{}", out.stdout, out.stderr)))
            .flatten()
    }

    /// The explanation shown before anything runs (step 1–2).
    pub fn plan(&self, id: &str) -> Result<InstallPlan, String> {
        let cli = lock(&self.catalogs).as_ref().and_then(|c| c.get("cli"));
        let (name, steps) = plan_with(id, &|c| self.version(c), cli.as_ref())?;
        let installed = steps.is_empty();
        Ok(InstallPlan {
            id: id.to_owned(),
            name,
            installed,
            version: installed
                .then(|| {
                    kivo_brain::catalog::cli_tool(id)
                        .and_then(|c| c.commands.first().copied())
                        .or_else(|| dependency(id).map(|d| d.command))
                        .and_then(|c| self.version(c))
                })
                .flatten(),
            steps: steps
                .into_iter()
                .map(|s| InstallStepView {
                    id: s.id,
                    title: s.title,
                    command: s.command,
                    why: s.why,
                    status: "pending".into(),
                    output: String::new(),
                })
                .collect(),
        })
    }

    /// Where an install stands.
    pub fn status(&self, id: &str) -> Option<InstallView> {
        lock(&self.runs).get(id).cloned()
    }

    pub fn cancel(&self, id: &str) -> bool {
        lock(&self.cancels).get(id).is_some_and(|c| {
            c.cancel();
            true
        })
    }

    fn changed(&self) {
        self.core.bus.publish(Event::new(EventKind::System(
            SystemEvent::DiscoveryChanged {
                section: "installs".into(),
            },
        )));
    }

    fn update(&self, id: &str, f: impl FnOnce(&mut InstallView)) {
        if let Some(v) = lock(&self.runs).get_mut(id) {
            f(v);
        }
        self.changed();
    }

    /// Runs the plan the user agreed to (step 3–4), in the background. `accepted` is the plan's
    /// commands as shown; if the plan changed since (something got installed meanwhile), it runs
    /// the current one only when every command in it was among those shown.
    pub fn start(self: &Arc<Self>, id: &str, accepted: &[String]) -> Result<(), String> {
        if lock(&self.runs)
            .get(id)
            .is_some_and(|v| v.status == "running")
        {
            return Err(text::t("install.already"));
        }
        let plan = self.plan(id)?;
        if plan
            .steps
            .iter()
            .any(|s| !s.command.is_empty() && !accepted.contains(&s.command))
        {
            return Err(text::t("install.changed"));
        }
        let cancel = CancellationToken::new();
        lock(&self.cancels).insert(id.to_owned(), cancel.clone());
        lock(&self.runs).insert(
            id.to_owned(),
            InstallView {
                id: id.to_owned(),
                name: plan.name.clone(),
                status: "running".into(),
                steps: plan.steps.clone(),
                version: None,
                error: None,
            },
        );
        self.changed();
        let me = Arc::clone(self);
        let id = id.to_owned();
        std::thread::spawn(move || me.run(&id, &plan, &cancel));
        Ok(())
    }

    fn run(&self, id: &str, plan: &InstallPlan, cancel: &CancellationToken) {
        // Tools just installed aren't on this process's PATH yet: add their folders.
        let mut path = std::env::var("PATH").unwrap_or_default();
        for step in &plan.steps {
            if cancel.is_cancelled() {
                self.update(id, |v| {
                    v.status = "cancelled".into();
                });
                return;
            }
            self.update(id, |v| {
                if let Some(s) = v.steps.iter_mut().find(|s| s.id == step.id) {
                    s.status = "running".into();
                }
            });
            if step.command.is_empty() {
                // A vendor download: the user does it; KIVO can't go on without it.
                self.update(id, |v| {
                    if let Some(s) = v.steps.iter_mut().find(|s| s.id == step.id) {
                        s.status = "needsYou".into();
                    }
                    v.status = "needsYou".into();
                });
                return;
            }
            let out = self.commands.run(
                &self.spec(&step.command, INSTALL_TIMEOUT, Some(&path)),
                &|| cancel.is_cancelled(),
            );
            let (ok, output) = match out {
                Ok(o) => (
                    o.exit_code == Some(0) && !o.timed_out && !o.cancelled,
                    crate::checks::tail(&o.stdout, &o.stderr, KEPT_LINES),
                ),
                Err(e) => (false, e.to_string()),
            };
            self.update(id, |v| {
                if let Some(s) = v.steps.iter_mut().find(|s| s.id == step.id) {
                    s.status = if ok { "done" } else { "failed" }.into();
                    s.output.clone_from(&output);
                }
                if !ok {
                    v.status = "failed".into();
                    v.error = Some(text::tf("install.stepFailed", &[("step", &step.title)]));
                }
            });
            if !ok {
                return;
            }
            if let Some(dep) = dependency(&step.id) {
                for folder in dep.folders {
                    path = format!("{};{path}", expand(folder));
                }
            }
        }
        // Verify: the thing answers `--version`.
        let command = kivo_brain::catalog::cli_tool(id)
            .and_then(|c| c.commands.first().copied())
            .or_else(|| dependency(id).map(|d| d.command))
            .unwrap_or(id);
        let version = self
            .commands
            .run(
                &self.spec(&format!("{command} --version"), CHECK_TIMEOUT, Some(&path)),
                &|| false,
            )
            .ok()
            .filter(|o| o.exit_code == Some(0))
            .and_then(|o| parse_version(&format!("{}\n{}", o.stdout, o.stderr)));
        self.update(id, |v| match &version {
            Some(ver) => {
                v.status = "done".into();
                v.version = Some(ver.clone());
            }
            None => {
                v.status = "failed".into();
                v.error = Some(text::tf("install.notVerified", &[("name", &v.name)]));
            }
        });
        lock(&self.cancels).remove(id);
    }
}

/// `%NAME%` in a folder, from the environment.
fn expand(folder: &str) -> String {
    let mut out = folder.to_owned();
    for (name, value) in std::env::vars() {
        out = out.replace(&format!("%{name}%"), &value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_read_from_what_the_tools_print() {
        assert_eq!(parse_version("v22.9.0\n").as_deref(), Some("22.9.0"));
        assert_eq!(
            parse_version("git version 2.46.0.windows.1").as_deref(),
            Some("2.46.0.windows.1")
        );
        assert_eq!(
            parse_version("2.1.4 (Claude Code)").as_deref(),
            Some("2.1.4")
        );
        assert_eq!(parse_version("command not found"), None);
    }

    #[test]
    fn a_plan_installs_only_whats_missing_with_the_catalogs_commands() {
        // Nothing installed: Node.js first (npm needs it), then the CLI and its adapter.
        let (name, steps) = plan("claude-code", &|_| None).unwrap();
        assert_eq!(name, "Claude Code");
        assert_eq!(
            steps.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            ["node", "claude-code", "claude-code-adapter"]
        );
        assert!(
            steps[0]
                .command
                .starts_with("winget install --id OpenJS.NodeJS.LTS --exact")
        );
        assert_eq!(steps[1].command, "npm install -g @anthropic-ai/claude-code");
        // Node.js already here: not installed again.
        let (_, steps) = plan("gemini-cli", &|c| {
            (c == "node").then(|| "22.9.0".to_owned())
        })
        .unwrap();
        assert_eq!(steps.len(), 1);
        // A tool on its own; nothing when it's there.
        let (_, steps) = plan("git", &|_| None).unwrap();
        assert_eq!(steps[0].command, winget_command("Git.Git"));
        assert!(plan("git", &|_| Some("2.46".into())).unwrap().1.is_empty());
        assert!(plan("rm-rf", &|_| None).is_err());
    }
}
