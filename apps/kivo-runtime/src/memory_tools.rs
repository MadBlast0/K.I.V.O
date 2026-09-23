//! Memory tools (CONVERSATION §6, CONV-24): what KIVO remembers — the user's preferences and
//! notes, the workspaces' notes, and past conversations — for KIVO's brains and, through KIVO's
//! MCP server, for the agents the user allows. Each item says whether it is sensitive (personal
//! conversations and `personal.` notes): the MCP server leaves those out for cloud agents unless
//! the user allows it. The Markdown vault (M7) adds to what these tools search.

use crate::workspaces::Workspaces;
use kivo_core::Capability;
use kivo_core::text;
use kivo_core::tool::{
    CapabilityTier, Platform, Reversibility, Risk, SideEffect, ToolError, ToolErrorCode, ToolSpec,
};
use kivo_store::Database;
use kivo_tools::{Output, Tool};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

/// The most items a search returns.
const MAX_RESULTS: usize = 20;
/// The most a note added by `memory.add` may hold.
const MAX_NOTE_CHARS: usize = 2_000;

fn spec(id: &str, description: &str, params: Value, risk: Risk, write: bool) -> ToolSpec {
    ToolSpec {
        id: id.into(),
        description: description.into(),
        title: text::t(&format!("tool.{id}")),
        params,
        result: json!({ "type": "object" }),
        risk,
        side_effects: vec![if write {
            SideEffect::LocalWrite
        } else {
            SideEffect::LocalRead
        }],
        data_egress: false,
        timeout_ms: 5_000,
        cancellable: false,
        tier: CapabilityTier::Native,
        platforms: vec![Platform::Windows, Platform::MacOs, Platform::Linux],
        reversibility: Reversibility::NotApplicable,
        capability: Capability::Memory,
    }
}

fn lock(db: &Mutex<Database>) -> std::sync::MutexGuard<'_, Database> {
    db.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn failed(message: String) -> ToolError {
    ToolError::new(ToolErrorCode::Failed, message)
}

/// Whether a preference is personal ("personal.address") and so sensitive.
fn personal(key: &str) -> bool {
    key.starts_with("personal.")
}

/// A memory item for the result.
fn item(id: String, kind: &str, title: &str, text: &str, sensitive: bool) -> Value {
    let (short, _) = clip(text, 400);
    json!({ "id": id, "kind": kind, "title": title, "text": short, "sensitive": sensitive })
}

fn clip(text: &str, max: usize) -> (String, bool) {
    if text.chars().count() <= max {
        (text.to_owned(), false)
    } else {
        (text.chars().take(max).collect(), true)
    }
}

struct Search {
    spec: ToolSpec,
    db: Arc<Mutex<Database>>,
    workspaces: Arc<Workspaces>,
}

