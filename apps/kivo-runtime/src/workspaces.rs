//! Instructions and workspaces (CONVERSATION §4, CONV-09/10/11).
//!
//! "About me" and each workspace's notes live in SQLite and are mirrored as Markdown in the
//! memory vault, where the user can edit them (`memory\about-me.md`,
//! `memory\workspaces\<name>\instructions.md`); an edited file is read back in. Workspaces are detected, not configured: a folder KIVO works in (a command's folder, an
//! agent's workspace, a git repository) leads to a one-time "Remember … as a workspace?". While
//! working in a workspace, the project's own agent files (`CLAUDE.md`, `AGENTS.md`, `GEMINI.md`)
//! are read, never written, unless the user asks to export KIVO's notes as `AGENTS.md`.

use crate::core::Core;
use kivo_core::text;
use kivo_ipc::protocol::{Offer, WorkspaceItem};
use kivo_store::Database;
use kivo_store::tasks::Workspace;
use notify::{RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The agent files a project may have, read in this order.
pub const AGENT_FILES: &[&str] = &["AGENTS.md", "CLAUDE.md", "GEMINI.md"];
/// The most of the project's agent files put into a request (CONV-11).
const AGENT_FILES_CHARS: usize = 3_000;
/// Markers of a project's root folder.
const ROOT_MARKERS: &[&str] = &[
    ".git",
    "Cargo.toml",
    "package.json",
    "pyproject.toml",
    "go.mod",
    ".sln",
    "pom.xml",
    "build.gradle",
];
const GLOBAL: &str = "global";

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The project folder `path` is in: the nearest folder up with a root marker (a git repository,
/// a Cargo or npm project), else the folder itself.
pub fn project_root(path: &Path) -> PathBuf {
    let start = if path.is_file() {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    let mut dir = Some(start);
    while let Some(d) = dir {
        let has_marker = ROOT_MARKERS.iter().any(|m| {
            if let Some(ext) = m.strip_prefix('.')
                && !m.contains('/')
                && *m != ".git"
            {
                std::fs::read_dir(d).ok().is_some_and(|mut it| {
                    it.any(|e| {
                        e.ok()
                            .is_some_and(|e| e.path().extension().is_some_and(|x| x == ext))
                    })
                })
            } else {
                d.join(m).exists()
            }
        });
        if has_marker {
            return d.to_path_buf();
        }
        dir = d.parent();
    }
    start.to_path_buf()
}

/// A `file:///d%3A/work/kivo` URI as a path (`d:\work\kivo`).
fn file_uri_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file:///")?;
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    let path = String::from_utf8(out).ok()?;
    Some(PathBuf::from(
        path.replace('/', std::path::MAIN_SEPARATOR_STR),
    ))
}

/// The folder open in the VS Code window with this title ("main.rs - kivo - Visual Studio
/// Code"), from VS Code's own window state (`storage.json`, read only): the open folder whose
/// name is one of the title's parts.
pub fn vscode_folder(title: &str, storage: &Path) -> Option<PathBuf> {
    let body = std::fs::read_to_string(storage).ok()?;
    let json: serde_json::Value = serde_json::from_str(&body).ok()?;
    let state = &json["windowsState"];
    let mut uris: Vec<&str> = Vec::new();
    uris.extend(state["lastActiveWindow"]["folder"].as_str());
    if let Some(opened) = state["openedWindows"].as_array() {
        uris.extend(opened.iter().filter_map(|w| w["folder"].as_str()));
    }
    let parts: Vec<String> = title
        .split([' ', '\u{2014}', '-'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_lowercase)
        .collect();
    let title_lower = title.to_lowercase();
    uris.into_iter().filter_map(file_uri_path).find(|p| {
        p.file_name().is_some_and(|n| {
            let name = n.to_string_lossy().to_lowercase();
            parts.contains(&name)
                || title_lower.contains(&format!(" - {name} - "))
                || title_lower.contains(&format!(" \u{2014} {name} \u{2014} "))
        })
    })
}

/// VS Code's window state file for this user.
fn vscode_storage() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(
        PathBuf::from(appdata)
            .join("Code")
            .join("User")
            .join("globalStorage")
            .join("storage.json"),
    )
}

/// A remembered workspace's folder by its name, ignoring case ("K.I.V.O").
pub fn folder_named(db: &Mutex<Database>, name: &str) -> Option<PathBuf> {
    let wanted = name.trim().to_lowercase();
    lock(db)
        .workspaces()
        .ok()?
        .into_iter()
        .find(|w| w.name.to_lowercase() == wanted)
        .map(|w| PathBuf::from(w.path))
}

/// A file-system-safe name for a workspace folder under `workspaces\`.
fn folder_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let mut collapsed = String::with_capacity(cleaned.len());
    for c in cleaned.chars() {
        if !(c == '-' && collapsed.ends_with('-')) {
            collapsed.push(c);
        }
    }
    let trimmed = collapsed.trim_matches(['-', '.']).to_owned();
    if trimmed.is_empty() {
        "workspace".into()
    } else {
        trimmed
    }
}

pub struct Workspaces {
    core: Arc<Core>,
    db: Arc<Mutex<Database>>,
    /// `%APPDATA%\KIVO`.
    dir: PathBuf,
    /// The workspace KIVO works in now.
    current: Mutex<Option<Workspace>>,
    /// Folders asked about this session (so one isn't asked twice before an answer).
    asked: Mutex<Vec<String>>,
    /// The file watcher, kept alive.
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
}

impl Workspaces {
    pub fn new(core: Arc<Core>, db: Arc<Mutex<Database>>, dir: PathBuf) -> Arc<Self> {
        let me = Arc::new(Self {
            core,
            db,
            dir,
            current: Mutex::new(None),
            asked: Mutex::default(),
            watcher: Mutex::new(None),
        });
        me.move_old_files();
        me.mirror_all();
        me
    }

    /// The memory vault, where the instruction files live (CONVERSATION §6).
    fn vault(&self) -> PathBuf {
        self.dir.join("memory")
    }

    /// A workspace's folder in the vault (the same one its notes use).
    fn workspace_dir(&self, name: &str) -> PathBuf {
        self.vault()
            .join("workspaces")
            .join(kivo_memory::vault::slug(name))
    }

    fn instructions_file(&self, scope: &str) -> Option<PathBuf> {
        if scope == GLOBAL {
            return Some(self.vault().join("about-me.md"));
        }
        let id = scope.strip_prefix("workspace:")?;
        let w = lock(&self.db).workspace(id).ok().flatten()?;
        Some(self.workspace_dir(&w.name).join("instructions.md"))
    }

    /// Before M7 the files lived in `instructions\` and `workspaces\` beside the vault: an edit
    /// made there while KIVO was closed is read in, then the old file goes (the vault has it).
    fn move_old_files(&self) {
        let old_global = self.dir.join("instructions").join("global.md");
        let mut moves = vec![(GLOBAL.to_owned(), old_global)];
        for w in lock(&self.db).workspaces().unwrap_or_default() {
            moves.push((
                format!("workspace:{}", w.id),
                self.dir
                    .join("workspaces")
                    .join(folder_name(&w.name))
                    .join("instructions.md"),
            ));
        }
        for (scope, file) in moves {
            let Ok(body) = std::fs::read_to_string(&file) else {
                continue;
            };
            if !body.trim().is_empty() && self.instructions(&scope) != body {
                let _ = lock(&self.db).set_instructions(&scope, &body);
            }
            let _ = std::fs::remove_file(&file);
            if let Some(parent) = file.parent() {
                let _ = std::fs::remove_dir(parent);
            }
        }
        let _ = std::fs::remove_dir(self.dir.join("instructions"));
        let _ = std::fs::remove_dir(self.dir.join("workspaces"));
    }

    /// Instructions for `global` or `workspace:<id>`.
    pub fn instructions(&self, scope: &str) -> String {
        lock(&self.db)
            .instructions(scope)
            .ok()
            .flatten()
            .map(|(t, _)| t)
            .unwrap_or_default()
    }

    /// Saves instructions and their Markdown mirror (CONV-09).
    pub fn set_instructions(&self, scope: &str, text: &str) -> Result<(), String> {
        if scope != GLOBAL && !scope.starts_with("workspace:") {
            return Err(text::t("workspace.badScope"));
        }
        lock(&self.db)
            .set_instructions(scope, text)
            .map_err(|e| e.to_string())?;
        self.mirror(scope, text);
        Ok(())
    }

    fn mirror(&self, scope: &str, body: &str) {
        let Some(file) = self.instructions_file(scope) else {
            return;
        };
        if std::fs::read_to_string(&file).is_ok_and(|t| t == body) {
            return;
        }
        if let Some(parent) = file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&file, body) {
            tracing::warn!(%e, "couldn't write the instructions file");
        }
    }

    /// Writes every stored text to its file (on start, and for files that went missing).
    fn mirror_all(&self) {
        let mut scopes = vec![GLOBAL.to_owned()];
        scopes.extend(
            lock(&self.db)
                .workspaces()
                .unwrap_or_default()
                .into_iter()
                .map(|w| format!("workspace:{}", w.id)),
        );
        for scope in scopes {
            let stored = lock(&self.db).instructions(&scope);
            if let Ok(Some((t, _))) = stored {
                self.mirror(&scope, &t);
            }
        }
    }

    /// Reads an edited Markdown file back in (CONV-09: the user can edit the files directly).
    pub fn file_changed(&self, file: &Path) {
        let Ok(raw) = std::fs::read_to_string(file) else {
            return;
        };
        // Front-matter added in Obsidian is the note's, not part of the instructions.
        let body = if raw.starts_with("---") {
            kivo_memory::Note::parse(&raw).body.trim_start().to_owned()
        } else {
            raw
        };
        let scope = if file == self.vault().join("about-me.md") {
            Some(GLOBAL.to_owned())
        } else {
            lock(&self.db)
                .workspaces()
                .unwrap_or_default()
                .into_iter()
                .find(|w| self.workspace_dir(&w.name).join("instructions.md") == file)
                .map(|w| format!("workspace:{}", w.id))
        };
        let Some(scope) = scope else { return };
        if self.instructions(&scope) != body {
            let _ = lock(&self.db).set_instructions(&scope, &body);
            tracing::info!(scope, "instructions edited outside KIVO; read back in");
        }
    }

    /// Watches the instruction files for edits, until KIVO quits.
    pub fn watch(self: &Arc<Self>) {
        let _ = std::fs::create_dir_all(self.vault());
        let me = Arc::downgrade(self);
        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            let Ok(event) = event else { return };
            if !matches!(
                event.kind,
                notify::EventKind::Modify(_) | notify::EventKind::Create(_)
            ) {
                return;
            }
            let Some(me) = me.upgrade() else { return };
            for path in &event.paths {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default();
                if name == "about-me.md" || name == "instructions.md" {
                    me.file_changed(path);
                }
            }
        });
        let Ok(mut watcher) = watcher else { return };
        let _ = watcher.watch(&self.vault(), RecursiveMode::Recursive);
        *lock(&self.watcher) = Some(watcher);
    }

    /// KIVO is working in `path` (a command's folder, an agent's workspace): its project becomes
    /// the current workspace, and a folder not seen before is offered once (CONV-10).
    pub fn worked_in(&self, path: &Path) {
        if !path.exists() {
            return;
        }
        let root = project_root(path);
        let key = root.display().to_string();
        let known = lock(&self.db).workspace_by_path(&key).ok().flatten();
        match known {
            Some(w) => {
                if w.remembered {
                    let _ = lock(&self.db).touch_workspace(&w.id);
                    *lock(&self.current) = Some(w);
                }
            }
            None => {
                if lock(&self.asked).contains(&key) {
                    return;
                }
                lock(&self.asked).push(key.clone());
                let name = root
                    .file_name()
                    .map_or_else(|| key.clone(), |n| n.to_string_lossy().into_owned());
                self.core.set_offer(Some(Offer {
                    id: format!("workspace:{key}"),
                    kind: "workspace".into(),
                    text: text::tf("workspace.remember", &[("name", &name)]),
                    accept: text::t("workspace.yes"),
                    decline: text::t("workspace.no"),
                }));
            }
        }
    }

    /// The window in front when a request starts: when it is VS Code, the folder it has open is
    /// a folder the user works in (CONV-10).
    pub fn front_window(&self, title: &str, app_id: &str) {
        let exe = Path::new(app_id)
            .file_stem()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if exe != "code" {
            return;
        }
        if let Some(folder) = vscode_storage().and_then(|s| vscode_folder(title, &s)) {
            self.worked_in(&folder);
        }
    }

    /// The answer to "Remember … as a workspace?".
    pub fn answer(&self, offer: &str, accept: bool) -> Result<Option<WorkspaceItem>, String> {
        let path = offer
            .strip_prefix("workspace:")
            .ok_or_else(|| text::t("workspace.badScope"))?;
        if self.core.offer().is_some_and(|o| o.id == offer) {
            self.core.set_offer(None);
        }
        let name = Path::new(path)
            .file_name()
            .map_or_else(|| path.to_owned(), |n| n.to_string_lossy().into_owned());
        let id = kivo_core::TaskId::new().to_string();
        let w = lock(&self.db)
            .remember_workspace(&id, path, &name, accept)
            .map_err(|e| e.to_string())?;
        if accept {
            *lock(&self.current) = Some(w.clone());
            self.mirror(&format!("workspace:{}", w.id), "");
            return Ok(Some(self.item(w)));
        }
        Ok(None)
    }

    pub fn current(&self) -> Option<Workspace> {
        lock(&self.current).clone()
    }

    pub fn set_current(&self, id: &str) -> Result<WorkspaceItem, String> {
        let w = lock(&self.db)
            .workspace(id)
            .ok()
            .flatten()
            .ok_or_else(|| text::t("workspace.notFound"))?;
        *lock(&self.current) = Some(w.clone());
        Ok(self.item(w))
    }

    pub fn forget(&self, id: &str) {
        let _ = lock(&self.db).forget_workspace(id);
        let mut current = lock(&self.current);
        if current.as_ref().is_some_and(|w| w.id == id) {
            *current = None;
        }
    }

    fn item(&self, w: Workspace) -> WorkspaceItem {
        let instructions = self.instructions(&format!("workspace:{}", w.id));
        let agent_files = AGENT_FILES
            .iter()
            .filter(|f| Path::new(&w.path).join(f).is_file())
            .map(|f| (*f).to_owned())
            .collect();
        WorkspaceItem {
            id: w.id,
            path: w.path,
            name: w.name,
            instructions,
            agent_files,
            preferred_agent: w.preferred_agent,
            last_used: w.last_used,
        }
    }

    pub fn list(&self) -> Vec<WorkspaceItem> {
        let rows = lock(&self.db).workspaces().unwrap_or_default();
        rows.into_iter().map(|w| self.item(w)).collect()
    }

    /// What a request in the current workspace starts with (CONV-05 layer 3, CONV-11): the
    /// workspace's notes and an excerpt of the project's own agent files, read-only.
    pub fn context(&self) -> String {
        let Some(w) = self.current() else {
            return String::new();
        };
        let mut out = text::tf("workspace.context", &[("name", &w.name), ("path", &w.path)]);
        let notes = self.instructions(&format!("workspace:{}", w.id));
        if !notes.trim().is_empty() {
            out.push('\n');
            out.push_str(notes.trim());
        }
        let mut left = AGENT_FILES_CHARS;
        for f in AGENT_FILES {
            let Ok(body) = std::fs::read_to_string(Path::new(&w.path).join(f)) else {
                continue;
            };
            if left == 0 {
                break;
            }
            let excerpt: String = body.chars().take(left).collect();
            left = left.saturating_sub(excerpt.chars().count());
            out.push_str(&format!("\n\n{f}:\n{excerpt}"));
        }
        out
    }

    /// "Export as AGENTS.md to project" (CONV-11): only when the user asks. KIVO's notes go into
    /// a section of their own; the rest of an existing file is kept as it was.
    pub fn export_agents_md(&self, id: &str) -> Result<PathBuf, String> {
        let w = lock(&self.db)
            .workspace(id)
            .ok()
            .flatten()
            .ok_or_else(|| text::t("workspace.notFound"))?;
        let notes = self.instructions(&format!("workspace:{id}"));
        if notes.trim().is_empty() {
            return Err(text::t("workspace.nothingToExport"));
        }
        let file = Path::new(&w.path).join("AGENTS.md");
        let section = format!("{}\n{}\n{}\n", SECTION_START, notes.trim(), SECTION_END);
        let body = match std::fs::read_to_string(&file) {
            Ok(existing) => match (existing.find(SECTION_START), existing.find(SECTION_END)) {
                (Some(a), Some(b)) if b > a => format!(
                    "{}{}{}",
                    &existing[..a],
                    section.trim_end(),
                    &existing[b + SECTION_END.len()..]
                ),
                _ => format!("{}\n\n{section}", existing.trim_end()),
            },
            Err(_) => section,
        };
        std::fs::write(&file, body).map_err(|e| e.to_string())?;
        Ok(file)
    }
}

