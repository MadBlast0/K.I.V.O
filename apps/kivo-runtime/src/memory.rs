//! The memory vault (CONVERSATION §6, MEMORY.md; MEM-04–08, CONV-17–20).
//!
//! What KIVO remembers is a folder of Markdown notes (`%APPDATA%\KIVO\memory\`) the user can open
//! in Obsidian; SQLite is only the index, rebuilt from the files (a changed file is re-indexed, a
//! deleted one forgotten), so the user's edits always win. This module keeps the two in step,
//! writes new memories ("remember …", accepted suggestions, workspace notes), picks the memories
//! for a request, and runs the tidy job that keeps the vault small.
//!
//! Rules that hold everywhere:
//! - Passwords, keys and tokens are never kept: a "remember" that contains one is refused, and the
//!   tidy job removes any that got into a note by hand.
//! - Guest turns read and write no memory (MEM-07).
//! - A note that is `sensitive` or above goes to a cloud brain only when Memory's "sensitive to
//!   cloud" setting allows it *and* the note itself allows it (`share_cloud: true`) (MEM-07).
//! - KIVO never records what the user does on the PC; memory comes only from requests, tasks and
//!   agent sessions KIVO took part in.

use crate::core::Core;
use kivo_core::config::{CaptureMode, NoteDetail};
use kivo_core::event::{Event, EventKind, SystemEvent};
use kivo_core::text;
use kivo_ipc::protocol::{
    MemoryFolderView, MemoryNoteDetail, MemoryNoteView, MemoryOverview, MemorySuggestionView,
    MemoryTagView, Offer,
};
use kivo_memory::graph;
use kivo_memory::note::{FrontMatter, Kind, Note};
use kivo_memory::query;
use kivo_memory::tidy::{self, Detail, Fact};
use kivo_memory::vault::{self, Vault};
use kivo_security::classify::{DataClass, classify};
use kivo_store::Database;
use kivo_store::memory::{GraphRows, MemoryRow};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The most a single remembered fact may hold.
const MAX_FACT_CHARS: usize = 2_000;
/// The most memory put into one request (MEM-08).
pub const MEMORY_TOKENS: usize = 400;
/// What an agent's handoff gets (CONVERSATION §8: ≤ 300 tokens of relevant memory).
pub const HANDOFF_TOKENS: usize = 300;
/// The most one memory takes of that.
const PER_ITEM_TOKENS: usize = 150;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn now_ms() -> i64 {
    kivo_store::brains::now_ms()
}

fn class_name(c: DataClass) -> String {
    serde_json::to_value(c)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "normal".into())
}

fn class_of(name: &str) -> DataClass {
    serde_json::from_value(serde_json::Value::String(name.to_owned())).unwrap_or(DataClass::Normal)
}

/// A title for a fact: its first sentence, cut at a word within 60 characters.
fn title_for(text: &str) -> String {
    // The first sentence ends at a line break, or at . ! ? followed by a space ("K.I.V.O uses
    // Rust" is one sentence).
    let text = text.trim();
    let chars: Vec<char> = text.chars().collect();
    let end = (0..chars.len())
        .find(|&i| {
            chars[i] == '\n'
                || (matches!(chars[i], '.' | '!' | '?')
                    && chars.get(i + 1).is_none_or(|c| c.is_whitespace()))
        })
        .unwrap_or(chars.len());
    let first: String = chars[..end].iter().collect();
    let first = first.trim();
    if first.chars().count() <= 60 {
        return first.to_owned();
    }
    let mut out = String::new();
    for w in first.split_whitespace() {
        if out.chars().count() + w.chars().count() + 1 > 57 {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(w);
    }
    format!("{out}…")
}

/// Where a request is being made, for retrieval.
#[derive(Clone, Debug, Default)]
pub struct Ask<'a> {
    pub text: &'a str,
    /// The app in front (`app:<id>` memories match it).
    pub app: Option<&'a str>,
    /// The current workspace's folder (`project:<path>` memories match it) and name.
    pub project: Option<&'a Path>,
    pub workspace: Option<&'a str>,
    /// The brain is a cloud one.
    pub cloud: bool,
    pub guest: bool,
}

/// A memory chosen for a request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recalled {
    pub path: String,
    pub title: String,
    pub text: String,
}

/// What "remember" did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Remembered {
    /// A new note, and the older facts it replaced.
    New {
        path: String,
        superseded: Vec<String>,
    },
    /// Already known (a near-duplicate): nothing new was written.
    Already { path: String },
}

/// What a tidy run did (shown in Activity).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TidyReport {
    pub merged: usize,
    pub superseded: usize,
    pub condensed_logs: usize,
    pub trimmed: usize,
    pub secrets_removed: usize,
}

impl TidyReport {
    pub fn is_empty(&self) -> bool {
        *self == TidyReport::default()
    }
}

pub struct Memory {
    core: Arc<Core>,
    db: Arc<Mutex<Database>>,
    vault: Vault,
    utc_offset: i32,
    /// One writer at a time (writes, sync and tidy).
    writing: Mutex<()>,
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
}

impl Memory {
    pub fn new(
        core: Arc<Core>,
        db: Arc<Mutex<Database>>,
        root: PathBuf,
        utc_offset: i32,
    ) -> Arc<Self> {
        let _ = std::fs::create_dir_all(&root);
        let me = Arc::new(Self {
            core,
            db,
            vault: Vault::new(root),
            utc_offset,
            writing: Mutex::new(()),
            watcher: Mutex::new(None),
        });
        me.sync();
        me
    }

    pub fn root(&self) -> &Path {
        self.vault.root()
    }

    fn today(&self) -> String {
        kivo_memory::date::format(now_ms(), self.utc_offset)
    }

    fn changed(&self) {
        self.core.bus.publish(Event::new(EventKind::System(
            SystemEvent::DiscoveryChanged {
                section: "memory".into(),
            },
        )));
    }

    // ---- The index follows the files -------------------------------------------------------