impl Tool for Search {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn run(&self, args: &Value) -> Result<Output, ToolError> {
        let query = args["query"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_lowercase();
        if query.is_empty() {
            return Err(ToolError::new(
                ToolErrorCode::InvalidArgs,
                text::t("memory.noQuery"),
            ));
        }
        let words: Vec<&str> = query.split_whitespace().collect();
        let matches = |s: &str| {
            let s = s.to_lowercase();
            words.iter().any(|w| s.contains(w))
        };
        let mut items = Vec::new();
        let db = lock(&self.db);
        for (key, value) in db.preferences().unwrap_or_default() {
            if matches(&key) || matches(&value) {
                items.push(item(
                    format!("pref:{key}"),
                    "preference",
                    &key,
                    &value,
                    personal(&key),
                ));
            }
        }
        drop(db);
        for w in self.workspaces.list() {
            let notes = self.workspaces.instructions(&format!("workspace:{}", w.id));
            if matches(&w.name) || matches(&notes) {
                items.push(item(
                    format!("workspace:{}", w.id),
                    "workspace",
                    &w.name,
                    &notes,
                    false,
                ));
            }
        }
        let about = self.workspaces.instructions("global");
        if !about.is_empty() && matches(&about) {
            items.push(item(
                "about-me".into(),
                "about",
                &text::t("memory.aboutMe"),
                &about,
                true,
            ));
        }
        let db = lock(&self.db);
        for m in db
            .search_messages(&query, u32::try_from(MAX_RESULTS).unwrap_or(20))
            .unwrap_or_default()
        {
            // What was said in a conversation is personal.
            items.push(item(
                format!("msg:{}", m.id),
                "conversation",
                &m.role,
                &m.text,
                true,
            ));
        }
        items.truncate(MAX_RESULTS);
        let say = text::tf("memory.found", &[("count", &items.len())]);
        Ok(Output::new(say, json!({ "items": items })))
    }
}

struct Get {
    spec: ToolSpec,
    db: Arc<Mutex<Database>>,
    workspaces: Arc<Workspaces>,
}

impl Tool for Get {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn run(&self, args: &Value) -> Result<Output, ToolError> {
        let id = args["id"].as_str().unwrap_or_default();
        let not_found = || ToolError::new(ToolErrorCode::NotFound, text::t("memory.notFound"));
        let (kind, title, body, sensitive) = if let Some(key) = id.strip_prefix("pref:") {
            let value = lock(&self.db)
                .preferences()
                .unwrap_or_default()
                .into_iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v)
                .ok_or_else(not_found)?;
            ("preference", key.to_owned(), value, personal(key))
        } else if let Some(ws) = id.strip_prefix("workspace:") {
            let w = self
                .workspaces
                .list()
                .into_iter()
                .find(|w| w.id == ws)
                .ok_or_else(not_found)?;
            let notes = self.workspaces.instructions(&format!("workspace:{ws}"));
            ("workspace", w.name, notes, false)
        } else if id == "about-me" {
            (
                "about",
                text::t("memory.aboutMe"),
                self.workspaces.instructions("global"),
                true,
            )
        } else if let Some(n) = id.strip_prefix("msg:").and_then(|n| n.parse::<i64>().ok()) {
            let m = lock(&self.db)
                .message(n)
                .ok()
                .flatten()
                .ok_or_else(not_found)?;
            ("conversation", m.role, m.text, true)
        } else {
            return Err(not_found());
        };
        Ok(Output::new(
            title.clone(),
            json!({ "id": id, "kind": kind, "title": title, "text": body, "sensitive": sensitive }),
        ))
    }
}

struct Add {
    spec: ToolSpec,
    db: Arc<Mutex<Database>>,
}

impl Tool for Add {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn run(&self, args: &Value) -> Result<Output, ToolError> {
        let note = args["text"].as_str().unwrap_or_default().trim();
        if note.is_empty() {
            return Err(ToolError::new(
                ToolErrorCode::InvalidArgs,
                text::t("memory.noText"),
            ));
        }
        let (note, _) = clip(note, MAX_NOTE_CHARS);
        let topic = args["topic"]
            .as_str()
            .map(|t| {
                t.chars()
                    .filter(|c| c.is_alphanumeric() || *c == '-' || *c == ' ')
                    .collect::<String>()
                    .trim()
                    .replace(' ', "-")
                    .to_lowercase()
            })
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| "note".into());
        let key = format!("{topic}.{}", kivo_store::brains::now_ms());
        lock(&self.db)
            .set_preference(&key, &note, "memory.add")
            .map_err(|e| failed(e.to_string()))?;
        Ok(Output::new(
            text::t("memory.added"),
            json!({ "id": format!("pref:{key}") }),
        ))
    }
}

struct Tags {
    spec: ToolSpec,
    db: Arc<Mutex<Database>>,
    workspaces: Arc<Workspaces>,
}

impl Tool for Tags {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn run(&self, _args: &Value) -> Result<Output, ToolError> {
        let mut topics: Vec<String> = lock(&self.db)
            .preferences()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(k, _)| k.split('.').next().map(str::to_owned))
            .collect();
        topics.sort();
        topics.dedup();
        let workspaces: Vec<String> = self.workspaces.list().into_iter().map(|w| w.name).collect();
        Ok(Output::new(
            text::tf("memory.topics", &[("count", &topics.len())]),
            json!({ "topics": topics, "workspaces": workspaces, "kinds": ["preference", "workspace", "about", "conversation"] }),
        ))
    }
}

