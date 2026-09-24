//! The App Capability Registry (TOOLS_AND_CONTROL §2, TOOL-05): what KIVO knows about an app,
//! as data files (`apps/*.toml`). The core entries ship inside KIVO; users and plugins add their
//! own files (`%APPDATA%\KIVO\apps`), which win over a core entry with the same id.
//!
//! An entry says how to recognise the app (program name, AUMID), how to launch it, its CLI verbs
//! (with their risk), URI verbs, UIA hints (control names to AutomationIds or roles), which tiers
//! of the capability ladder to try first, and known quirks.

use kivo_core::tool::{CapabilityTier, Risk};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// The core registry, compiled in.
const CORE: &[(&str, &str)] = &[
    ("vscode", include_str!("../apps/vscode.toml")),
    (
        "windows-terminal",
        include_str!("../apps/windows-terminal.toml"),
    ),
    ("git", include_str!("../apps/git.toml")),
    ("github-cli", include_str!("../apps/github-cli.toml")),
    ("spotify", include_str!("../apps/spotify.toml")),
    ("chrome", include_str!("../apps/chrome.toml")),
    ("edge", include_str!("../apps/edge.toml")),
    ("brave", include_str!("../apps/brave.toml")),
    ("firefox", include_str!("../apps/firefox.toml")),
    ("explorer", include_str!("../apps/explorer.toml")),
    ("notepad", include_str!("../apps/notepad.toml")),
    ("settings", include_str!("../apps/settings.toml")),
    ("calculator", include_str!("../apps/calculator.toml")),
    ("claude-code", include_str!("../apps/claude-code.toml")),
    ("codex", include_str!("../apps/codex.toml")),
    ("gemini-cli", include_str!("../apps/gemini-cli.toml")),
    (
        "claude-desktop",
        include_str!("../apps/claude-desktop.toml"),
    ),
    ("chatgpt", include_str!("../apps/chatgpt.toml")),
    ("copilot", include_str!("../apps/copilot.toml")),
];

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Match {
    /// Program file names (`Code.exe`), matched case-insensitively.
    pub exe: Vec<String>,
    /// Packaged app ids.
    pub aumid: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CliVerb {
    /// Arguments with `{placeholders}` filled from the call.
    pub args: Vec<String>,
    #[serde(default = "low")]
    pub risk: Risk,
    #[serde(default)]
    pub description: String,
    /// A working folder (`{path}`), for tools that act on the current repository.
    #[serde(default)]
    pub cwd: Option<String>,
}

fn low() -> Risk {
    Risk::Low
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Cli {
    pub program: String,
    pub verbs: BTreeMap<String, CliVerb>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UriVerb {
    pub uri: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Uri {
    /// Protocols KIVO may open for this app (`spotify`).
    pub schemes: Vec<String>,
    pub verbs: BTreeMap<String, UriVerb>,
}

/// How to find a named control: an AutomationId, or a role (and name).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct Hint {
    pub automation_id: Option<String>,
    pub role: Option<String>,
    pub name: Option<String>,
}

/// Where a desktop AI app's composer, send button, latest reply and project picker are
/// (CONVERSATION §5.3), for the app version they were written against. Apps change their UI, so
/// the flow falls back when a hint stops matching.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AiHints {
    /// The app version the hints were written for.
    pub version: String,
    /// Whether they were checked against that version on a real install.
    pub verified: bool,
    pub composer: Hint,
    pub send: Option<Hint>,
    pub reply: Option<Hint>,
    pub project: Option<Hint>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Uia {
    pub hints: BTreeMap<String, Hint>,
}

/// A tier as registry files name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tier {
    Native,
    Uri,
    AppCli,
    Uia,
    BrowserDom,
    Vision,
    Input,
}

impl Tier {
    pub fn capability_tier(self) -> CapabilityTier {
        match self {
            Self::Native | Self::Uri => CapabilityTier::OsApi,
            Self::AppCli => CapabilityTier::AppCli,
            Self::Uia => CapabilityTier::Uia,
            Self::BrowserDom => CapabilityTier::BrowserDom,
            Self::Vision => CapabilityTier::Vision,
            Self::Input => CapabilityTier::Input,
        }
    }
}

/// How to start a CLI agent in a visible terminal (CONVERSATION §5.2, CONV-14): the command,
/// the flags for each mode, which modes skip the agent's own permission checks (always High risk),
/// and how to resume a session (CONV-13). Before using a flag, KIVO checks that the installed
/// version's `--help` lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AgentLaunch {
    pub command: String,
    /// Mode → extra arguments (`plan` → `["--permission-mode", "plan"]`).
    pub modes: BTreeMap<String, Vec<String>>,
    /// Modes that let the agent act without asking.
    pub bypass_modes: Vec<String>,
    /// Arguments that resume a session, with `{session}`.
    pub resume: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppEntry {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    /// Tiers to try first for this app (a per-app override of the ladder).
    pub prefer: Vec<Tier>,
    pub quirks: Vec<String>,
    /// A web browser (the extension and the UIA fallback apply).
    pub browser: bool,
    /// A command-line tool without a window.
    pub cli_only: bool,
    #[serde(rename = "match")]
    pub matches: Match,
    pub cli: Option<Cli>,
    pub uri: Option<Uri>,
    pub uia: Option<Uia>,
    /// "What can I say?" (UX-44): example requests while this app is in front.
    pub examples: Vec<String>,
    /// A CLI agent KIVO can start in a visible terminal (CONV-14).
    pub agent: Option<AgentLaunch>,
    /// A desktop AI app (DISC-06): found in the installed apps and listed on the Agents page.
    pub desktop_ai: bool,
    /// How to prompt it through UI Automation (CONV-16).
    pub ai: Option<AiHints>,
    /// Where the entry came from: `core` or the file it was read from.
    #[serde(skip)]
    pub origin: String,
}

impl AppEntry {
    /// Whether `exe` (a path or a file name) is this app's program.
    pub fn is_exe(&self, exe: &str) -> bool {
        // Windows paths, split by hand so this also works when the tests run elsewhere.
        let file = exe
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or_default()
            .to_lowercase();
        !file.is_empty() && self.matches.exe.iter().any(|e| e.to_lowercase() == file)
    }

    /// Whether the user's words name this app.
    pub fn is_named(&self, words: &str) -> bool {
        let w = words.trim().to_lowercase();
        self.id == w
            || self.name.to_lowercase() == w
            || self.aliases.iter().any(|a| a.to_lowercase() == w)
    }

    /// The ladder for this app: its preferred tiers first, then the default order.
    pub fn ladder(&self) -> Vec<Tier> {
        let mut out = self.prefer.clone();
        for t in DEFAULT_LADDER {
            if !out.contains(t) {
                out.push(*t);
            }
        }
        out
    }
}

/// Native/OS API → App CLI → UIA → Browser DOM → Vision → Input (TOOLS_AND_CONTROL §2).
pub const DEFAULT_LADDER: &[Tier] = &[
    Tier::Native,
    Tier::Uri,
    Tier::AppCli,
    Tier::Uia,
    Tier::BrowserDom,
    Tier::Vision,
    Tier::Input,
];

#[derive(Debug, thiserror::Error)]
#[error("{file}: {problem}")]
pub struct RegistryError {
    pub file: String,
    pub problem: String,
}

#[derive(Clone, Debug, Default)]
pub struct AppRegistry {
    entries: Vec<AppEntry>,
}

fn parse(origin: &str, source: &str) -> Result<AppEntry, RegistryError> {
    let mut entry: AppEntry = toml::from_str(source).map_err(|e| RegistryError {
        file: origin.to_owned(),
        problem: e.to_string(),
    })?;
    if entry.id.is_empty() || entry.name.is_empty() {
        return Err(RegistryError {
            file: origin.to_owned(),
            problem: "id and name are required".into(),
        });
    }
    entry.origin = origin.to_owned();
    Ok(entry)
}

impl AppRegistry {
    /// The core entries shipped with KIVO.
    pub fn core() -> Self {
        let entries = CORE
            .iter()
            .map(|(name, src)| {
                let mut e = parse(name, src).unwrap_or_else(|e| panic!("core app entry {e}"));
                e.origin = "core".into();
                e
            })
            .collect();
        Self { entries }
    }

    /// Adds the `*.toml` files in `dir` (users' and plugins' entries). A bad file is skipped and
    /// reported; it never stops KIVO.
    pub fn with_dir(mut self, dir: &Path) -> (Self, Vec<RegistryError>) {
        let mut errors = Vec::new();
        let Ok(files) = std::fs::read_dir(dir) else {
            return (self, errors);
        };
        let mut paths: Vec<_> = files
            .flatten()
            .map(|f| f.path())
            .filter(|p| p.extension().is_some_and(|e| e == "toml"))
            .collect();
        paths.sort();
        for path in paths {
            let origin = path.display().to_string();
            match std::fs::read_to_string(&path)
                .map_err(|e| RegistryError {
                    file: origin.clone(),
                    problem: e.to_string(),
                })
                .and_then(|s| parse(&origin, &s))
            {
                Ok(entry) => self.add(entry),
                Err(e) => errors.push(e),
            }
        }
        (self, errors)
    }

    /// Adds an entry, replacing one with the same id.
    pub fn add(&mut self, entry: AppEntry) {
        self.entries.retain(|e| e.id != entry.id);
        self.entries.push(entry);
    }

    pub fn entries(&self) -> &[AppEntry] {
        &self.entries
    }

    pub fn get(&self, id: &str) -> Option<&AppEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// The entry for a running window's program.
    pub fn find_by_exe(&self, exe: &str) -> Option<&AppEntry> {
        self.entries.iter().find(|e| e.is_exe(exe))
    }

    /// The entry the user's words name ("VS Code", "gh").
    pub fn find_by_name(&self, words: &str) -> Option<&AppEntry> {
        self.entries.iter().find(|e| e.is_named(words))
    }

    /// Whether a URI's scheme is one some entry declares (only those are opened).
    pub fn allows_uri(&self, uri: &str) -> Option<&AppEntry> {
        let scheme = uri.split_once(':')?.0.to_lowercase();
        self.entries.iter().find(|e| {
            e.uri
                .as_ref()
                .is_some_and(|u| u.schemes.iter().any(|s| s.to_lowercase() == scheme))
        })
    }
}

/// Fills `{name}` placeholders from `values`; `None` if one is missing.
pub fn fill(template: &str, values: &serde_json::Value) -> Option<String> {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let end = rest[start..].find('}')? + start;
        let key = &rest[start + 1..end];
        let value = match &values[key] {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Number(n) => n.to_string(),
            _ => return None,
        };
        out.push_str(&value);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_core_registry_loads_and_matches_programs_and_names() {
        let r = AppRegistry::core();
        assert!(r.entries().len() >= 13);
        assert_eq!(
            r.find_by_exe(r"C:\Program Files\Microsoft VS Code\Code.exe")
                .unwrap()
                .id,
            "vscode"
        );
        assert_eq!(r.find_by_name("vs code").unwrap().id, "vscode");
        assert_eq!(r.find_by_name("gh").unwrap().id, "github-cli");
        assert!(r.find_by_exe("msedge.exe").unwrap().browser);
        assert_eq!(r.allows_uri("spotify:search:jazz").unwrap().id, "spotify");
        assert!(r.allows_uri("javascript:alert(1)").is_none());
        assert!(r.allows_uri("file:///C:/x").is_none());
    }

    #[test]
    fn the_ladder_puts_an_apps_preferences_first() {
        let r = AppRegistry::core();
        let code = r.get("vscode").unwrap().ladder();
        assert_eq!(&code[..2], &[Tier::AppCli, Tier::Uia]);
        assert_eq!(code.len(), DEFAULT_LADDER.len());
        assert_eq!(AppEntry::default().ladder(), DEFAULT_LADDER);
    }

    #[test]
    fn user_entries_are_added_and_override_core_ones_and_bad_files_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("testapp.toml"),
            "id = \"kivo-test-app\"\nname = \"KIVO Test App\"\n[match]\nexe = [\"kivo-test-app.exe\"]\n[uia.hints]\nexport = { automationId = \"107\" }\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("vscode.toml"),
            "id = \"vscode\"\nname = \"My Code\"\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("broken.toml"), "id = [").unwrap();
        let (r, errors) = AppRegistry::core().with_dir(dir.path());
        assert_eq!(errors.len(), 1);
        let test = r.find_by_exe("kivo-test-app.exe").unwrap();
        assert_eq!(
            test.uia.as_ref().unwrap().hints["export"]
                .automation_id
                .as_deref(),
            Some("107")
        );
        assert_eq!(r.get("vscode").unwrap().name, "My Code");
    }

    #[test]
    fn placeholders_fill_or_refuse() {
        assert_eq!(
            fill("-g {path}:{line}", &json!({ "path": "a.rs", "line": 3 })).unwrap(),
            "-g a.rs:3"
        );
        assert!(fill("{path}", &json!({})).is_none());
    }
}