    /// Re-indexes the notes that changed on disk and forgets the ones that are gone. Returns how
    /// many changed.
    pub fn sync(&self) -> usize {
        let _one = lock(&self.writing);
        self.sync_locked()
    }

    fn sync_locked(&self) -> usize {
        let on_disk = self.vault.list();
        let indexed: BTreeMap<String, String> = lock(&self.db)
            .memory_hashes()
            .unwrap_or_default()
            .into_iter()
            .collect();
        let mut changed = 0;
        for e in &on_disk {
            if indexed.get(&e.path) != Some(&e.hash) {
                self.index(&e.path, &e.text, &e.hash);
                changed += 1;
            }
        }
        for path in indexed.keys() {
            if !on_disk.iter().any(|e| &e.path == path) {
                let _ = lock(&self.db).unindex_memory(path);
                changed += 1;
            }
        }
        if changed > 0 {
            tracing::debug!(changed, "memory index updated from the vault");
            self.changed();
        }
        changed
    }

    fn index(&self, path: &str, text: &str, hash: &str) {
        let note = Note::parse(text);
        let row = self.row(path, &note, hash);
        let c = graph::contribution(path, &note);
        let rows = GraphRows {
            entities: c.entities,
            relations: c.relations,
            observations: c
                .observations
                .into_iter()
                .map(|o| (o.entity, o.text))
                .collect(),
            links: graph::links(&note.body),
        };
        if let Err(e) = lock(&self.db).index_memory(&row, &rows) {
            tracing::warn!(%e, path, "couldn't index a memory note");
        }
    }

    fn row(&self, path: &str, note: &Note, hash: &str) -> MemoryRow {
        let kind = note.kind(path);
        let title = note.title(path);
        let text = note.text();
        let f = &note.front;
        let workspace = f.workspace.clone().or_else(|| {
            path.strip_prefix("workspaces/")
                .and_then(|r| r.split('/').next())
                .map(str::to_owned)
        });
        // The stricter of what the note says and what's in it; notes about the user and people
        // are personal at least.
        let declared = f.sensitivity.as_deref().map_or(DataClass::Normal, class_of);
        let floor = if matches!(kind, Kind::About | Kind::Person) {
            DataClass::Personal
        } else {
            DataClass::Normal
        };
        let found = classify(&format!("{title}\n{text}"));
        let sensitivity = declared.max(floor).max(found);
        let parse = |d: &Option<String>| {
            d.as_deref()
                .and_then(|d| kivo_memory::date::parse(d, self.utc_offset))
        };
        let created = parse(&f.created)
            .or_else(|| parse(&f.updated))
            .unwrap_or_else(now_ms);
        MemoryRow {
            path: path.to_owned(),
            kind: kind.as_str().to_owned(),
            title,
            text,
            tags: note.all_tags(),
            workspace,
            scope: f.scope.clone().unwrap_or_else(|| "global".into()),
            sensitivity: class_name(sensitivity),
            share_cloud: f.share_cloud,
            source: f.source.clone(),
            created_at: created,
            updated_at: parse(&f.updated).unwrap_or(created),
            valid_until: parse(&f.valid_until),
            last_used_at: None,
            use_count: 0,
            hash: hash.to_owned(),
        }
    }

    /// Writes a note and indexes it at once (the watcher would too, a moment later).
    fn put(&self, path: &str, note: &Note) -> Result<(), String> {
        let text = note.render();
        self.vault.write(path, &text).map_err(|e| e.to_string())?;
        self.index(path, &text, &vault::hash(&text));
        Ok(())
    }

    /// Watches the vault, so edits made in Obsidian or any editor are indexed (the files win).
    pub fn watch(self: &Arc<Self>) {
        use notify::{RecursiveMode, Watcher};
        let weak = Arc::downgrade(self);
        let handle = tokio::runtime::Handle::current();
        let Ok(mut watcher) =
            notify::recommended_watcher(move |r: notify::Result<notify::Event>| {
                let Ok(event) = r else { return };
                let Some(this) = weak.upgrade() else { return };
                // KIVO's own cache and Obsidian's settings aren't notes.
                let notes = event.paths.iter().any(|p| this.vault.relative(p).is_some());
                if notes {
                    handle.spawn_blocking(move || this.sync());
                }
            })
        else {
            return;
        };
        let _ = watcher.watch(self.vault.root(), RecursiveMode::Recursive);
        *lock(&self.watcher) = Some(watcher);
    }

    // ---- Remembering ------------------------------------------------------------------------

    fn current_facts(&self) -> Vec<MemoryRow> {
        let now = now_ms();
        lock(&self.db)
            .memories()
            .unwrap_or_default()
            .into_iter()
            .filter(|m| m.kind == "fact" && m.valid_until.is_none_or(|v| v > now))
            .collect()
    }

    /// "Remember this" on an answer (MEM-05): the answer KIVO gave in `turn`, kept as a fact.
    pub fn remember_turn(&self, turn: &str) -> Result<Remembered, String> {
        let answer = lock(&self.db)
            .turn_messages(turn)
            .map_err(|e| e.to_string())?
            .into_iter()
            .rev()
            .find(|m| m.role == "assistant")
            .map(|m| m.text)
            .ok_or_else(|| text::t("memory.noAnswer"))?;
        self.remember(&answer, &[], Some("user"), false)
    }

