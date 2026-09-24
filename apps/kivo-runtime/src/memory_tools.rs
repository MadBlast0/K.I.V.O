//! Memory tools (CONVERSATION §6, CONV-24, MEM-05): what KIVO remembers — the notes in the
//! memory vault, the user's stated preferences, and past conversations — for KIVO's brains and,
//! through KIVO's MCP server, for the agents the user allows.
//!
//! Each item says whether it is sensitive (personal notes, About me, `personal.` preferences,
//! conversations): the MCP server leaves those out for agents unless the user allows it. Guests
//! get none of these tools (the permission engine denies the Memory capability to guests).

use crate::memory::{Memory, Remembered};
use kivo_core::Capability;
use kivo_core::text;
use kivo_core::tool::{
    CapabilityTier, Platform, Reversibility, Risk, SideEffect, ToolError, ToolErrorCode, ToolSpec,
};
use kivo_memory::query;
use kivo_security::classify::DataClass;
use kivo_store::Database;
use kivo_tools::{Output, Tool};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

/// The most items a search returns.
const MAX_RESULTS: usize = 20;

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

fn sensitive_class(name: &str) -> bool {
    serde_json::from_value::<DataClass>(Value::String(name.to_owned()))
        .is_ok_and(|c| c >= DataClass::Personal)
}

/// A memory item for the result.
fn item(id: String, kind: &str, title: &str, text: &str, sensitive: bool) -> Value {
    let short: String = text.chars().take(400).collect();
    json!({ "id": id, "kind": kind, "title": title, "text": short, "sensitive": sensitive })
}

struct Search {
    spec: ToolSpec,
    db: Arc<Mutex<Database>>,
    memory: Arc<Memory>,
}

impl Tool for Search {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn run(&self, args: &Value) -> Result<Output, ToolError> {
        // `query`, or `text` when the grammar matched "what do you remember about …".
        let query_text = args["query"]
            .as_str()
            .or_else(|| args["text"].as_str())
            .unwrap_or_default()
            .trim()
            .to_lowercase();
        if query_text.is_empty() {
            return Err(ToolError::new(
                ToolErrorCode::InvalidArgs,
                text::t("memory.noQuery"),
            ));
        }
        let words = query::keywords(&query_text);
        let matches = |s: &str| {
            let s = s.to_lowercase();
            words.iter().any(|w| s.contains(w.as_str()))
        };
        let mut items = Vec::new();
        // Notes in the vault, best first by words and meaning (CONV-23); superseded facts are
        // left out.
        let now = kivo_store::brains::now_ms();
        let notes = self.memory.find(&query_text, 30);
        let db = lock(&self.db);
        {
            for m in notes {
                if m.valid_until.is_some_and(|v| v <= now) {
                    continue;
                }
                items.push(item(
                    m.path.clone(),
                    &m.kind,
                    &m.title,
                    &m.text,
                    sensitive_class(&m.sensitivity),
                ));
            }
        }
        // Stated preferences (MEM-03).
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
        // What was said in conversations is personal.
        for m in db
            .search_messages(&query_text, u32::try_from(MAX_RESULTS).unwrap_or(20))
            .unwrap_or_default()
        {
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
    memory: Arc<Memory>,
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
            (
                "preference".to_owned(),
                key.to_owned(),
                value,
                personal(key),
            )
        } else if let Some(n) = id.strip_prefix("msg:").and_then(|n| n.parse::<i64>().ok()) {
            let m = lock(&self.db)
                .message(n)
                .ok()
                .flatten()
                .ok_or_else(not_found)?;
            ("conversation".to_owned(), m.role, m.text, true)
        } else {
            let detail = self.memory.note(id).map_err(|_| not_found())?;
            let body = kivo_memory::Note::parse(&detail.markdown).text();
            (
                detail.note.kind.clone(),
                detail.note.title.clone(),
                body,
                sensitive_class(&detail.note.sensitivity),
            )
        };
        Ok(Output::new(
            title.clone(),
            json!({ "id": id, "kind": kind, "title": title, "text": body, "sensitive": sensitive }),
        ))
    }
}

