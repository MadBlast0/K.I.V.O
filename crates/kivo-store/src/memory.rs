//! The memory index (MEM-04, CONV-17): one row per vault note, full-text search over them, the
//! knowledge graph each note contributes, links for backlinks, suggestions waiting for the user
//! (CONV-20) and which memories a turn used (MEM-10). Rows are rebuilt from the Markdown files;
//! only `use_count` and `last_used_at` exist here alone, and survive a note being re-indexed.

use crate::brains::now_ms;
use crate::db::{Database, DbError};
use rusqlite::{OptionalExtension, Row, params};

/// One note in the index.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemoryRow {
    pub path: String,
    pub kind: String,
    pub title: String,
    /// The body without its title.
    pub text: String,
    pub tags: Vec<String>,
    pub workspace: Option<String>,
    /// `global`, `app:<id>`, `project:<path>`.
    pub scope: String,
    /// A data class.
    pub sensitivity: String,
    pub share_cloud: bool,
    pub source: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub valid_until: Option<i64>,
    pub last_used_at: Option<i64>,
    pub use_count: i64,
    pub hash: String,
}

/// What a note adds to the graph (see `kivo_memory::graph`).
#[derive(Clone, Debug, Default)]
pub struct GraphRows {
    /// (name, kind).
    pub entities: Vec<(String, String)>,
    /// (from, kind, to).
    pub relations: Vec<(String, String, String)>,
    /// (entity, text).
    pub observations: Vec<(String, String)>,
    /// Link targets as written, for backlinks.
    pub links: Vec<String>,
}

/// A fact about an entity, with when it held.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredObservation {
    pub entity: String,
    pub text: String,
    pub path: String,
    pub valid_from: i64,
    pub valid_to: Option<i64>,
}

/// A suggestion KIVO made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredSuggestion {
    pub id: i64,
    pub text: String,
    pub reason: Option<String>,
    pub source: Option<String>,
    pub workspace: Option<String>,
    pub created_at: i64,
    pub state: String,
}

const COLUMNS: &str = "path, kind, title, text, tags, workspace, scope, sensitivity, share_cloud,
     source, created_at, updated_at, valid_until, last_used_at, use_count, hash";

fn row(r: &Row<'_>) -> rusqlite::Result<MemoryRow> {
    let tags: String = r.get(4)?;
    Ok(MemoryRow {
        path: r.get(0)?,
        kind: r.get(1)?,
        title: r.get(2)?,
        text: r.get(3)?,
        tags: serde_json::from_str(&tags).unwrap_or_default(),
        workspace: r.get(5)?,
        scope: r.get(6)?,
        sensitivity: r.get(7)?,
        share_cloud: r.get(8)?,
        source: r.get(9)?,
        created_at: r.get(10)?,
        updated_at: r.get(11)?,
        valid_until: r.get(12)?,
        last_used_at: r.get(13)?,
        use_count: r.get(14)?,
        hash: r.get(15)?,
    })
}

