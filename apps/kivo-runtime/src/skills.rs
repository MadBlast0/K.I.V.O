//! Agent Skills (CONVERSATION §9, CONV-32; DISCOVERY §1.2, DISC-10/16): `SKILL.md` folders — the
//! open standard Claude, Codex and Gemini use.
//!
//! - Found in KIVO's own `skills\` folder (the user's, on), in `~/.claude/skills/` and in the
//!   workspaces' `.claude/skills/` (referenced in place, off until the user reviews them).
//! - Only a skill's name and description go into a request (the context's skills layer); the body
//!   loads when the brain calls `skills.load`. Scripts in a skill run through the shell tool,
//!   under the permission engine, like any command.
//! - Import from a folder or a zip copies it into KIVO's folder as untrusted: it waits for review.
//! - The folders are watched (`notify`), so a new or changed skill shows up by itself.

use crate::core::Core;
use kivo_core::event::{EventKind, SystemEvent};
use kivo_core::text;
use kivo_core::tool::{
    CapabilityTier, Platform, Reversibility, Risk, SideEffect, ToolError, ToolErrorCode, ToolSpec,
};
use kivo_core::{Capability, Event};
use kivo_ipc::protocol::SkillView;
use kivo_store::Database;
use kivo_store::extensions::StoredSkill;
use kivo_tools::{Output, Tool};
use notify::{RecursiveMode, Watcher};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The most a skill's body may be (bigger ones are cut when loaded).
const MAX_BODY_CHARS: usize = 30_000;
/// Files that make a skill "run scripts".
const SCRIPT_EXTS: &[&str] = &["py", "ps1", "sh", "js", "ts", "bat", "cmd", "exe", "rb"];

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// What a `SKILL.md` says about itself: its front matter's `name` and `description`, and the
/// body after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parsed {
    pub name: String,
    pub description: String,
    pub body: String,
}

/// Reads a `SKILL.md`: YAML front matter between `---` lines (`name`, `description`; quoted or
/// plain, a folded `>`/`|` block too), then Markdown.
pub fn parse(text: &str) -> Option<Parsed> {
    let text = text.trim_start_matches('\u{feff}');
    let rest = text.strip_prefix("---")?;
    let end = rest.find("\n---")?;
    let front = &rest[..end];
    let body = rest[end + 4..].trim_start_matches(['\r', '\n']).to_owned();
    let mut name = None;
    let mut description = None;
    let lines: Vec<&str> = front.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if let Some((key, value)) = line.split_once(':')
            && !line.starts_with(' ')
        {
            let key = key.trim();
            let mut value = value.trim().to_owned();
            if value == ">" || value == "|" || value == ">-" || value == "|-" {
                let mut block = Vec::new();
                while i + 1 < lines.len() && lines[i + 1].starts_with(' ') {
                    i += 1;
                    block.push(lines[i].trim());
                }
                value = block.join(" ");
            }
            let value = value.trim_matches(['"', '\'']).to_owned();
            match key {
                "name" => name = Some(value),
                "description" => description = Some(value),
                _ => {}
            }
        }
        i += 1;
    }
    Some(Parsed {
        name: name.filter(|n| !n.is_empty())?,
        description: description.unwrap_or_default(),
        body,
    })
}

fn has_scripts(dir: &Path) -> bool {
    walk(dir, 3).iter().any(|p| {
        p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| SCRIPT_EXTS.contains(&e.to_ascii_lowercase().as_str()))
    })
}

/// Files under `dir`, `depth` levels deep at most.
fn walk(dir: &Path, depth: u32) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            if depth > 0 {
                out.extend(walk(&p, depth - 1));
            }
        } else {
            out.push(p);
        }
    }
    out
}

pub struct Skills {
    core: Arc<Core>,
    db: Arc<Mutex<Database>>,
    /// KIVO's own skills folder.
    own: PathBuf,
    /// The user's profile folder (for `~/.claude/skills`).
    home: PathBuf,
    projects: Mutex<Vec<PathBuf>>,
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
}