    /// "Remember …" (MEM-05): a fact note in `notes/`. A near-duplicate isn't written again; an
    /// older fact about the same thing is marked superseded (kept as history).
    pub fn remember(
        &self,
        text: &str,
        tags: &[String],
        source: Option<&str>,
        share_cloud: bool,
    ) -> Result<Remembered, String> {
        let text = text.trim();
        if text.is_empty() {
            return Err(text::t("memory.noText"));
        }
        if text.chars().count() > MAX_FACT_CHARS {
            return Err(text::t("memory.tooLong"));
        }
        let class = classify(text);
        if class == DataClass::Credential {
            return Err(text::t("memory.noSecrets"));
        }
        let _one = lock(&self.writing);
        let facts = self.current_facts();
        if let Some(same) = facts
            .iter()
            .find(|f| query::similarity(&f.text, text) >= tidy::DUPLICATE)
        {
            // Known already; new tags join it.
            let new_tags: Vec<&String> = tags.iter().filter(|t| !same.tags.contains(t)).collect();
            if !new_tags.is_empty()
                && let Ok(raw) = self.vault.read(&same.path)
            {
                let mut note = Note::parse(&raw);
                note.front.tags.extend(new_tags.into_iter().cloned());
                note.front.updated = Some(self.today());
                self.put(&same.path, &note)?;
            }
            return Ok(Remembered::Already {
                path: same.path.clone(),
            });
        }
        let title = title_for(text);
        let path = self.vault.free_path("notes", &title);
        let today = self.today();
        let note = Note {
            front: FrontMatter {
                kind: Some(Kind::Fact),
                tags: tags
                    .iter()
                    .map(|t| t.trim().trim_start_matches('#').to_lowercase())
                    .filter(|t| !t.is_empty())
                    .collect(),
                sensitivity: (class > DataClass::Normal).then(|| class_name(class)),
                created: Some(today.clone()),
                updated: Some(today.clone()),
                source: source.map(str::to_owned),
                share_cloud,
                ..FrontMatter::default()
            },
            body: format!("# {title}\n\n{text}\n"),
        };
        self.put(&path, &note)?;
        // Older facts about the same thing now hold only until today.
        let mut superseded = Vec::new();
        if let Some(subject) = query::subject(text) {
            for old in facts
                .iter()
                .filter(|f| query::subject(&f.text).as_deref() == Some(subject.as_str()))
            {
                if let Ok(raw) = self.vault.read(&old.path) {
                    let mut n = Note::parse(&raw);
                    tidy::supersede(&mut n, &path, &today);
                    self.put(&old.path, &n)?;
                    superseded.push(old.path.clone());
                }
            }
        }
        self.changed();
        Ok(Remembered::New { path, superseded })
    }

    /// Saves a note the user edited in the Memory page (the whole Markdown file).
    pub fn save(&self, path: &str, markdown: &str) -> Result<(), String> {
        if classify(markdown) == DataClass::Credential {
            return Err(text::t("memory.noSecrets"));
        }
        let _one = lock(&self.writing);
        let mut note = Note::parse(markdown);
        note.front.updated = Some(self.today());
        if note.front.created.is_none() {
            note.front.created = Some(self.today());
        }
        self.put(path, &note)?;
        self.changed();
        Ok(())
    }

    /// Changes a note's tags, sensitivity or cloud sharing.
    pub fn set_meta(
        &self,
        path: &str,
        tags: Option<Vec<String>>,
        sensitivity: Option<String>,
        share_cloud: Option<bool>,
    ) -> Result<(), String> {
        let _one = lock(&self.writing);
        let raw = self.vault.read(path).map_err(|e| e.to_string())?;
        let mut note = Note::parse(&raw);
        if let Some(t) = tags {
            note.front.tags = t;
        }
        if let Some(s) = sensitivity {
            note.front.sensitivity = Some(class_name(class_of(&s)));
        }
        if let Some(c) = share_cloud {
            note.front.share_cloud = c;
        }
        note.front.updated = Some(self.today());
        self.put(path, &note)?;
        self.changed();
        Ok(())
    }

    pub fn delete(&self, path: &str) -> Result<(), String> {
        let _one = lock(&self.writing);
        self.vault.delete(path).map_err(|e| e.to_string())?;
        let _ = lock(&self.db).unindex_memory(path);
        self.changed();
        Ok(())
    }

    /// Forget everything (MEM-06): every note and suggestion. Returns how many notes went.
    pub fn forget_everything(&self) -> usize {
        let _one = lock(&self.writing);
        let n = self.vault.forget_everything();
        let _ = lock(&self.db).forget_memories();
        self.changed();
        n
    }

    /// Everything as JSON (MEM-06): each note's path, front-matter fields and text.
    pub fn export(&self) -> serde_json::Value {
        let notes: Vec<serde_json::Value> = lock(&self.db)
            .memories()
            .unwrap_or_default()
            .into_iter()
            .map(|m| {
                let markdown = self.vault.read(&m.path).unwrap_or_default();
                serde_json::json!({
                    "path": m.path, "markdown": markdown, "type": m.kind, "title": m.title, "text": m.text,
                    "tags": m.tags, "workspace": m.workspace, "scope": m.scope,
                    "sensitivity": m.sensitivity, "shareCloud": m.share_cloud,
                    "source": m.source, "created": m.created_at, "updated": m.updated_at,
                    "validUntil": m.valid_until, "useCount": m.use_count,
                })
            })
            .collect();
        serde_json::json!({ "kivo": "memory", "version": 1, "notes": notes })
    }

    /// Restores notes from an export (Settings → Import): each note's Markdown at its path, only
    /// where no note is yet (this PC's notes win), never with a secret. Returns how many came in.
    pub fn import(&self, notes: &[serde_json::Value]) -> usize {
        let guard = lock(&self.writing);
        let mut n = 0;
        for note in notes {
            let (Some(path), Some(markdown)) = (note["path"].as_str(), note["markdown"].as_str())
            else {
                continue;
            };
            if markdown.trim().is_empty()
                || classify(markdown) == DataClass::Credential
                || self.vault.exists(path)
            {
                continue;
            }
            if self.vault.write(path, markdown).is_ok() {
                n += 1;
            }
        }
        drop(guard);
        self.sync();
        n
    }

    // ---- Suggestions (CONV-20) and workspace notes (CONV-19) ------------------------------