impl Database {
    /// Adds or re-indexes a note, and replaces what it contributes to the graph. Use counts are
    /// kept.
    pub fn index_memory(&self, m: &MemoryRow, graph: &GraphRows) -> Result<(), DbError> {
        let owner = self.owner()?;
        let tx = self.connection().unchecked_transaction()?;
        tx.execute(
            "INSERT INTO memories (profile_id, path, kind, title, text, tags, workspace, scope,
                 sensitivity, share_cloud, source, created_at, updated_at, valid_until, hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT (profile_id, path) DO UPDATE SET
                 kind = excluded.kind, title = excluded.title, text = excluded.text,
                 tags = excluded.tags, workspace = excluded.workspace, scope = excluded.scope,
                 sensitivity = excluded.sensitivity, share_cloud = excluded.share_cloud,
                 source = excluded.source, created_at = excluded.created_at,
                 updated_at = excluded.updated_at, valid_until = excluded.valid_until,
                 hash = excluded.hash",
            params![
                owner,
                m.path,
                m.kind,
                m.title,
                m.text,
                serde_json::to_string(&m.tags).unwrap_or_else(|_| "[]".into()),
                m.workspace,
                m.scope,
                m.sensitivity,
                m.share_cloud,
                m.source,
                m.created_at,
                m.updated_at,
                m.valid_until,
                m.hash,
            ],
        )?;
        let id: i64 = tx.query_row(
            "SELECT id FROM memories WHERE profile_id = ?1 AND path = ?2",
            params![owner, m.path],
            |r| r.get(0),
        )?;
        tx.execute("DELETE FROM relations WHERE memory_id = ?1", [id])?;
        tx.execute("DELETE FROM observations WHERE memory_id = ?1", [id])?;
        tx.execute("DELETE FROM memory_links WHERE memory_id = ?1", [id])?;
        let entity = |name: &str, kind: &str| -> Result<i64, DbError> {
            // A note's own kind wins over "thing" (a link to it written elsewhere).
            tx.execute(
                "INSERT INTO entities (profile_id, name, kind) VALUES (?1, ?2, ?3)
                 ON CONFLICT (profile_id, name) DO UPDATE SET kind = excluded.kind
                 WHERE entities.kind = 'thing'",
                params![owner, name, kind],
            )?;
            Ok(tx.query_row(
                "SELECT id FROM entities WHERE profile_id = ?1 AND name = ?2",
                params![owner, name],
                |r| r.get(0),
            )?)
        };
        for (name, kind) in &graph.entities {
            entity(name, kind)?;
        }
        for (from, kind, to) in &graph.relations {
            let (a, b) = (entity(from, "thing")?, entity(to, "thing")?);
            tx.execute(
                "INSERT INTO relations (profile_id, from_id, kind, to_id, memory_id)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![owner, a, kind, b, id],
            )?;
        }
        for (name, text) in &graph.observations {
            let e = entity(name, "thing")?;
            tx.execute(
                "INSERT INTO observations (profile_id, entity_id, text, memory_id, valid_from, valid_to)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![owner, e, text, id, m.created_at, m.valid_until],
            )?;
        }
        for target in &graph.links {
            tx.execute(
                "INSERT INTO memory_links (profile_id, memory_id, target) VALUES (?1, ?2, ?3)",
                params![owner, id, target],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Forgets a note's index row (its graph rows go with it). Entities nothing refers to any more
    /// are removed.
    pub fn unindex_memory(&self, path: &str) -> Result<bool, DbError> {
        let owner = self.owner()?;
        let n = self.connection().execute(
            "DELETE FROM memories WHERE profile_id = ?1 AND path = ?2",
            params![owner, path],
        )?;
        self.drop_orphan_entities()?;
        Ok(n > 0)
    }

    fn drop_orphan_entities(&self) -> Result<(), DbError> {
        self.connection().execute(
            "DELETE FROM entities WHERE id NOT IN (SELECT entity_id FROM observations)
               AND id NOT IN (SELECT from_id FROM relations)
               AND id NOT IN (SELECT to_id FROM relations)
               AND lower(name) NOT IN (SELECT lower(title) FROM memories)",
            [],
        )?;
        Ok(())
    }

    /// (path, hash) of every indexed note, to see what changed on disk.
    pub fn memory_hashes(&self) -> Result<Vec<(String, String)>, DbError> {
        let owner = self.owner()?;
        let mut stmt = self
            .connection()
            .prepare("SELECT path, hash FROM memories WHERE profile_id = ?1")?;
        let rows = stmt.query_map([owner], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn memory(&self, path: &str) -> Result<Option<MemoryRow>, DbError> {
        let owner = self.owner()?;
        Ok(self
            .connection()
            .query_row(
                &format!("SELECT {COLUMNS} FROM memories WHERE profile_id = ?1 AND path = ?2"),
                params![owner, path],
                row,
            )
            .optional()?)
    }

    /// Every note, newest first.
    pub fn memories(&self) -> Result<Vec<MemoryRow>, DbError> {
        let owner = self.owner()?;
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {COLUMNS} FROM memories WHERE profile_id = ?1 ORDER BY updated_at DESC, path"
        ))?;
        let rows = stmt.query_map([owner], row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Notes matching an FTS5 expression, best first (bm25, titles weighted higher).
    pub fn search_memories(&self, fts: &str, limit: u32) -> Result<Vec<MemoryRow>, DbError> {
        let owner = self.owner()?;
        let prefixed: Vec<String> = COLUMNS
            .split(',')
            .map(|c| format!("m.{}", c.trim()))
            .collect();
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {} FROM memories_fts f JOIN memories m ON m.id = f.rowid
             WHERE memories_fts MATCH ?1 AND m.profile_id = ?2
             ORDER BY bm25(memories_fts, 5.0, 1.0, 2.0) LIMIT ?3",
            prefixed.join(", ")
        ))?;
        let rows = stmt.query_map(params![fts, owner, limit], row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Notes whose vector is missing or was made by another model than `model`, up to `limit`:
    /// (row id, the text to embed).
    pub fn memories_to_embed(
        &self,
        model: &str,
        limit: u32,
    ) -> Result<Vec<(i64, String)>, DbError> {
        let owner = self.owner()?;
        let mut stmt = self.connection().prepare(
            "SELECT m.id, m.title, m.text FROM memories m
             LEFT JOIN memory_vector_models v ON v.memory_id = m.id
             WHERE m.profile_id = ?1 AND (v.model IS NULL OR v.model != ?2)
             ORDER BY m.id LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![owner, model, limit], |r| {
            let (id, title, text): (i64, String, String) = (r.get(0)?, r.get(1)?, r.get(2)?);
            Ok((id, format!("{title}\n{text}")))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Stores a note's vector (MEM-09) and the model that made it.
    pub fn set_memory_vector(&self, id: i64, model: &str, vector: &[f32]) -> Result<(), DbError> {
        let blob: Vec<u8> = vector.iter().flat_map(|v| v.to_le_bytes()).collect();
        let c = self.connection();
        c.execute("DELETE FROM memory_vectors WHERE rowid = ?1", [id])?;
        c.execute(
            "INSERT INTO memory_vectors (rowid, embedding) VALUES (?1, ?2)",
            params![id, blob],
        )?;
        c.execute(
            "INSERT INTO memory_vector_models (profile_id, memory_id, model)
             SELECT profile_id, id, ?2 FROM memories WHERE id = ?1
             ON CONFLICT (memory_id) DO UPDATE SET model = excluded.model",
            params![id, model],
        )?;
        Ok(())
    }

    /// The `k` notes nearest to `vector` among those embedded by `model`, nearest first, with
    /// their cosine distance (0 is the same direction, 2 the opposite).
    pub fn nearest_memories(
        &self,
        vector: &[f32],
        model: &str,
        k: u32,
    ) -> Result<Vec<(MemoryRow, f32)>, DbError> {
        let owner = self.owner()?;
        let blob: Vec<u8> = vector.iter().flat_map(|v| v.to_le_bytes()).collect();
        let prefixed: Vec<String> = COLUMNS
            .split(',')
            .map(|c| format!("m.{}", c.trim()))
            .collect();
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {}, n.distance FROM
                 (SELECT rowid, distance FROM memory_vectors WHERE embedding MATCH ?1 AND k = ?2) n
             JOIN memories m ON m.id = n.rowid
             JOIN memory_vector_models v ON v.memory_id = m.id
             WHERE m.profile_id = ?3 AND v.model = ?4
             ORDER BY n.distance",
            prefixed.join(", ")
        ))?;
        let columns = COLUMNS.split(',').count();
        let rows = stmt.query_map(params![blob, k, owner, model], |r| {
            let distance: f64 = r.get(columns)?;
            #[allow(clippy::cast_possible_truncation, reason = "a distance in 0..=2")]
            Ok((row(r)?, distance as f32))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Counts a use of each note (retrieval put it in a request).
    pub fn mark_memories_used(&self, paths: &[String], at: i64) -> Result<(), DbError> {
        let owner = self.owner()?;
        for p in paths {
            self.connection().execute(
                "UPDATE memories SET use_count = use_count + 1, last_used_at = ?3
                 WHERE profile_id = ?1 AND path = ?2",
                params![owner, p, at],
            )?;
        }
        Ok(())
    }

    /// Every `[[link]]`: (the linking note's path, the target as written).
    pub fn memory_link_pairs(&self) -> Result<Vec<(String, String)>, DbError> {
        let owner = self.owner()?;
        let mut stmt = self.connection().prepare(
            "SELECT m.path, l.target FROM memory_links l JOIN memories m ON m.id = l.memory_id
             WHERE l.profile_id = ?1 ORDER BY m.path, l.target",
        )?;
        let rows = stmt.query_map([owner], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Notes that link to any of `targets` (a note's title, path, or file name).
    pub fn memory_backlinks(&self, targets: &[String]) -> Result<Vec<String>, DbError> {
        let owner = self.owner()?;
        let mut out: Vec<String> = Vec::new();
        let mut stmt = self.connection().prepare(
            "SELECT DISTINCT m.path FROM memory_links l JOIN memories m ON m.id = l.memory_id
             WHERE l.profile_id = ?1 AND l.target = ?2 ORDER BY m.path",
        )?;
        for t in targets {
            for p in stmt.query_map(params![owner, t], |r| r.get::<_, String>(0))? {
                let p = p?;
                if !out.contains(&p) {
                    out.push(p);
                }
            }
        }
        Ok(out)
    }

    /// What's known about an entity, oldest first, including facts no longer true.
    pub fn observations(&self, entity: &str) -> Result<Vec<StoredObservation>, DbError> {
        let owner = self.owner()?;
        let mut stmt = self.connection().prepare(
            "SELECT e.name, o.text, m.path, o.valid_from, o.valid_to
             FROM observations o JOIN entities e ON e.id = o.entity_id
             JOIN memories m ON m.id = o.memory_id
             WHERE o.profile_id = ?1 AND e.name = ?2 ORDER BY o.valid_from, o.id",
        )?;
        let rows = stmt.query_map(params![owner, entity], |r| {
            Ok(StoredObservation {
                entity: r.get(0)?,
                text: r.get(1)?,
                path: r.get(2)?,
                valid_from: r.get(3)?,
                valid_to: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// (from, kind, to) for every relation.
    pub fn relations(&self) -> Result<Vec<(String, String, String)>, DbError> {
        let owner = self.owner()?;
        let mut stmt = self.connection().prepare(
            "SELECT a.name, r.kind, b.name FROM relations r
             JOIN entities a ON a.id = r.from_id JOIN entities b ON b.id = r.to_id
             WHERE r.profile_id = ?1 ORDER BY a.name, b.name",
        )?;
        let rows = stmt.query_map([owner], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Forgets the whole index (Forget everything, after the files are gone). Suggestions go too.
    pub fn forget_memories(&self) -> Result<(), DbError> {
        let owner = self.owner()?;
        let c = self.connection();
        c.execute("DELETE FROM memories WHERE profile_id = ?1", [&owner])?;
        c.execute("DELETE FROM entities WHERE profile_id = ?1", [&owner])?;
        c.execute(
            "DELETE FROM memory_suggestions WHERE profile_id = ?1",
            [&owner],
        )?;
        c.execute("DELETE FROM turn_memories WHERE profile_id = ?1", [&owner])?;
        Ok(())
    }

    // ---- Suggestions (CONV-20) -----------------------------------------------------------------

    pub fn add_memory_suggestion(
        &self,
        text: &str,
        reason: Option<&str>,
        source: Option<&str>,
        workspace: Option<&str>,
    ) -> Result<i64, DbError> {
        let owner = self.owner()?;
        self.connection().execute(
            "INSERT INTO memory_suggestions (profile_id, text, reason, source, workspace, created_at, state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending')",
            params![owner, text, reason, source, workspace, now_ms()],
        )?;
        Ok(self.connection().last_insert_rowid())
    }

    /// Suggestions in `state` (`pending` for those waiting), newest first.
    pub fn memory_suggestions(&self, state: &str) -> Result<Vec<StoredSuggestion>, DbError> {
        let owner = self.owner()?;
        let mut stmt = self.connection().prepare(
            "SELECT id, text, reason, source, workspace, created_at, state FROM memory_suggestions
             WHERE profile_id = ?1 AND state = ?2 ORDER BY created_at DESC, id DESC",
        )?;
        let rows = stmt.query_map(params![owner, state], |r| {
            Ok(StoredSuggestion {
                id: r.get(0)?,
                text: r.get(1)?,
                reason: r.get(2)?,
                source: r.get(3)?,
                workspace: r.get(4)?,
                created_at: r.get(5)?,
                state: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn memory_suggestion(&self, id: i64) -> Result<Option<StoredSuggestion>, DbError> {
        Ok(self
            .memory_suggestions("pending")?
            .into_iter()
            .find(|s| s.id == id))
    }

    pub fn set_memory_suggestion(&self, id: i64, state: &str) -> Result<bool, DbError> {
        let owner = self.owner()?;
        Ok(self.connection().execute(
            "UPDATE memory_suggestions SET state = ?3 WHERE profile_id = ?1 AND id = ?2",
            params![owner, id, state],
        )? > 0)
    }

    // ---- Which memories a turn used (MEM-10) -------------------------------------------------

    pub fn record_turn_memories(
        &self,
        turn_id: &str,
        used: &[(String, String)],
    ) -> Result<(), DbError> {
        let owner = self.owner()?;
        for (path, title) in used {
            self.connection().execute(
                "INSERT OR IGNORE INTO turn_memories (profile_id, turn_id, path, title)
                 VALUES (?1, ?2, ?3, ?4)",
                params![owner, turn_id, path, title],
            )?;
        }
        Ok(())
    }

    /// (path, title) of the memories a turn used.
    pub fn turn_memories(&self, turn_id: &str) -> Result<Vec<(String, String)>, DbError> {
        let owner = self.owner()?;
        let mut stmt = self.connection().prepare(
            "SELECT path, title FROM turn_memories WHERE profile_id = ?1 AND turn_id = ?2 ORDER BY path",
        )?;
        let rows = stmt.query_map(params![owner, turn_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(path: &str, title: &str, text: &str) -> MemoryRow {
        MemoryRow {
            path: path.into(),
            kind: "fact".into(),
            title: title.into(),
            text: text.into(),
            tags: vec!["rust".into()],
            scope: "global".into(),
            sensitivity: "normal".into(),
            created_at: 1,
            updated_at: 1,
            hash: "h".into(),
            ..MemoryRow::default()
        }
    }

    #[test]
    fn notes_are_indexed_searched_and_counted() {
        let db = Database::in_memory().unwrap();
        db.index_memory(
            &note("notes/tests.md", "Tests", "Run tests with cargo nextest"),
            &GraphRows::default(),
        )
        .unwrap();
        db.index_memory(
            &note("notes/maya.md", "Maya", "Maya prefers dark mode"),
            &GraphRows::default(),
        )
        .unwrap();
        let found = db.search_memories("\"nextest\"*", 5).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, "notes/tests.md");
        db.mark_memories_used(&["notes/tests.md".into()], 50)
            .unwrap();
        // Re-indexing an edited note keeps its use count and updates the search.
        let mut edited = note(
            "notes/tests.md",
            "Tests",
            "Run tests with cargo test --workspace",
        );
        edited.hash = "h2".into();
        db.index_memory(&edited, &GraphRows::default()).unwrap();
        let row = db.memory("notes/tests.md").unwrap().unwrap();
        assert_eq!((row.use_count, row.last_used_at), (1, Some(50)));
        assert!(db.search_memories("\"nextest\"*", 5).unwrap().is_empty());
        assert_eq!(db.search_memories("\"workspace\"*", 5).unwrap().len(), 1);
        assert_eq!(db.memory_hashes().unwrap().len(), 2);
        assert!(db.unindex_memory("notes/maya.md").unwrap());
        assert!(db.search_memories("\"dark\"*", 5).unwrap().is_empty());
    }

    #[test]
    fn the_graph_keeps_history_and_backlinks() {
        let db = Database::in_memory().unwrap();
        let mut old = note("notes/maya-1.md", "Maya job", "[[Maya]] works at Acme");
        old.valid_until = Some(100);
        db.index_memory(
            &old,
            &GraphRows {
                entities: vec![("Maya".into(), "thing".into())],
                observations: vec![("Maya".into(), "[[Maya]] works at Acme".into())],
                links: vec!["Maya".into()],
                ..GraphRows::default()
            },
        )
        .unwrap();
        let mut new = note("notes/maya-2.md", "Maya job", "[[Maya]] works at Globex");
        new.created_at = 100;
        db.index_memory(
            &new,
            &GraphRows {
                entities: vec![("Maya".into(), "thing".into())],
                observations: vec![("Maya".into(), "[[Maya]] works at Globex".into())],
                links: vec!["Maya".into()],
                ..GraphRows::default()
            },
        )
        .unwrap();
        db.index_memory(
            &note("people/maya.md", "Maya", "- Designer"),
            &GraphRows {
                entities: vec![
                    ("Maya".into(), "person".into()),
                    ("Island".into(), "thing".into()),
                ],
                relations: vec![("Maya".into(), "links_to".into(), "Island".into())],
                observations: vec![("Maya".into(), "Designer".into())],
                links: vec!["Island".into()],
            },
        )
        .unwrap();
        let history = db.observations("maya").unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].valid_to, Some(100));
        assert_eq!(history[2].valid_to, None);
        assert_eq!(
            db.relations().unwrap(),
            [("Maya".into(), "links_to".into(), "Island".into())]
        );
        assert_eq!(
            db.memory_backlinks(&["Maya".into(), "people/maya".into()])
                .unwrap(),
            ["notes/maya-1.md", "notes/maya-2.md"]
        );
        // The person note gave Maya her kind; removing the facts keeps the entity (a note has it).
        db.unindex_memory("notes/maya-1.md").unwrap();
        assert_eq!(db.observations("Maya").unwrap().len(), 2);
    }

    #[test]
    fn suggestions_and_turn_memories() {
        let db = Database::in_memory().unwrap();
        let id = db
            .add_memory_suggestion(
                "Your project lives in D:\\work",
                Some("from a task"),
                Some("task:1"),
                None,
            )
            .unwrap();
        assert_eq!(db.memory_suggestions("pending").unwrap().len(), 1);
        assert!(db.set_memory_suggestion(id, "accepted").unwrap());
        assert!(db.memory_suggestions("pending").unwrap().is_empty());
        db.record_turn_memories("t1", &[("notes/a.md".into(), "A".into())])
            .unwrap();
        db.record_turn_memories("t1", &[("notes/a.md".into(), "A".into())])
            .unwrap();
        assert_eq!(
            db.turn_memories("t1").unwrap(),
            [("notes/a.md".into(), "A".into())]
        );
        db.forget_memories().unwrap();
        assert!(db.turn_memories("t1").unwrap().is_empty());
    }

    /// MEM-09: vectors in sqlite-vec, nearest first; a changed note needs a new vector, an
    /// unchanged re-index keeps it, another model's vectors don't count, a deleted note's go.
    #[test]
    fn note_vectors_are_searched_and_kept_current() {
        let db = Database::in_memory().unwrap();
        let axis = |i: usize| {
            let mut v = vec![0.0_f32; 384];
            v[i] = 1.0;
            v
        };
        let mut a = note("a.md", "Parking", "Level 3, spot 12");
        let b = note("b.md", "Sister", "Priya");
        db.index_memory(&a, &GraphRows::default()).unwrap();
        db.index_memory(&b, &GraphRows::default()).unwrap();
        let todo = db.memories_to_embed("minilm", 10).unwrap();
        assert_eq!(todo.len(), 2);
        assert!(todo[0].1.starts_with("Parking\n"));
        db.set_memory_vector(todo[0].0, "minilm", &axis(0)).unwrap();
        db.set_memory_vector(todo[1].0, "minilm", &axis(1)).unwrap();
        assert!(db.memories_to_embed("minilm", 10).unwrap().is_empty());

        let mut query = axis(0);
        query[1] = 0.3;
        let near = db.nearest_memories(&query, "minilm", 5).unwrap();
        assert_eq!(near[0].0.path, "a.md");
        assert!(near[0].1 < near[1].1);
        assert!(
            db.nearest_memories(&query, "other-model", 5)
                .unwrap()
                .is_empty()
        );
        assert_eq!(db.memories_to_embed("other-model", 10).unwrap().len(), 2);

        // Re-indexed unchanged: the vector stays. Changed: it's embedded again.
        db.index_memory(&a, &GraphRows::default()).unwrap();
        assert!(db.memories_to_embed("minilm", 10).unwrap().is_empty());
        a.text = "Level 4".into();
        db.index_memory(&a, &GraphRows::default()).unwrap();
        assert_eq!(db.memories_to_embed("minilm", 10).unwrap().len(), 1);

        db.unindex_memory("b.md").unwrap();
        let left = db.nearest_memories(&axis(1), "minilm", 5).unwrap();
        assert!(left.iter().all(|(m, _)| m.path != "b.md"));
    }
}