const SECTION_START: &str = "<!-- KIVO workspace notes: start -->";
const SECTION_END: &str = "<!-- KIVO workspace notes: end -->";

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Arc<Workspaces>, Arc<Core>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let core = Arc::new(Core::with_config(kivo_core::KivoConfig::default(), None));
        let db = Arc::new(Mutex::new(Database::in_memory().unwrap()));
        let w = Workspaces::new(Arc::clone(&core), db, dir.path().join("KIVO"));
        (w, core, dir)
    }

    #[test]
    fn vs_codes_open_folder_is_found() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().join("storage.json");
        std::fs::write(
            &storage,
            r#"{"windowsState":{"lastActiveWindow":{"folder":"file:///d%3A/work/K.I.V.O"},
               "openedWindows":[{"folder":"file:///c%3A/Users/me/notes%20app"}]}}"#,
        )
        .unwrap();
        assert_eq!(
            vscode_folder("main.rs - K.I.V.O - Visual Studio Code", &storage),
            Some(PathBuf::from(r"d:\work\K.I.V.O"))
        );
        assert_eq!(
            vscode_folder("todo.md - notes app - Visual Studio Code", &storage),
            Some(PathBuf::from(r"c:\Users\me\notes app"))
        );
        assert_eq!(
            vscode_folder("Welcome - Visual Studio Code", &storage),
            None
        );
        assert_eq!(
            vscode_folder(
                "x - K.I.V.O - Visual Studio Code",
                &dir.path().join("none.json")
            ),
            None
        );
    }

    #[tokio::test]
    async fn a_new_project_is_offered_once_and_remembered() {
        let (w, core, dir) = setup();
        let project = dir.path().join("kivo");
        std::fs::create_dir_all(project.join("src")).unwrap();
        std::fs::write(project.join("Cargo.toml"), "[package]").unwrap();
        std::fs::write(project.join("CLAUDE.md"), "Tests: cargo nextest.").unwrap();
        w.worked_in(&project.join("src"));
        let offer = core.offer().expect("asked once");
        assert_eq!(offer.text, "Remember kivo as a workspace?");
        core.set_offer(None);
        w.worked_in(&project);
        assert!(core.offer().is_none(), "not asked twice");
        let item = w.answer(&offer.id, true).unwrap().unwrap();
        assert_eq!(item.agent_files, ["CLAUDE.md"]);
        w.set_instructions(&format!("workspace:{}", item.id), "Never touch /vendor.")
            .unwrap();
        let context = w.context();
        assert!(context.contains("Never touch /vendor."), "{context}");
        assert!(
            context.contains("CLAUDE.md:\nTests: cargo nextest."),
            "{context}"
        );
        // Read, never written.
        assert_eq!(
            std::fs::read_to_string(project.join("CLAUDE.md")).unwrap(),
            "Tests: cargo nextest."
        );
        // Declined folders aren't asked again either.
        let other = dir.path().join("other");
        std::fs::create_dir_all(&other).unwrap();
        w.worked_in(&other);
        let offer = core.offer().unwrap();
        w.answer(&offer.id, false).unwrap();
        assert_eq!(w.list().len(), 1);
    }

    #[tokio::test]
    async fn instructions_are_mirrored_and_edits_come_back() {
        let (w, _core, dir) = setup();
        w.set_instructions("global", "Call me Sam.").unwrap();
        let file = dir.path().join("KIVO").join("memory").join("about-me.md");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "Call me Sam.");
        std::fs::write(&file, "Call me Sam. Use metric.").unwrap();
        w.file_changed(&file);
        assert_eq!(w.instructions("global"), "Call me Sam. Use metric.");
        // Front-matter added in Obsidian isn't part of the instructions.
        std::fs::write(&file, "---\ntags: [me]\n---\nCall me Sam.\n").unwrap();
        w.file_changed(&file);
        assert_eq!(w.instructions("global"), "Call me Sam.\n");
        assert!(w.set_instructions("elsewhere", "x").is_err());
    }

    #[tokio::test]
    async fn files_from_before_the_vault_are_moved_in() {
        let dir = tempfile::tempdir().unwrap();
        let kivo = dir.path().join("KIVO");
        std::fs::create_dir_all(kivo.join("instructions")).unwrap();
        std::fs::write(
            kivo.join("instructions").join("global.md"),
            "Edited while closed.",
        )
        .unwrap();
        let core = Arc::new(Core::with_config(kivo_core::KivoConfig::default(), None));
        let db = Arc::new(Mutex::new(Database::in_memory().unwrap()));
        let w = Workspaces::new(core, db, kivo.clone());
        assert_eq!(w.instructions("global"), "Edited while closed.");
        assert!(!kivo.join("instructions").exists());
        assert_eq!(
            std::fs::read_to_string(kivo.join("memory").join("about-me.md")).unwrap(),
            "Edited while closed."
        );
    }

    #[tokio::test]
    async fn exporting_agents_md_keeps_the_rest_of_the_file() {
        let (w, core, dir) = setup();
        let project = dir.path().join("app");
        std::fs::create_dir_all(project.join(".git")).unwrap();
        std::fs::write(project.join("AGENTS.md"), "# App\nOwn notes.\n").unwrap();
        w.worked_in(&project);
        let offer = core.offer().unwrap();
        let item = w.answer(&offer.id, true).unwrap().unwrap();
        assert!(
            w.export_agents_md(&item.id).is_err(),
            "nothing to export yet"
        );
        w.set_instructions(&format!("workspace:{}", item.id), "Use pnpm.")
            .unwrap();
        w.export_agents_md(&item.id).unwrap();
        w.set_instructions(&format!("workspace:{}", item.id), "Use pnpm 9.")
            .unwrap();
        w.export_agents_md(&item.id).unwrap();
        let body = std::fs::read_to_string(project.join("AGENTS.md")).unwrap();
        assert!(body.starts_with("# App\nOwn notes."), "{body}");
        assert_eq!(body.matches(SECTION_START).count(), 1, "{body}");
        assert!(body.contains("Use pnpm 9.") && !body.contains("Use pnpm.\n"));
    }

    #[test]
    fn project_roots_are_found_by_their_markers() {
        let dir = tempfile::tempdir().unwrap();
        let deep = dir.path().join("repo").join("a").join("b");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::create_dir_all(dir.path().join("repo").join(".git")).unwrap();
        assert_eq!(project_root(&deep), dir.path().join("repo"));
        assert_eq!(folder_name("my: project?"), "my-project");
    }
}