struct Add {
    spec: ToolSpec,
    memory: Arc<Memory>,
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
        let mut tags: Vec<String> = args["tags"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|t| t.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(topic) = args["topic"].as_str().filter(|t| !t.trim().is_empty()) {
            tags.push(kivo_memory::vault::slug(topic));
        }
        match self.memory.remember(note, &tags, Some("tool"), false) {
            Ok(Remembered::New { path, superseded }) => Ok(Output::new(
                if superseded.is_empty() {
                    text::t("memory.added")
                } else {
                    text::t("memory.replaced")
                },
                json!({ "id": path, "superseded": superseded }),
            )),
            Ok(Remembered::Already { path }) => Ok(Output::new(
                text::t("memory.already"),
                json!({ "id": path }),
            )),
            Err(e) => Err(failed(e)),
        }
    }
}

struct Forget {
    spec: ToolSpec,
    db: Arc<Mutex<Database>>,
    memory: Arc<Memory>,
}

impl Tool for Forget {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn run(&self, args: &Value) -> Result<Output, ToolError> {
        // By the id memory.search gave, or by what it says ("forget that my project is in D:\work").
        let path = match args["id"].as_str().filter(|i| !i.is_empty()) {
            Some(id) => id.to_owned(),
            None => {
                let what = args["text"].as_str().unwrap_or_default();
                let fts = query::fts_query(what).ok_or_else(|| {
                    ToolError::new(ToolErrorCode::InvalidArgs, text::t("memory.noText"))
                })?;
                lock(&self.db)
                    .search_memories(&fts, 5)
                    .unwrap_or_default()
                    .into_iter()
                    .find(|m| m.kind == "fact")
                    .map(|m| m.path)
                    .ok_or_else(|| {
                        ToolError::new(ToolErrorCode::NotFound, text::t("memory.notFound"))
                    })?
            }
        };
        self.memory.delete(&path).map_err(failed)?;
        Ok(Output::new(
            text::t("memory.forgotten"),
            json!({ "id": path }),
        ))
    }
}

struct Propose {
    spec: ToolSpec,
    memory: Arc<Memory>,
}

impl Tool for Propose {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn run(&self, args: &Value) -> Result<Output, ToolError> {
        let what = args["text"].as_str().unwrap_or_default();
        let reason = args["reason"].as_str();
        match self.memory.suggest(what, reason, Some("brain"), None) {
            Some(id) => Ok(Output::new(
                text::t("memory.proposed"),
                json!({ "suggestion": id }),
            )),
            None => Ok(Output::new(
                text::t("memory.notProposed"),
                json!({ "suggestion": null }),
            )),
        }
    }
}

struct Tags {
    spec: ToolSpec,
    memory: Arc<Memory>,
}

impl Tool for Tags {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn run(&self, _args: &Value) -> Result<Output, ToolError> {
        let o = self.memory.overview();
        let tags: Vec<&str> = o.tags.iter().map(|t| t.tag.as_str()).collect();
        let folders: Vec<&str> = o.folders.iter().map(|f| f.path.as_str()).collect();
        Ok(Output::new(
            text::tf("memory.topics", &[("count", &tags.len())]),
            json!({ "tags": tags, "folders": folders }),
        ))
    }
}