    /// KIVO thinks something is worth remembering. In Suggest mode it waits for the user (the
    /// Island asks when nothing else is showing); otherwise it's dropped. A suggestion already
    /// known, already waiting, or holding a secret is dropped too.
    pub fn suggest(
        &self,
        text: &str,
        reason: Option<&str>,
        source: Option<&str>,
        workspace: Option<&str>,
    ) -> Option<i64> {
        let config = self.core.config();
        if config.memory.capture != CaptureMode::Suggest {
            return None;
        }
        let text = text.trim();
        if text.is_empty()
            || text.chars().count() > MAX_FACT_CHARS
            || classify(text) == DataClass::Credential
        {
            return None;
        }
        if self
            .current_facts()
            .iter()
            .any(|f| query::similarity(&f.text, text) >= tidy::DUPLICATE)
        {
            return None;
        }
        let db = lock(&self.db);
        if db
            .memory_suggestions("pending")
            .unwrap_or_default()
            .iter()
            .chain(
                db.memory_suggestions("dismissed")
                    .unwrap_or_default()
                    .iter(),
            )
            .any(|s| query::similarity(&s.text, text) >= tidy::DUPLICATE)
        {
            return None;
        }
        let id = db
            .add_memory_suggestion(text, reason, source, workspace)
            .ok()?;
        drop(db);
        if self.core.offer().is_none() {
            self.core.set_offer(Some(Offer {
                id: format!("memory:{id}"),
                kind: "memory".into(),
                text: text::tf("memory.offer", &[("text", &text)]),
                accept: text::t("memory.offerYes"),
                decline: text::t("memory.offerNo"),
            }));
        }
        self.changed();
        Some(id)
    }

    /// Accepts (optionally edited) or dismisses a suggestion.
    pub fn answer(
        &self,
        id: i64,
        accept: bool,
        edited: Option<&str>,
    ) -> Result<Option<Remembered>, String> {
        let s = lock(&self.db)
            .memory_suggestion(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| text::t("memory.notFound"))?;
        if self
            .core
            .offer()
            .is_some_and(|o| o.id == format!("memory:{id}"))
        {
            self.core.set_offer(None);
        }
        let result = if accept {
            let text = edited
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .unwrap_or(&s.text);
            let tags: Vec<String> = s.workspace.iter().cloned().collect();
            Some(self.remember(text, &tags, s.source.as_deref(), false)?)
        } else {
            None
        };
        let _ =
            lock(&self.db).set_memory_suggestion(id, if accept { "accepted" } else { "dismissed" });
        self.changed();
        Ok(result)
    }

    /// Adds to today's session log of a workspace (Workspace notes, on by default): what was
    /// done, as bullets at the chosen detail. Creates the workspace's overview on first use.
    pub fn workspace_log(&self, workspace: &str, what: &str, bullets: &[String]) -> Option<String> {
        let config = self.core.config();
        if !config.memory.workspace_notes || bullets.is_empty() {
            return None;
        }
        // Never keep a secret in a note.
        let bullets: Vec<String> = bullets
            .iter()
            .filter(|b| classify(b) != DataClass::Credential)
            .map(|b| b.lines().next().unwrap_or_default().trim().to_owned())
            .filter(|b| !b.is_empty())
            .collect();
        if bullets.is_empty() {
            return None;
        }
        let _one = lock(&self.writing);
        let folder = format!("workspaces/{}", vault::slug(workspace));
        let today = self.today();
        let overview = format!("{folder}/overview.md");
        if !self.vault.exists(&overview) {
            let note =
                tidy::workspace_note(Kind::Workspace, &vault::slug(workspace), workspace, &today);
            let _ = self.put(&overview, &note);
        }
        let path = format!("{folder}/log/{today}.md");
        let mut note = self
            .vault
            .read(&path)
            .map(|t| Note::parse(&t))
            .unwrap_or_else(|_| {
                let mut n =
                    tidy::workspace_note(Kind::Log, &vault::slug(workspace), &today, &today);
                n.front.tags = vec!["log".into()];
                n
            });
        let now = now_ms() + i64::from(self.utc_offset) * 60_000;
        let minutes = now.rem_euclid(86_400_000) / 60_000;
        let time = format!("{:02}:{:02}", minutes / 60, minutes % 60);
        let detail = match config.memory.detail {
            NoteDetail::Brief => Detail::Brief,
            NoteDetail::Standard => Detail::Standard,
            NoteDetail::Detailed => Detail::Detailed,
        };
        note.body
            .push_str(&tidy::log_entry(&time, what, &bullets, detail));
        note.front.updated = Some(today);
        self.put(&path, &note).ok()?;
        self.changed();
        Some(path)
    }

    // ---- Retrieval (MEM-07, MEM-08) ---------------------------------------------------------

    /// The memories for a request: full-text search plus the scope filter, current facts only,
    /// the privacy rules, at most `max-items` and ~400 tokens. Their use is counted.
    pub fn recall(&self, ask: &Ask<'_>, budget_tokens: usize) -> Vec<Recalled> {
        if ask.guest {
            return Vec::new();
        }
        let Some(fts) = query::fts_query(ask.text) else {
            return Vec::new();
        };
        let config = self.core.config();
        let now = now_ms();
        let found = lock(&self.db).search_memories(&fts, 30).unwrap_or_default();
        let mut out = Vec::new();
        let mut used = 0;
        for m in found {
            // About me and instructions are their own layers; logs are condensed later.
            if matches!(m.kind.as_str(), "about" | "instructions") {
                continue;
            }
            if m.valid_until.is_some_and(|v| v <= now) {
                continue;
            }
            let in_scope = match m.scope.split_once(':') {
                None => true,
                Some(("app", id)) => ask.app.is_some_and(|a| a.eq_ignore_ascii_case(id)),
                Some(("project", p)) => ask.project.is_some_and(|w| {
                    w.to_string_lossy()
                        .to_lowercase()
                        .starts_with(&p.to_lowercase())
                }),
                Some(_) => true,
            };
            let in_workspace = m.workspace.as_deref().is_none_or(|w| {
                ask.workspace
                    .is_some_and(|mine| vault::slug(mine) == vault::slug(w))
                    || m.kind != "log"
            });
            if !in_scope || !in_workspace {
                continue;
            }
            let class = class_of(&m.sensitivity);
            if class >= DataClass::Credential {
                continue;
            }
            if ask.cloud
                && !class.cloud_ok()
                && !(config.memory.sensitive_to_cloud && m.share_cloud)
            {
                continue;
            }
            let text = if m.kind == "fact" {
                m.text.clone()
            } else {
                format!("{}: {}", m.title, m.text)
            };
            // A long note is cut to its first ~150 tokens; one that still doesn't fit what's left
            // of the budget is skipped (a fragment would mislead more than help).
            let text: String = text.chars().take(PER_ITEM_TOKENS * 4).collect();
            let cost = query::tokens(&text);
            if used + cost > budget_tokens {
                continue;
            }
            used += cost;
            out.push(Recalled {
                path: m.path,
                title: m.title,
                text,
            });
            if out.len() >= config.memory.max_items as usize {
                break;
            }
        }
        if !out.is_empty() {
            let paths: Vec<String> = out.iter().map(|r| r.path.clone()).collect();
            let _ = lock(&self.db).mark_memories_used(&paths, now);
        }
        out
    }