impl Skills {
    pub fn new(
        core: Arc<Core>,
        db: Arc<Mutex<Database>>,
        own: PathBuf,
        home: PathBuf,
    ) -> Arc<Self> {
        let _ = std::fs::create_dir_all(&own);
        Arc::new(Self {
            core,
            db,
            own,
            home,
            projects: Mutex::default(),
            watcher: Mutex::default(),
        })
    }

    pub fn set_projects(&self, projects: Vec<PathBuf>) {
        *lock(&self.projects) = projects;
    }

    /// The folders skills come from: (source, folder, trusted).
    fn sources(&self) -> Vec<(String, PathBuf, bool)> {
        let mut out = vec![
            ("kivo".to_owned(), self.own.clone(), true),
            (
                "claude-code".to_owned(),
                self.home.join(".claude").join("skills"),
                false,
            ),
        ];
        for p in lock(&self.projects).iter() {
            let label = p.file_name().map_or_else(
                || p.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            );
            out.push((
                format!("project:{label}"),
                p.join(".claude").join("skills"),
                false,
            ));
        }
        out
    }

    /// Looks at every source again and records what's there (DISC-10). A skill in KIVO's own
    /// folder is on from the start unless it was imported (then it waits for review, like a
    /// skill from another app).
    pub fn scan(&self) {
        let known: Vec<StoredSkill> = lock(&self.db).skills().unwrap_or_default();
        for (source, dir, trusted) in self.sources() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for e in entries.flatten() {
                let folder = e.path();
                let Ok(text) = std::fs::read_to_string(folder.join("SKILL.md")) else {
                    continue;
                };
                let Some(parsed) = parse(&text) else {
                    continue;
                };
                let id = format!("{source}:{}", parsed.name);
                let imported = folder.join(".kivo-imported").exists();
                let fresh = !known.iter().any(|k| k.id == id);
                let on = trusted && !imported;
                let _ = lock(&self.db).upsert_skill(&StoredSkill {
                    id,
                    name: parsed.name,
                    path: folder.display().to_string(),
                    source: if imported {
                        "import".into()
                    } else {
                        source.clone()
                    },
                    enabled: fresh && on,
                    reviewed: fresh && on,
                    updated_at: 0,
                });
            }
        }
        // Skills whose folder is gone are forgotten. (The list is read first: the lock must not
        // be held across the loop, or deleting would wait on it forever.)
        let stored = lock(&self.db).skills().unwrap_or_default();
        for k in stored {
            if !Path::new(&k.path).join("SKILL.md").is_file() {
                let _ = lock(&self.db).delete_skill(&k.id);
            }
        }
        self.core.bus.publish(Event::new(EventKind::System(
            SystemEvent::DiscoveryChanged {
                section: "skills".into(),
            },
        )));
    }

    /// Watches the skills folders; a change scans again (DISC-16).
    pub fn watch(self: &Arc<Self>) {
        let weak = Arc::downgrade(self);
        let handle = tokio::runtime::Handle::current();
        let Ok(mut watcher) =
            notify::recommended_watcher(move |r: notify::Result<notify::Event>| {
                if r.is_ok()
                    && let Some(this) = weak.upgrade()
                {
                    handle.spawn_blocking(move || this.scan());
                }
            })
        else {
            return;
        };
        for (_, dir, _) in self.sources() {
            if dir.is_dir() {
                let _ = watcher.watch(&dir, RecursiveMode::Recursive);
            }
        }
        *lock(&self.watcher) = Some(watcher);
    }

    pub fn list(&self) -> Vec<SkillView> {
        lock(&self.db)
            .skills()
            .unwrap_or_default()
            .into_iter()
            .map(|s| {
                let folder = PathBuf::from(&s.path);
                let parsed = std::fs::read_to_string(folder.join("SKILL.md"))
                    .ok()
                    .and_then(|t| parse(&t));
                let description = parsed.map(|p| p.description).unwrap_or_default();
                let tokens =
                    u32::try_from((s.name.len() + description.len()) / 4 + 4).unwrap_or(u32::MAX);
                SkillView {
                    scripts: has_scripts(&folder),
                    id: s.id,
                    name: s.name,
                    description,
                    source: s.source,
                    path: s.path,
                    enabled: s.enabled,
                    reviewed: s.reviewed,
                    tokens,
                }
            })
            .collect()
    }

    /// Switches a skill on (the user reviewed it) or off.
    pub fn enable(&self, id: &str, on: bool) -> Result<(), String> {
        if lock(&self.db)
            .set_skill_enabled(id, on)
            .map_err(|e| e.to_string())?
        {
            Ok(())
        } else {
            Err(text::t("skills.notFound"))
        }
    }

    /// What a skill contains, for review: its `SKILL.md` and its files.
    pub fn read(&self, id: &str) -> Result<Value, String> {
        let s = lock(&self.db)
            .skills()
            .unwrap_or_default()
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| text::t("skills.notFound"))?;
        let folder = PathBuf::from(&s.path);
        let text = std::fs::read_to_string(folder.join("SKILL.md")).map_err(|e| e.to_string())?;
        let files: Vec<String> = walk(&folder, 3)
            .into_iter()
            .filter_map(|p| {
                p.strip_prefix(&folder)
                    .ok()
                    .map(|r| r.display().to_string())
            })
            .filter(|p| p != ".kivo-imported")
            .collect();
        Ok(json!({ "text": text, "files": files }))
    }

    /// Removes a skill: one in KIVO's folder is deleted (to the recycle bin is the file tools'
    /// job; this is KIVO's own copy); one in another app's folder is only switched off.
    pub fn remove(&self, id: &str) -> Result<(), String> {
        let s = lock(&self.db)
            .skills()
            .unwrap_or_default()
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| text::t("skills.notFound"))?;
        let folder = PathBuf::from(&s.path);
        if folder.starts_with(&self.own) {
            std::fs::remove_dir_all(&folder).map_err(|e| e.to_string())?;
            lock(&self.db).delete_skill(id).map_err(|e| e.to_string())?;
        } else {
            self.enable(id, false)?;
        }
        Ok(())
    }

    /// Imports a skill folder or a `.zip` into KIVO's folder, marked for review (untrusted).
    pub fn import(&self, path: &Path) -> Result<String, String> {
        let staging = self
            .own
            .join(format!(".import-{}", kivo_store::brains::now_ms()));
        let result = (|| {
            if path.is_dir() {
                copy_dir(path, &staging)?;
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
            {
                unzip(path, &staging)?;
            } else {
                return Err(text::t("skills.notASkill"));
            }
            // The skill may sit one folder down in the archive.
            let root = if staging.join("SKILL.md").is_file() {
                staging.clone()
            } else {
                std::fs::read_dir(&staging)
                    .map_err(|e| e.to_string())?
                    .flatten()
                    .map(|e| e.path())
                    .find(|p| p.join("SKILL.md").is_file())
                    .ok_or_else(|| text::t("skills.notASkill"))?
            };
            let text = std::fs::read_to_string(root.join("SKILL.md")).map_err(|e| e.to_string())?;
            let parsed = parse(&text).ok_or_else(|| text::t("skills.notASkill"))?;
            let folder_name: String = parsed
                .name
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == '-' || c == '_' {
                        c
                    } else {
                        '-'
                    }
                })
                .collect();
            let dest = self.own.join(&folder_name);
            if dest.exists() {
                return Err(text::tf("skills.exists", &[("name", &parsed.name)]));
            }
            std::fs::rename(&root, &dest).map_err(|e| e.to_string())?;
            std::fs::write(dest.join(".kivo-imported"), b"").map_err(|e| e.to_string())?;
            Ok(parsed.name)
        })();
        let _ = std::fs::remove_dir_all(&staging);
        let name = result?;
        self.scan();
        Ok(name)
    }

    /// The skills layer of a request: the enabled skills' names and descriptions.
    pub fn index(&self) -> String {
        self.list()
            .into_iter()
            .filter(|s| s.enabled)
            .map(|s| format!("- {}: {}", s.name, s.description))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// An enabled skill's body, for `skills.load`.
    fn load(&self, name: &str) -> Option<(String, String)> {
        let s = lock(&self.db)
            .skills()
            .unwrap_or_default()
            .into_iter()
            .find(|s| s.enabled && s.name.eq_ignore_ascii_case(name))?;
        let text = std::fs::read_to_string(Path::new(&s.path).join("SKILL.md")).ok()?;
        let parsed = parse(&text)?;
        let (body, _) = kivo_mcp::client::clip(&parsed.body, MAX_BODY_CHARS);
        Some((body, s.path))
    }
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for e in std::fs::read_dir(from)
        .map_err(|e| e.to_string())?
        .flatten()
    {
        let p = e.path();
        let dest = to.join(e.file_name());
        if p.is_dir() {
            copy_dir(&p, &dest)?;
        } else {
            std::fs::copy(&p, &dest).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Extracts a zip, refusing entries that would land outside `to` ("zip slip").
fn unzip(zip: &Path, to: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = entry.enclosed_name() else {
            return Err(text::t("skills.badZip"));
        };
        let dest = to.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut out = std::fs::File::create(&dest).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// `skills.load`: a skill's instructions, when the brain decides to use it.
struct Load {
    spec: ToolSpec,
    skills: Arc<Skills>,
}

impl Tool for Load {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn run(&self, args: &Value) -> Result<Output, ToolError> {
        let name = args["name"].as_str().unwrap_or_default();
        let (body, folder) = self
            .skills
            .load(name)
            .ok_or_else(|| ToolError::new(ToolErrorCode::NotFound, text::t("skills.notFound")))?;
        Ok(Output::new(
            text::tf("skills.loaded", &[("name", &name)]),
            json!({ "name": name, "folder": folder, "instructions": body }),
        ))
    }
}

/// The skills tool.
pub fn tools(skills: &Arc<Skills>) -> Vec<Arc<dyn Tool>> {
    vec![Arc::new(Load {
        spec: ToolSpec {
            id: "skills.load".into(),
            description: "Load one of the user's skills (listed in the context by name and description) to follow its instructions. Scripts it mentions run with shell.run from its folder.".into(),
            title: text::t("tool.skills.load"),
            params: json!({ "type": "object", "properties": { "name": { "type": "string" } }, "required": ["name"] }),
            result: json!({ "type": "object" }),
            risk: Risk::Safe,
            side_effects: vec![SideEffect::LocalRead],
            data_egress: false,
            timeout_ms: 5_000,
            cancellable: false,
            tier: CapabilityTier::Native,
            platforms: vec![Platform::Windows, Platform::MacOs, Platform::Linux],
            reversibility: Reversibility::NotApplicable,
            capability: Capability::Memory,
        },
        skills: Arc::clone(skills),
    })]
}

#[cfg(test)]
mod tests {
    use super::*;

    const SKILL: &str = "---\nname: release-notes\ndescription: >\n  Turns commits into\n  release notes.\n---\n# Release notes\nRun `scripts/log.ps1`, then group by type.\n";

    #[test]
    fn skill_md_is_read() {
        let p = parse(SKILL).unwrap();
        assert_eq!(p.name, "release-notes");
        assert_eq!(p.description, "Turns commits into release notes.");
        assert!(p.body.starts_with("# Release notes"));
        let quoted = parse("---\nname: \"pdf\"\ndescription: 'Fill PDFs'\n---\nBody").unwrap();
        assert_eq!(
            (quoted.name.as_str(), quoted.description.as_str()),
            ("pdf", "Fill PDFs")
        );
        assert!(parse("no front matter").is_none());
        assert!(
            parse("---\ndescription: x\n---\n").is_none(),
            "a name is required"
        );
    }

    fn setup() -> (Arc<Skills>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let core = Arc::new(Core::with_config(kivo_core::KivoConfig::default(), None));
        let db = Arc::new(Mutex::new(Database::in_memory().unwrap()));
        let skills = Skills::new(
            core,
            db,
            dir.path().join("KIVO").join("skills"),
            dir.path().join("home"),
        );
        (skills, dir)
    }

    fn write_skill(folder: &Path, text: &str, script: bool) {
        std::fs::create_dir_all(folder.join("scripts")).unwrap();
        std::fs::write(folder.join("SKILL.md"), text).unwrap();
        if script {
            std::fs::write(folder.join("scripts").join("log.ps1"), "git log").unwrap();
        }
    }

    #[test]
    fn own_skills_are_on_and_others_wait_for_review() {
        let (skills, dir) = setup();
        write_skill(
            &dir.path().join("KIVO").join("skills").join("notes"),
            SKILL,
            true,
        );
        write_skill(
            &dir.path()
                .join("home")
                .join(".claude")
                .join("skills")
                .join("pdf"),
            "---\nname: pdf\ndescription: Fill PDFs\n---\nUse pdftk.",
            false,
        );
        skills.scan();
        let list = skills.list();
        assert_eq!(list.len(), 2);
        let own = list.iter().find(|s| s.name == "release-notes").unwrap();
        assert!(own.enabled && own.reviewed && own.scripts);
        let found = list.iter().find(|s| s.name == "pdf").unwrap();
        assert!(
            !found.enabled && !found.reviewed,
            "another app's skill waits for review"
        );
        assert_eq!(found.source, "claude-code");
        // Only enabled skills are in the index, and only their names and descriptions.
        assert_eq!(
            skills.index(),
            "- release-notes: Turns commits into release notes."
        );
        let tool = &tools(&skills)[0];
        assert!(
            tool.run(&json!({ "name": "pdf" })).is_err(),
            "not loaded before review"
        );
        skills.enable(&found.id, true).unwrap();
        let out = tool.run(&json!({ "name": "pdf" })).unwrap();
        assert_eq!(out.data["instructions"], "Use pdftk.");
        // Found again later: the decision stays.
        skills.scan();
        assert!(
            skills
                .list()
                .iter()
                .find(|s| s.name == "pdf")
                .unwrap()
                .enabled
        );
        // Removing another app's skill only switches it off; its files stay.
        skills.remove(&found.id).unwrap();
        assert!(
            dir.path()
                .join("home")
                .join(".claude")
                .join("skills")
                .join("pdf")
                .exists()
        );
    }

    #[test]
    fn imports_wait_for_review_and_zips_cant_escape() {
        let (skills, dir) = setup();
        let src = dir.path().join("download").join("tidy");
        write_skill(
            &src,
            "---\nname: tidy\ndescription: Tidies data\n---\nSteps.",
            true,
        );
        assert_eq!(skills.import(&src).unwrap(), "tidy");
        let s = skills
            .list()
            .into_iter()
            .find(|s| s.name == "tidy")
            .unwrap();
        assert_eq!(s.source, "import");
        assert!(!s.enabled && !s.reviewed, "untrusted until reviewed");
        let review = skills.read(&s.id).unwrap();
        assert!(review["text"].as_str().unwrap().contains("Tidies data"));
        assert!(
            review["files"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f.as_str().unwrap().contains("log.ps1"))
        );
        // Importing the same name again is refused.
        assert!(skills.import(&src).is_err());

        // A zip with a path that climbs out is refused.
        let zip_path = dir.path().join("evil.zip");
        {
            let f = std::fs::File::create(&zip_path).unwrap();
            let mut z = zip::ZipWriter::new(f);
            let opts = zip::write::SimpleFileOptions::default();
            z.start_file("../../escape.txt", opts).unwrap();
            std::io::Write::write_all(&mut z, b"x").unwrap();
            z.finish().unwrap();
        }
        assert!(skills.import(&zip_path).is_err());
        assert!(!dir.path().join("escape.txt").exists());
        // A good zip, with the skill one folder down.
        let good = dir.path().join("good.zip");
        {
            let f = std::fs::File::create(&good).unwrap();
            let mut z = zip::ZipWriter::new(f);
            let opts = zip::write::SimpleFileOptions::default();
            z.start_file("brief/SKILL.md", opts).unwrap();
            std::io::Write::write_all(
                &mut z,
                b"---\nname: brief\ndescription: Meeting brief\n---\nDo it.",
            )
            .unwrap();
            z.finish().unwrap();
        }
        assert_eq!(skills.import(&good).unwrap(), "brief");
    }
}