/// The memory tools.
pub fn tools(db: &Arc<Mutex<Database>>, memory: &Arc<Memory>) -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(Search {
            spec: spec(
                "memory.search",
                "Search what KIVO remembers about the user: notes in their memory (facts, people, workspaces, topics, decisions), stated preferences, and past conversations. Returns short items with ids for memory.get.",
                json!({ "type": "object", "properties": { "query": { "type": "string" } }, "required": ["query"] }),
                Risk::Safe,
                false,
            ),
            db: Arc::clone(db),
            memory: Arc::clone(memory),
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
            memory: Arc::clone(memory),
        }),
        Arc::new(Add {
            spec: spec(
                "memory.add",
                "Remember something for the user when they ask you to remember it: a short fact, preference or decision, optionally with tags.",
                json!({ "type": "object", "properties": {
                    "text": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "topic": { "type": "string" }
                }, "required": ["text"] }),
                Risk::Medium,
                true,
            ),
            memory: Arc::clone(memory),
        }),
        Arc::new(Forget {
            spec: spec(
                "memory.forget",
                "Forget something KIVO remembered, when the user asks: by its id, or by what it says.",
                json!({ "type": "object", "properties": {
                    "id": { "type": "string" },
                    "text": { "type": "string" }
                } }),
                Risk::Medium,
                true,
            ),
            db: Arc::clone(db),
            memory: Arc::clone(memory),
        }),
        Arc::new(Propose {
            spec: spec(
                "memory.propose",
                "Suggest remembering a lasting fact the user told you about themselves or their work (not small talk, never a password). KIVO asks the user; nothing is kept unless they accept.",
                json!({ "type": "object", "properties": {
                    "text": { "type": "string" },
                    "reason": { "type": "string" }
                }, "required": ["text"] }),
                Risk::Safe,
                false,
            ),
            memory: Arc::clone(memory),
        }),
        Arc::new(Tags {
            spec: spec(
                "memory.tags",
                "List the tags and folders KIVO's memory is organized by.",
                json!({ "type": "object", "properties": {} }),
                Risk::Safe,
                false,
            ),
            memory: Arc::clone(memory),
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Core;

    type Setup = (
        Vec<Arc<dyn Tool>>,
        Arc<Mutex<Database>>,
        Arc<Memory>,
        tempfile::TempDir,
    );

    fn setup() -> Setup {
        let dir = tempfile::tempdir().unwrap();
        let core = Arc::new(Core::with_config(kivo_core::KivoConfig::default(), None));
        let db = Arc::new(Mutex::new(Database::in_memory().unwrap()));
        let memory = Memory::new(core, Arc::clone(&db), dir.path().join("memory"), 0);
        std::fs::write(
            dir.path().join("memory").join("about-me.md"),
            "Call me Sam. I live in Pune.",
        )
        .unwrap();
        memory.sync();
        (tools(&db, &memory), db, memory, dir)
    }

    fn tool(tools: &[Arc<dyn Tool>], id: &str) -> Arc<dyn Tool> {
        tools.iter().find(|t| t.spec().id == id).cloned().unwrap()
    }

    #[test]
    fn notes_are_added_found_read_forgotten_and_marked_sensitive() {
        let (tools, db, _memory, dir) = setup();
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
        assert_eq!(id, "notes/kivo-tests-run-with-cargo-nextest.md");
        let file = std::fs::read_to_string(dir.path().join("memory").join(&id)).unwrap();
        assert!(
            file.contains("type: fact") && file.contains("tags: [rust-testing]"),
            "{file}"
        );
        // Saying it again adds nothing.
        let again = add
            .run(&json!({ "text": "KIVO tests run with cargo nextest" }))
            .unwrap();
        assert_eq!(again.data["id"], id.as_str());

        let found = tool(&tools, "memory.search")
            .run(&json!({ "query": "nextest" }))
            .unwrap();
        let items = found.data["items"].as_array().unwrap();
        assert_eq!(items.len(), 1, "{items:?}");
        assert_eq!(items[0]["sensitive"], false);
        let got = tool(&tools, "memory.get")
            .run(&json!({ "id": id }))
            .unwrap();
        assert!(got.data["text"].as_str().unwrap().contains("cargo nextest"));

        let personal = tool(&tools, "memory.search")
            .run(&json!({ "query": "park pune" }))
            .unwrap();
        let items = personal.data["items"].as_array().unwrap();
        assert_eq!(items.len(), 2, "{items:?}");
        assert!(
            items.iter().all(|i| i["sensitive"] == true),
            "personal preferences and About me: {items:?}"
        );

        let tags = tool(&tools, "memory.tags").run(&json!({})).unwrap();
        assert!(
            tags.data["tags"]
                .as_array()
                .unwrap()
                .contains(&json!("rust-testing"))
        );
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
        // Passwords are never kept.
        assert!(
            add.run(&json!({ "text": "my wifi password is hunter22" }))
                .is_err()
        );

        tool(&tools, "memory.forget")
            .run(&json!({ "text": "cargo nextest" }))
            .unwrap();
        assert!(!dir.path().join("memory").join(&id).exists());
    }
}