    /// Notes which memories a turn used ("Why did you say that?", MEM-10).
    pub fn record_used(&self, turn_id: &str, used: &[Recalled]) {
        if used.is_empty() {
            return;
        }
        let rows: Vec<(String, String)> = used
            .iter()
            .map(|r| (r.path.clone(), r.title.clone()))
            .collect();
        let _ = lock(&self.db).record_turn_memories(turn_id, &rows);
    }

    pub fn used_by(&self, turn_id: &str) -> Vec<(String, String)> {
        lock(&self.db).turn_memories(turn_id).unwrap_or_default()
    }

    // ---- Tidy ------------------------------------------------------------------------------

    /// The tidy job (CONVERSATION §6 "Staying small"): merge near-duplicates, mark superseded
    /// facts, condense old workspace logs (harder over the size cap), and remove secrets that got
    /// into notes.
    pub fn tidy(&self) -> TidyReport {
        let config = self.core.config();
        let guard = lock(&self.writing);
        let mut report = TidyReport::default();
        let today = self.today();
        let now = now_ms();

        // Secrets never stay in a note.
        for e in self.vault.list() {
            if classify(&e.text) == DataClass::Credential {
                let cleaned = remove_secrets(&e.text);
                if cleaned != e.text && self.vault.write(&e.path, &cleaned).is_ok() {
                    report.secrets_removed += 1;
                }
            }
        }

        let facts: Vec<Fact> = self
            .current_facts()
            .into_iter()
            .map(|m| Fact {
                path: m.path,
                text: m.text,
                created: m.created_at,
                current: true,
            })
            .collect();
        if config.memory.merge_duplicates {
            for (keep, gone) in tidy::duplicates(&facts) {
                let (Ok(a), Ok(b)) = (self.vault.read(&keep), self.vault.read(&gone)) else {
                    continue;
                };
                let mut into = Note::parse(&a);
                tidy::merge(&mut into, &Note::parse(&b));
                into.front.updated = Some(today.clone());
                if self.vault.write(&keep, &into.render()).is_ok()
                    && self.vault.delete(&gone).is_ok()
                {
                    report.merged += 1;
                }
            }
        }
        let facts: Vec<Fact> = facts
            .into_iter()
            .filter(|f| self.vault.exists(&f.path))
            .collect();
        for (old, new) in tidy::superseded(&facts) {
            if let Ok(raw) = self.vault.read(&old) {
                let mut n = Note::parse(&raw);
                tidy::supersede(&mut n, &new, &today);
                if self.vault.write(&old, &n.render()).is_ok() {
                    report.superseded += 1;
                }
            }
        }

        if config.memory.condense_logs {
            let (logs, trimmed) = self.condense_workspaces(&today, now);
            report.condensed_logs += logs;
            report.trimmed += trimmed;
        }
        drop(guard);
        self.sync();
        if !report.is_empty() {
            tracing::info!(?report, "memory tidied");
        }
        report
    }

    fn condense_workspaces(&self, today: &str, now: i64) -> (usize, usize) {
        let mut logs_done = 0;
        let mut trimmed = 0;
        let entries = self.vault.list();
        let mut workspaces: Vec<String> = entries
            .iter()
            .filter_map(|e| e.path.strip_prefix("workspaces/"))
            .filter_map(|r| r.split('/').next())
            .map(str::to_owned)
            .collect();
        workspaces.sort();
        workspaces.dedup();
        for w in workspaces {
            let prefix = format!("workspaces/{w}/");
            let size: usize = entries
                .iter()
                .filter(|e| e.path.starts_with(&prefix))
                .map(|e| e.text.len())
                .sum();
            let over = size > tidy::WORKSPACE_CAP_BYTES;
            let after = if over {
                tidy::CONDENSE_HARD_AFTER_DAYS
            } else {
                tidy::CONDENSE_AFTER_DAYS
            };
            let old_logs: Vec<(String, String, Note)> = entries
                .iter()
                .filter(|e| e.path.starts_with(&format!("{prefix}log/")))
                .filter_map(|e| {
                    let date = e.path.rsplit('/').next()?.strip_suffix(".md")?.to_owned();
                    let at = kivo_memory::date::parse(&date, self.utc_offset)?;
                    (kivo_memory::date::days_between(at, now, self.utc_offset) > after)
                        .then(|| (e.path.clone(), date, Note::parse(&e.text)))
                })
                .collect();
            if old_logs.is_empty() && !over {
                continue;
            }
            let overview_path = format!("{prefix}overview.md");
            let decisions_path = format!("{prefix}decisions.md");
            let mut overview = self
                .vault
                .read(&overview_path)
                .map(|t| Note::parse(&t))
                .unwrap_or_else(|_| tidy::workspace_note(Kind::Workspace, &w, &w, today));
            let mut decisions = self
                .vault
                .read(&decisions_path)
                .map(|t| Note::parse(&t))
                .unwrap_or_else(|_| {
                    tidy::workspace_note(Kind::Decisions, &w, &text::t("memory.decisions"), today)
                });
            let dated: Vec<(String, Note)> = old_logs
                .iter()
                .map(|(_, d, n)| (d.clone(), n.clone()))
                .collect();
            let (_, decided) = tidy::condense(&mut overview, &mut decisions, &dated, today);
            if over {
                trimmed += tidy::trim_earlier(&mut overview);
            }
            let _ = self.vault.write(&overview_path, &overview.render());
            if decided > 0 || self.vault.exists(&decisions_path) {
                let _ = self.vault.write(&decisions_path, &decisions.render());
            }
            for (path, _, _) in &old_logs {
                let archived = format!("archive/{path}");
                if self.vault.rename(path, &archived).is_ok() {
                    logs_done += 1;
                }
            }
        }
        (logs_done, trimmed)
    }