/// The memory tools.
pub fn tools(db: &Arc<Mutex<Database>>, workspaces: &Arc<Workspaces>) -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(Search {
            spec: spec(
                "memory.search",
                "Search what KIVO remembers about the user: preferences and notes, workspace notes, and past conversations. Returns short items with ids for memory.get.",
                json!({ "type": "object", "properties": { "query": { "type": "string" } }, "required": ["query"] }),
                Risk::Safe,
                false,
            ),
            db: Arc::clone(db),
            workspaces: Arc::clone(workspaces),
        }),
        Arc::new(Get {
            spec: spec(
                "memory.get",
                "Read one remembered item in full by the id memory.search gave.",
                json!({ "type": "object", "properties": { "id": { "type": "string" } }, "required": ["id"] }),
                Risk::Safe,
                false,
            ),
            db: Arc::clone(db),
            workspaces: Arc::clone(workspaces),
        }),
        Arc::new(Add {
            spec: spec(
                "memory.add",
                "Remember a short note for the user (a decision, a preference, a fact about a project), under a topic.",
                json!({ "type": "object", "properties": { "text": { "type": "string" }, "topic": { "type": "string" } }, "required": ["text"] }),
                Risk::Medium,
                true,
            ),
            db: Arc::clone(db),
        }),
        Arc::new(Tags {
            spec: spec(
                "memory.tags",
                "List the topics and workspaces KIVO's memory is organized by.",
                json!({ "type": "object", "properties": {} }),
                Risk::Safe,
                false,
            ),
            db: Arc::clone(db),
            workspaces: Arc::clone(workspaces),
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Core;

    fn setup() -> (Vec<Arc<dyn Tool>>, Arc<Mutex<Database>>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let core = Arc::new(Core::with_config(kivo_core::KivoConfig::default(), None));
        let db = Arc::new(Mutex::new(Database::in_memory().unwrap()));
        let workspaces = Workspaces::new(core, Arc::clone(&db), dir.path().join("KIVO"));
        workspaces
            .set_instructions("global", "Call me Sam. I live in Pune.")
            .unwrap();
        (tools(&db, &workspaces), db, dir)
    }

    fn tool(tools: &[Arc<dyn Tool>], id: &str) -> Arc<dyn Tool> {
        tools.iter().find(|t| t.spec().id == id).cloned().unwrap()
    }

    #[test]
    fn notes_are_added_found_read_and_marked_sensitive() {
        let (tools, db, _dir) = setup();
        db.lock()
            .unwrap()
            .set_preference("personal.address", "12 Park Street", "user")
            .unwrap();
        let add = tool(&tools, "memory.add");
        assert_eq!(
            add.spec().risk,
            Risk::Medium,
            "writing asks like any change"
        );
        let added = add
            .run(&json!({ "text": "KIVO tests run with cargo nextest.", "topic": "Rust testing" }))
            .unwrap();
        let id = added.data["id"].as_str().unwrap().to_owned();
        assert!(id.starts_with("pref:rust-testing."), "{id}");

        let found = tool(&tools, "memory.search")
            .run(&json!({ "query": "nextest" }))
            .unwrap();
        let items = found.data["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["sensitive"], false);
        let got = tool(&tools, "memory.get")
            .run(&json!({ "id": id }))
            .unwrap();
        assert_eq!(got.data["text"], "KIVO tests run with cargo nextest.");

        let personal = tool(&tools, "memory.search")
            .run(&json!({ "query": "park pune" }))
            .unwrap();
        let items = personal.data["items"].as_array().unwrap();
        assert_eq!(items.len(), 2, "{items:?}");
        assert!(
            items.iter().all(|i| i["sensitive"] == true),
            "personal notes and About me"
        );

        let tags = tool(&tools, "memory.tags").run(&json!({})).unwrap();
        assert_eq!(tags.data["topics"], json!(["personal", "rust-testing"]));
        assert!(
            tool(&tools, "memory.get")
                .run(&json!({ "id": "pref:nope" }))
                .is_err()
        );
        assert!(
            tool(&tools, "memory.search")
                .run(&json!({ "query": " " }))
                .is_err()
        );
    }
}