    // ---- The Memory page -------------------------------------------------------------------

    fn view(m: &MemoryRow) -> MemoryNoteView {
        let excerpt: String = m
            .text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(200)
            .collect();
        let folder = m
            .path
            .rsplit_once('/')
            .map_or(String::new(), |(f, _)| f.to_owned());
        MemoryNoteView {
            path: m.path.clone(),
            kind: m.kind.clone(),
            title: m.title.clone(),
            excerpt,
            tags: m.tags.clone(),
            folder,
            workspace: m.workspace.clone(),
            sensitivity: m.sensitivity.clone(),
            share_cloud: m.share_cloud,
            updated_at: m.updated_at,
            valid_until: m.valid_until,
            use_count: m.use_count,
            last_used_at: m.last_used_at,
        }
    }

    /// Everything the Memory page shows at once.
    pub fn overview(&self) -> MemoryOverview {
        let rows = lock(&self.db).memories().unwrap_or_default();
        let mut tags: BTreeMap<String, u32> = BTreeMap::new();
        let mut folders: BTreeMap<String, u32> = BTreeMap::new();
        for m in &rows {
            for t in &m.tags {
                *tags.entry(t.clone()).or_default() += 1;
            }
            // Sensitive notes can be filtered like a tag.
            if class_of(&m.sensitivity) >= DataClass::Sensitive
                && !m.tags.iter().any(|t| t == "sensitive")
            {
                *tags.entry("sensitive".into()).or_default() += 1;
            }
            let mut parts: Vec<&str> = m.path.split('/').collect();
            parts.pop();
            for i in 1..=parts.len() {
                *folders.entry(parts[..i].join("/")).or_default() += 1;
            }
        }
        let suggestions = lock(&self.db)
            .memory_suggestions("pending")
            .unwrap_or_default()
            .into_iter()
            .map(|s| MemorySuggestionView {
                id: s.id,
                text: s.text,
                reason: s.reason,
                workspace: s.workspace,
                created_at: s.created_at,
            })
            .collect();
        MemoryOverview {
            root: self.vault.root().display().to_string(),
            notes: rows.iter().map(Self::view).collect(),
            tags: tags
                .into_iter()
                .map(|(tag, count)| MemoryTagView { tag, count })
                .collect(),
            folders: folders
                .into_iter()
                .map(|(path, count)| MemoryFolderView { path, count })
                .collect(),
            suggestions,
        }
    }

    /// One note with its Markdown, links, backlinks and the facts it replaced or was replaced by.
    pub fn note(&self, path: &str) -> Result<MemoryNoteDetail, String> {
        let raw = self.vault.read(path).map_err(|e| e.to_string())?;
        let db = lock(&self.db);
        let row = db
            .memory(path)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| text::t("memory.notFound"))?;
        let note = Note::parse(&raw);
        let stem = path.strip_suffix(".md").unwrap_or(path).to_owned();
        let file = stem.rsplit('/').next().unwrap_or(&stem).to_owned();
        let backlinks = db
            .memory_backlinks(&[row.title.clone(), stem.clone(), file])
            .unwrap_or_default()
            .into_iter()
            .filter(|p| p != path)
            .collect();
        // The chain of facts about the same thing: what this replaced and what replaced it.
        let history: Vec<MemoryNoteView> = if row.kind == "fact" {
            let subject = query::subject(&row.text);
            db.memories()
                .unwrap_or_default()
                .iter()
                .filter(|m| {
                    m.kind == "fact"
                        && m.path != path
                        && subject.is_some()
                        && query::subject(&m.text) == subject
                })
                .map(Self::view)
                .collect()
        } else {
            Vec::new()
        };
        Ok(MemoryNoteDetail {
            note: Self::view(&row),
            markdown: raw,
            links: graph::links(&note.body),
            backlinks,
            history,
            superseded_by: note.front.superseded_by,
        })
    }
}

/// Runs the tidy job ten minutes after start and then once a day, reporting in Activity when it
/// changed anything.
pub fn tidy_daily(memory: Arc<Memory>, engine: Arc<crate::engine::Engine>) {
    tokio::spawn(async move {
        let mut wait = std::time::Duration::from_secs(10 * 60);
        loop {
            tokio::time::sleep(wait).await;
            wait = std::time::Duration::from_secs(24 * 60 * 60);
            let m = Arc::clone(&memory);
            let Ok(report) = tokio::task::spawn_blocking(move || m.tidy()).await else {
                continue;
            };
            if !report.is_empty() {
                engine
                    .recorder
                    .background("memory", &report.summary(), None);
            }
        }
    });
}

impl TidyReport {
    /// "Tidied memory: 2 merged, 1 updated, 3 logs condensed".
    pub fn summary(&self) -> String {
        text::tf(
            "memory.tidied",
            &[
                ("merged", &self.merged),
                ("superseded", &self.superseded),
                ("logs", &self.condensed_logs),
            ],
        )
    }
}

/// Replaces each secret the detectors find with "[removed]".
fn remove_secrets(text: &str) -> String {
    text.lines()
        .map(|l| {
            if classify(l) == DataClass::Credential {
                kivo_security::classify::redact_secrets(l)
            } else {
                l.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + if text.ends_with('\n') { "\n" } else { "" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_short_first_sentences() {
        assert_eq!(
            title_for("My project is in D:\\work. It uses Rust."),
            "My project is in D:\\work"
        );
        assert_eq!(title_for("K.I.V.O uses Rust"), "K.I.V.O uses Rust");
        let long =
            "Maya prefers that every design review happens on Thursday afternoons after lunch";
        let t = title_for(long);
        assert!(t.ends_with('…') && t.chars().count() <= 58, "{t}");
    }

    fn setup() -> (Arc<Memory>, Arc<Core>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let core = Arc::new(Core::with_config(kivo_core::KivoConfig::default(), None));
        let db = Arc::new(Mutex::new(Database::in_memory().unwrap()));
        let memory = Memory::new(Arc::clone(&core), db, dir.path().join("memory"), 0);
        (memory, core, dir)
    }

    fn ask(text: &str) -> Ask<'_> {
        Ask {
            text,
            ..Ask::default()
        }
    }

    fn file(dir: &tempfile::TempDir, path: &str) -> String {
        std::fs::read_to_string(dir.path().join("memory").join(path)).unwrap()
    }

    /// MEM-04/05, CONV-17: a fact becomes a note; saying it again adds nothing; a newer fact
    /// about the same thing supersedes the older one, which stays as history.
    #[test]
    fn facts_are_kept_once_and_superseded_not_lost() {
        let (memory, _core, dir) = setup();
        let Remembered::New { path: first, .. } = memory
            .remember(
                "My project is in D:\\work",
                &["projects".into()],
                Some("turn:1"),
                false,
            )
            .unwrap()
        else {
            panic!("a new note");
        };
        assert_eq!(first, "notes/my-project-is-in-d-work.md");
        let text = file(&dir, &first);
        assert!(
            text.contains("type: fact") && text.contains("source: turn:1"),
            "{text}"
        );
        assert_eq!(
            memory
                .remember("my project is in D:\\work", &[], None, false)
                .unwrap(),
            Remembered::Already {
                path: first.clone()
            }
        );
        let Remembered::New {
            path: second,
            superseded,
        } = memory
            .remember("My project is in E:\\code now", &[], None, false)
            .unwrap()
        else {
            panic!("a new note");
        };
        assert_eq!(superseded, std::slice::from_ref(&first));
        let old = file(&dir, &first);
        assert!(
            old.contains("valid_until:") && old.contains("superseded_by:"),
            "{old}"
        );
        // Only the current fact is recalled; the history is still there.
        let recalled = memory.recall(&ask("where is my project?"), MEMORY_TOKENS);
        assert_eq!(recalled.len(), 1);
        assert_eq!(recalled[0].path, second);
        let detail = memory.note(&second).unwrap();
        assert_eq!(detail.history.len(), 1);
        // Secrets are refused.
        assert!(
            memory
                .remember("the wifi password is hunter22", &[], None, false)
                .is_err()
        );
    }

    /// MEM-07/08: guests get nothing; sensitive notes stay off cloud brains unless both the
    /// setting and the note allow it; scope and the token budget hold.
    #[test]
    fn recall_follows_scope_privacy_guests_and_budget() {
        let (memory, core, dir) = setup();
        memory
            .remember("Maya's phone is +1 415 555 0100", &[], None, false)
            .unwrap();
        memory
            .remember("Maya likes jasmine tea", &[], None, false)
            .unwrap();
        std::fs::create_dir_all(dir.path().join("memory").join("topics")).unwrap();
        std::fs::write(
            dir.path()
                .join("memory")
                .join("topics")
                .join("code-style.md"),
            "---\nscope: app:code\n---\n# Code style\nMaya wants tabs in VS Code.\n",
        )
        .unwrap();
        memory.sync();
        let texts = |a: &Ask<'_>, budget: usize| -> Vec<String> {
            memory
                .recall(a, budget)
                .into_iter()
                .map(|r| r.text)
                .collect()
        };
        let local = texts(&ask("what about Maya"), MEMORY_TOKENS);
        assert_eq!(local.len(), 2, "{local:?}");
        let guest = Ask {
            guest: true,
            ..ask("what about Maya")
        };
        assert!(memory.recall(&guest, MEMORY_TOKENS).is_empty());
        // The phone number is personal: fine for the cloud (personal is allowed), but a
        // sensitive note isn't.
        let sensitive = memory.vault.free_path("notes", "health");
        memory
            .save(
                &sensitive,
                "---\nsensitivity: sensitive\n---\n# Health\nMaya is allergic to nuts.\n",
            )
            .unwrap();
        let cloud = Ask {
            cloud: true,
            ..ask("Maya allergic nuts")
        };
        assert!(
            texts(&cloud, MEMORY_TOKENS)
                .iter()
                .all(|t| !t.contains("allergic"))
        );
        assert!(
            texts(&ask("Maya allergic nuts"), MEMORY_TOKENS)
                .iter()
                .any(|t| t.contains("allergic"))
        );
        // Allowed per note, but the setting is still off: not sent.
        memory.set_meta(&sensitive, None, None, Some(true)).unwrap();
        assert!(
            texts(&cloud, MEMORY_TOKENS)
                .iter()
                .all(|t| !t.contains("allergic"))
        );
        core.update_config(|c| c.memory.sensitive_to_cloud = true);
        assert!(
            texts(&cloud, MEMORY_TOKENS)
                .iter()
                .any(|t| t.contains("allergic"))
        );
        // An app-scoped note only where that app is in front.
        assert!(
            texts(&ask("Maya tabs"), MEMORY_TOKENS)
                .iter()
                .all(|t| !t.contains("tabs"))
        );
        let in_code = Ask {
            app: Some("code"),
            ..ask("Maya tabs")
        };
        assert!(
            texts(&in_code, MEMORY_TOKENS)
                .iter()
                .any(|t| t.contains("tabs"))
        );
        // The budget: nothing longer than it fits.
        assert!(texts(&ask("what about Maya"), 3).is_empty());
        // Counted as used.
        let row = lock(&memory.db)
            .memory("notes/maya-likes-jasmine-tea.md")
            .unwrap()
            .unwrap();
        assert!(row.use_count >= 1);
    }

    /// CONV-18: the files win. An edit made outside KIVO is indexed; a deleted file is forgotten.
    #[test]
    fn edits_made_elsewhere_are_indexed() {
        let (memory, _core, dir) = setup();
        let Remembered::New { path, .. } = memory
            .remember("Standups are at 10:00", &[], None, false)
            .unwrap()
        else {
            panic!()
        };
        let full = dir.path().join("memory").join(&path);
        std::fs::write(
            &full,
            "---\ntype: fact\ntags: [work]\n---\n# Standups\nStandups are at 9:30 now.\n",
        )
        .unwrap();
        assert_eq!(memory.sync(), 1);
        let recalled = memory.recall(&ask("when are standups"), MEMORY_TOKENS);
        assert!(recalled[0].text.contains("9:30"));
        assert!(memory.overview().tags.iter().any(|t| t.tag == "work"));
        std::fs::remove_file(&full).unwrap();
        memory.sync();
        assert!(
            memory
                .recall(&ask("when are standups"), MEMORY_TOKENS)
                .is_empty()
        );
    }

    /// CONV-19/20: workspace notes follow the detail level; suggestions wait, dedupe, and are
    /// remembered only when accepted.
    #[test]
    fn workspace_notes_and_suggestions() {
        let (memory, core, dir) = setup();
        let bullets: Vec<String> = (1..=12).map(|i| format!("Step {i}")).collect();
        let path = memory
            .workspace_log("K.I.V.O", "Claude Code: fix the tests", &bullets)
            .unwrap();
        assert!(path.starts_with("workspaces/k-i-v-o/log/"), "{path}");
        let log = file(&dir, &path);
        assert_eq!(
            log.matches("\n- Step").count(),
            10,
            "Standard keeps 10: {log}"
        );
        assert!(file(&dir, "workspaces/k-i-v-o/overview.md").contains("# K.I.V.O"));
        core.update_config(|c| c.memory.workspace_notes = false);
        assert!(memory.workspace_log("K.I.V.O", "x", &bullets).is_none());

        let id = memory
            .suggest(
                "Sam ships releases on Fridays",
                Some("from a chat"),
                None,
                None,
            )
            .unwrap();
        assert_eq!(core.offer().unwrap().id, format!("memory:{id}"));
        assert!(
            memory
                .suggest("Sam ships releases on Fridays", None, None, None)
                .is_none(),
            "no repeats"
        );
        memory
            .answer(id, true, Some("Sam ships releases on Fridays after 4 pm"))
            .unwrap();
        assert!(core.offer().is_none());
        assert_eq!(
            memory
                .recall(&ask("when does Sam ship releases"), MEMORY_TOKENS)
                .len(),
            1
        );
        let no = memory
            .suggest("Sam drinks coffee", None, None, None)
            .unwrap();
        memory.answer(no, false, None).unwrap();
        assert!(
            memory
                .recall(&ask("Sam coffee"), MEMORY_TOKENS)
                .iter()
                .all(|r| !r.text.contains("coffee"))
        );
        assert!(
            memory
                .suggest("Sam drinks coffee", None, None, None)
                .is_none(),
            "a no is remembered"
        );
        core.update_config(|c| c.memory.capture = CaptureMode::OnlyWhenAsked);
        assert!(memory.suggest("Sam likes cats", None, None, None).is_none());
    }

    /// The tidy job: duplicates merged, old logs condensed and archived, secrets removed.
    #[test]
    fn tidy_keeps_memory_small_and_clean() {
        let (memory, _core, dir) = setup();
        let root = dir.path().join("memory");
        std::fs::create_dir_all(root.join("notes")).unwrap();
        std::fs::write(
            root.join("notes").join("a.md"),
            "---\ntype: fact\ncreated: 2026-01-01\n---\nTests run with cargo nextest\n",
        )
        .unwrap();
        std::fs::write(root.join("notes").join("b.md"), "---\ntype: fact\ncreated: 2026-02-01\ntags: [rust]\n---\nRun tests with cargo nextest\n").unwrap();
        std::fs::write(
            root.join("notes").join("key.md"),
            "---\ntype: fact\n---\nDeploy key sk-ant-api03-abcdefghijklmnopqrstuvwx\n",
        )
        .unwrap();
        let log_dir = root.join("workspaces").join("app").join("log");
        std::fs::create_dir_all(&log_dir).unwrap();
        std::fs::write(
            log_dir.join("2026-01-02.md"),
            "# 2026-01-02\n- Fixed the build\n- Decided to use pnpm\n",
        )
        .unwrap();
        memory.sync();
        let report = memory.tidy();
        assert_eq!(report.merged, 1);
        assert_eq!(report.secrets_removed, 1);
        assert_eq!(report.condensed_logs, 1);
        assert!(!root.join("notes").join("b.md").exists());
        assert!(
            std::fs::read_to_string(root.join("notes").join("a.md"))
                .unwrap()
                .contains("rust")
        );
        assert!(
            !std::fs::read_to_string(root.join("notes").join("key.md"))
                .unwrap()
                .contains("sk-ant")
        );
        assert!(file(&dir, "workspaces/app/overview.md").contains("Fixed the build (2026-01-02)"));
        assert!(
            file(&dir, "workspaces/app/decisions.md").contains("2026-01-02: Decided to use pnpm")
        );
        assert!(
            root.join("archive")
                .join("workspaces")
                .join("app")
                .join("log")
                .join("2026-01-02.md")
                .exists()
        );
        // Forget everything.
        assert!(memory.forget_everything() > 0);
        assert!(memory.overview().notes.is_empty());
        assert!(memory.export()["notes"].as_array().unwrap().is_empty());
    }
}
