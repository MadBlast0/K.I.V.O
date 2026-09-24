//! The SQLite database (ARCHITECTURE §5): WAL mode, foreign keys on, migrations embedded in the
//! binary and run forward only, with a consistent backup taken before any upgrade (DISTRIBUTION §2).
//!
//! Every table that holds a person's data carries `profile_id` (UX §8.2), so people profiles
//! later need no change of meaning. A test checks the real schema for it, so a
//! migration that forgets it fails CI rather than the user's app.

use kivo_core::ProfileId;
use rusqlite::{Connection, OptionalExtension, params};
use rusqlite_migration::{M, Migrations};
use std::path::Path;
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Schema migrations, oldest first; the schema version is the number applied. Features add their
/// own tables in later migrations. Never edit a migration that has shipped.
const SCHEMA: &[&str] = &[
    // 1: app metadata and people profiles.
    "CREATE TABLE app_meta (
             key   TEXT PRIMARY KEY,
             value TEXT NOT NULL
         ) STRICT;
         CREATE TABLE profiles (
             id         TEXT PRIMARY KEY,
             name       TEXT NOT NULL,
             is_owner   INTEGER NOT NULL DEFAULT 0 CHECK (is_owner IN (0, 1)),
             created_at INTEGER NOT NULL
         ) STRICT;
     CREATE UNIQUE INDEX one_owner ON profiles (is_owner) WHERE is_owner = 1;",
    // 2: benchmark results (BENCHMARKS §1). About the machine, not a person.
    "CREATE TABLE benchmarks (
             id         INTEGER PRIMARY KEY,
             suite      TEXT NOT NULL,
             machine    TEXT NOT NULL,
             started_at INTEGER NOT NULL,
             result     TEXT NOT NULL CHECK (json_valid(result))
         ) STRICT;
     CREATE INDEX benchmarks_by_suite ON benchmarks (suite, started_at);",
    // 3: turns and their timings, the Activity timeline, the hash-chained audit log and
    // permission grants (ARCHITECTURE §4.1, §4.3; SECURITY §2, §7).
    "CREATE TABLE turns (
             id         TEXT PRIMARY KEY,
             profile_id TEXT NOT NULL REFERENCES profiles(id),
             started_at INTEGER NOT NULL,
             ended_at   INTEGER,
             source     TEXT NOT NULL,
             transcript TEXT,
             route      TEXT,
             outcome    TEXT,
             reply      TEXT
         ) STRICT;
     CREATE INDEX turns_by_time ON turns (started_at);
     CREATE TABLE turn_metrics (
             turn_id    TEXT PRIMARY KEY REFERENCES turns(id) ON DELETE CASCADE,
             profile_id TEXT NOT NULL REFERENCES profiles(id),
             spans      TEXT NOT NULL CHECK (json_valid(spans))
         ) STRICT;
     CREATE TABLE activity (
             id         INTEGER PRIMARY KEY,
             profile_id TEXT NOT NULL REFERENCES profiles(id),
             ts         INTEGER NOT NULL,
             turn_id    TEXT,
             task_id    TEXT,
             kind       TEXT NOT NULL,
             title      TEXT NOT NULL,
             detail     TEXT,
             status     TEXT NOT NULL,
             data       TEXT CHECK (data IS NULL OR json_valid(data))
         ) STRICT;
     CREATE INDEX activity_by_time ON activity (ts);
     CREATE TABLE audit (
             id           INTEGER PRIMARY KEY,
             profile_id   TEXT NOT NULL REFERENCES profiles(id),
             ts           INTEGER NOT NULL,
             turn_id      TEXT,
             task_id      TEXT,
             tool         TEXT NOT NULL,
             args_summary TEXT NOT NULL,
             risk         TEXT NOT NULL,
             decision     TEXT NOT NULL,
             confirmed_by TEXT,
             result       TEXT,
             error        TEXT,
             prev_hash    TEXT NOT NULL,
             hash         TEXT NOT NULL UNIQUE
         ) STRICT;
     CREATE TRIGGER audit_is_append_only_update BEFORE UPDATE ON audit
         BEGIN SELECT RAISE(ABORT, 'the audit log is append-only'); END;
     CREATE TRIGGER audit_is_append_only_delete BEFORE DELETE ON audit
         BEGIN SELECT RAISE(ABORT, 'the audit log is append-only'); END;
     CREATE TABLE permissions_grants (
             id         INTEGER PRIMARY KEY,
             profile_id TEXT NOT NULL REFERENCES profiles(id),
             tool       TEXT NOT NULL,
             scope      TEXT,
             created_at INTEGER NOT NULL,
             expires_at INTEGER
         ) STRICT;",
    // 4: wake words (VOICE §4, VOICE-15). The built-in "Hey Kivo" can be disabled, not deleted.
    "CREATE TABLE wake_words (
             id          TEXT PRIMARY KEY,
             profile_id  TEXT NOT NULL REFERENCES profiles(id),
             phrase      TEXT NOT NULL,
             phonetic    TEXT,
             engine      TEXT NOT NULL CHECK (engine IN ('kws', 'trained')),
             model_path  TEXT,
             enabled     INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
             built_in    INTEGER NOT NULL DEFAULT 0 CHECK (built_in IN (0, 1)),
             sensitivity REAL NOT NULL DEFAULT 0.5 CHECK (sensitivity BETWEEN 0 AND 1),
             samples     TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(samples)),
             verifier    BLOB,
             quality     TEXT NOT NULL CHECK (quality IN ('good', 'fair', 'risky')),
             fa_test     TEXT CHECK (fa_test IS NULL OR json_valid(fa_test)),
             created_at  INTEGER NOT NULL
         ) STRICT;",
    // 5: voice enrollment (VOICE §5, SECURITY §5): the recorded clips (the files are encrypted
    // on disk; rows point at them) and the speaker profile, whose embeddings are stored
    // encrypted too. Deleting voice data removes both.
    "CREATE TABLE voice_clips (
             id         INTEGER PRIMARY KEY,
             profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
             prompt     TEXT NOT NULL,
             file       TEXT NOT NULL,
             created_at INTEGER NOT NULL
         ) STRICT;
     CREATE TABLE speaker_profiles (
             profile_id TEXT PRIMARY KEY REFERENCES profiles(id) ON DELETE CASCADE,
             model_id   TEXT NOT NULL,
             threshold  REAL NOT NULL,
             embeddings BLOB NOT NULL,
             created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL
         ) STRICT;",
    // 6: brains (M3). Conversations and their messages with full-text recall (MEM-01, CONV-01/06),
    // stated preferences (MEM-03), metered usage (BRAIN-34), CLI agent sessions to resume
    // (CONV-02), "that's not what I meant" reports (BRAIN-06), the user's own words for
    // recognition and repair (VOICE-23), and the discovery cache (DISC-02, about the machine).
    "CREATE TABLE conversations (
             id           TEXT PRIMARY KEY,
             profile_id   TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
             kind         TEXT NOT NULL CHECK (kind IN ('voice', 'chat')),
             title        TEXT NOT NULL DEFAULT '',
             summary      TEXT NOT NULL DEFAULT '',
             summarized   INTEGER NOT NULL DEFAULT 0,
             brain        TEXT,
             pinned       INTEGER NOT NULL DEFAULT 0 CHECK (pinned IN (0, 1)),
             created_at   INTEGER NOT NULL,
             updated_at   INTEGER NOT NULL
         ) STRICT;
     CREATE INDEX conversations_by_time ON conversations (updated_at);
     CREATE TABLE messages (
             id              INTEGER PRIMARY KEY,
             conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
             profile_id      TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
             ts              INTEGER NOT NULL,
             role            TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'tool')),
             text            TEXT NOT NULL,
             brain           TEXT,
             turn_id         TEXT,
             data            TEXT CHECK (data IS NULL OR json_valid(data))
         ) STRICT;
     CREATE INDEX messages_by_conversation ON messages (conversation_id, id);
     CREATE VIRTUAL TABLE messages_fts USING fts5 (text, content='messages', content_rowid='id');
     CREATE TRIGGER messages_fts_insert AFTER INSERT ON messages BEGIN
         INSERT INTO messages_fts (rowid, text) VALUES (new.id, new.text);
     END;
     CREATE TRIGGER messages_fts_delete AFTER DELETE ON messages BEGIN
         INSERT INTO messages_fts (messages_fts, rowid, text) VALUES ('delete', old.id, old.text);
     END;
     CREATE TABLE preferences (
             profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
             key        TEXT NOT NULL,
             value      TEXT NOT NULL,
             source     TEXT NOT NULL,
             updated_at INTEGER NOT NULL,
             PRIMARY KEY (profile_id, key)
         ) STRICT;
     CREATE TABLE usage (
             id            INTEGER PRIMARY KEY,
             profile_id    TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
             ts            INTEGER NOT NULL,
             kind          TEXT NOT NULL CHECK (kind IN ('brain', 'stt', 'tts', 'realtime', 'computerUse', 'agent')),
             provider      TEXT NOT NULL,
             model         TEXT NOT NULL,
             brain_profile TEXT,
             input_tokens  INTEGER NOT NULL DEFAULT 0,
             output_tokens INTEGER NOT NULL DEFAULT 0,
             cached_tokens INTEGER NOT NULL DEFAULT 0,
             audio_seconds REAL NOT NULL DEFAULT 0,
             images        INTEGER NOT NULL DEFAULT 0,
             requests      INTEGER NOT NULL DEFAULT 1,
             cost          REAL,
             turn_id       TEXT,
             task_id       TEXT,
             routine_id    TEXT
         ) STRICT;
     CREATE INDEX usage_by_time ON usage (ts);
     CREATE TABLE agent_sessions (
             id              TEXT PRIMARY KEY,
             profile_id      TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
             agent           TEXT NOT NULL,
             workspace       TEXT NOT NULL,
             conversation_id TEXT,
             created_at      INTEGER NOT NULL,
             last_used       INTEGER NOT NULL
         ) STRICT;
     CREATE TABLE misroutes (
             id         INTEGER PRIMARY KEY,
             profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
             ts         INTEGER NOT NULL,
             turn_id    TEXT NOT NULL,
             transcript TEXT NOT NULL,
             route      TEXT NOT NULL,
             note       TEXT NOT NULL DEFAULT ''
         ) STRICT;
     CREATE TABLE user_vocabulary (
             profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
             word       TEXT NOT NULL,
             uses       INTEGER NOT NULL DEFAULT 1,
             PRIMARY KEY (profile_id, word)
         ) STRICT;
     CREATE TABLE discovery (
             section    TEXT NOT NULL,
             id         TEXT NOT NULL,
             data       TEXT NOT NULL CHECK (json_valid(data)),
             checked_at INTEGER NOT NULL,
             first_seen INTEGER NOT NULL,
             viewed     INTEGER NOT NULL DEFAULT 0 CHECK (viewed IN (0, 1)),
             PRIMARY KEY (section, id)
         ) STRICT;
     -- The brain's hidden reasoning, only when the user turns the reasoning log on (BRAIN-09).
     ALTER TABLE turns ADD COLUMN reasoning TEXT;",
    // 7: grants scoped by an argument pattern, and grants for this session only (SEC-08).
    "ALTER TABLE permissions_grants ADD COLUMN pattern TEXT;
     ALTER TABLE permissions_grants ADD COLUMN session TEXT;",
    // 8: tasks and their steps (ARCH-27, MEM-02), routines (ROUT-01), workspaces and
    // instructions (CONV-09/10).
    "CREATE TABLE tasks (
             id          TEXT PRIMARY KEY,
             profile_id  TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
             title       TEXT NOT NULL,
             kind        TEXT NOT NULL,
             owner       TEXT NOT NULL,
             status      TEXT NOT NULL,
             spec        TEXT NOT NULL CHECK (json_valid(spec)),
             result      TEXT,
             error       TEXT,
             summary     TEXT,
             routine_id  TEXT,
             turn_id     TEXT,
             created_at  INTEGER NOT NULL,
             updated_at  INTEGER NOT NULL,
             finished_at INTEGER
         ) STRICT;
     CREATE INDEX tasks_by_status ON tasks (status, updated_at);
     CREATE TABLE task_steps (
             task_id     TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
             profile_id  TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
             step_id     TEXT NOT NULL,
             position    INTEGER NOT NULL,
             title       TEXT NOT NULL,
             status      TEXT NOT NULL,
             detail      TEXT,
             result      TEXT,
             attempts    INTEGER NOT NULL DEFAULT 0,
             started_at  INTEGER,
             finished_at INTEGER,
             PRIMARY KEY (task_id, step_id)
         ) STRICT;
     CREATE TABLE routines (
             id         TEXT PRIMARY KEY,
             profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
             name       TEXT NOT NULL,
             enabled    INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
             version    INTEGER NOT NULL,
             body       TEXT NOT NULL CHECK (json_valid(body)),
             starter    TEXT,
             created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL,
             last_run   INTEGER
         ) STRICT;
     CREATE UNIQUE INDEX routines_one_starter ON routines (profile_id, starter) WHERE starter IS NOT NULL;
     CREATE TABLE workspaces (
             id              TEXT PRIMARY KEY,
             profile_id      TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
             path            TEXT NOT NULL COLLATE NOCASE,
             name            TEXT NOT NULL,
             remembered      INTEGER NOT NULL CHECK (remembered IN (0, 1)),
             preferred_agent TEXT,
             agent_mode      TEXT,
             created_at      INTEGER NOT NULL,
             last_used       INTEGER NOT NULL
         ) STRICT;
     CREATE UNIQUE INDEX workspaces_by_path ON workspaces (profile_id, path);
     CREATE TABLE instructions (
             profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
             scope      TEXT NOT NULL,
             text       TEXT NOT NULL,
             updated_at INTEGER NOT NULL,
             PRIMARY KEY (profile_id, scope)
         ) STRICT;",
    // 9 · M6: MCP servers (their setup and the user's tool decisions, as JSON; secrets live in
    // Credential Manager) and skills (where each one is, and whether it's reviewed and on).
    "CREATE TABLE mcp_servers (
             id         TEXT NOT NULL,
             profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
             body       TEXT NOT NULL,
             updated_at INTEGER NOT NULL,
             PRIMARY KEY (profile_id, id)
         ) STRICT;
     CREATE TABLE skills (
             id         TEXT NOT NULL,
             profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
             name       TEXT NOT NULL,
             path       TEXT NOT NULL,
             source     TEXT NOT NULL,
             enabled    INTEGER NOT NULL CHECK (enabled IN (0, 1)),
             reviewed   INTEGER NOT NULL CHECK (reviewed IN (0, 1)),
             updated_at INTEGER NOT NULL,
             PRIMARY KEY (profile_id, id)
         ) STRICT;",
    // 10 · M7: the memory index (MEM-04, CONV-17/18). The Markdown vault is the source of truth;
    // these rows are rebuilt from its files (only use counts live here alone). The graph
    // (entities, relations, observations with validity windows), links for backlinks, KIVO's
    // suggestions waiting for the user (CONV-20), and which memories each turn used (MEM-10).
    "CREATE TABLE memories (
             id           INTEGER PRIMARY KEY,
             profile_id   TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
             path         TEXT NOT NULL,
             kind         TEXT NOT NULL,
             title        TEXT NOT NULL,
             text         TEXT NOT NULL,
             tags         TEXT NOT NULL CHECK (json_valid(tags)),
             workspace    TEXT,
             scope        TEXT NOT NULL,
             sensitivity  TEXT NOT NULL,
             share_cloud  INTEGER NOT NULL CHECK (share_cloud IN (0, 1)),
             source       TEXT,
             created_at   INTEGER NOT NULL,
             updated_at   INTEGER NOT NULL,
             valid_until  INTEGER,
             last_used_at INTEGER,
             use_count    INTEGER NOT NULL DEFAULT 0,
             hash         TEXT NOT NULL
         ) STRICT;
     CREATE UNIQUE INDEX memories_by_path ON memories (profile_id, path);
     CREATE VIRTUAL TABLE memories_fts USING fts5 (title, text, tags, content='memories', content_rowid='id');
     CREATE TRIGGER memories_fts_insert AFTER INSERT ON memories BEGIN
         INSERT INTO memories_fts (rowid, title, text, tags) VALUES (new.id, new.title, new.text, new.tags);
     END;
     CREATE TRIGGER memories_fts_delete AFTER DELETE ON memories BEGIN
         INSERT INTO memories_fts (memories_fts, rowid, title, text, tags)
             VALUES ('delete', old.id, old.title, old.text, old.tags);
     END;
     CREATE TRIGGER memories_fts_update AFTER UPDATE OF title, text, tags ON memories BEGIN
         INSERT INTO memories_fts (memories_fts, rowid, title, text, tags)
             VALUES ('delete', old.id, old.title, old.text, old.tags);
         INSERT INTO memories_fts (rowid, title, text, tags) VALUES (new.id, new.title, new.text, new.tags);
     END;
     CREATE TABLE memory_links (
             profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
             memory_id  INTEGER NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
             target     TEXT NOT NULL COLLATE NOCASE
         ) STRICT;
     CREATE INDEX memory_links_by_target ON memory_links (profile_id, target);
     CREATE TABLE entities (
             id         INTEGER PRIMARY KEY,
             profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
             name       TEXT NOT NULL COLLATE NOCASE,
             kind       TEXT NOT NULL,
             UNIQUE (profile_id, name)
         ) STRICT;
     CREATE TABLE relations (
             id         INTEGER PRIMARY KEY,
             profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
             from_id    INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
             kind       TEXT NOT NULL,
             to_id      INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
             memory_id  INTEGER NOT NULL REFERENCES memories(id) ON DELETE CASCADE
         ) STRICT;
     CREATE TABLE observations (
             id         INTEGER PRIMARY KEY,
             profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
             entity_id  INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
             text       TEXT NOT NULL,
             memory_id  INTEGER NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
             valid_from INTEGER NOT NULL,
             valid_to   INTEGER
         ) STRICT;
     CREATE INDEX observations_by_entity ON observations (entity_id);
     CREATE TABLE memory_suggestions (
             id         INTEGER PRIMARY KEY,
             profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
             text       TEXT NOT NULL,
             reason     TEXT,
             source     TEXT,
             workspace  TEXT,
             created_at INTEGER NOT NULL,
             state      TEXT NOT NULL CHECK (state IN ('pending', 'accepted', 'dismissed'))
         ) STRICT;
     CREATE TABLE turn_memories (
             profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
             turn_id    TEXT NOT NULL,
             path       TEXT NOT NULL,
             title      TEXT NOT NULL,
             PRIMARY KEY (profile_id, turn_id, path)
         ) STRICT;",
    // Hybrid memory search (MEM-09, CONV-23): one 384-dimension vector per note in sqlite-vec,
    // keyed by the note's row id, with the model that made it. A changed or deleted note loses
    // its vector, and the runtime embeds it again in the background.
    "CREATE VIRTUAL TABLE memory_vectors USING vec0 (embedding float[384] distance_metric=cosine);
     CREATE TABLE memory_vector_models (
             profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
             memory_id  INTEGER PRIMARY KEY REFERENCES memories(id) ON DELETE CASCADE,
             model      TEXT NOT NULL
         ) STRICT;
     CREATE TRIGGER memory_vectors_delete AFTER DELETE ON memories BEGIN
         DELETE FROM memory_vectors WHERE rowid = old.id;
     END;
     CREATE TRIGGER memory_vectors_stale AFTER UPDATE OF title, text ON memories
         WHEN old.title IS NOT new.title OR old.text IS NOT new.text BEGIN
         DELETE FROM memory_vector_models WHERE memory_id = new.id;
     END;",
    // Conversation recall by meaning (CONV-06): one vector per kept message, like the notes'.
    "CREATE VIRTUAL TABLE message_vectors USING vec0 (embedding float[384] distance_metric=cosine);
     CREATE TABLE message_vector_models (
             profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
             message_id INTEGER PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
             model      TEXT NOT NULL
         ) STRICT;
     CREATE TRIGGER message_vectors_delete AFTER DELETE ON messages BEGIN
         DELETE FROM message_vectors WHERE rowid = old.id;
     END;",
];

static MIGRATIONS: LazyLock<Migrations<'static>> =
    LazyLock::new(|| Migrations::new(SCHEMA.iter().copied().map(M::up).collect()));

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("database upgrade failed: {0}")]
    Migration(#[from] rusqlite_migration::Error),
    #[error("couldn't prepare the data folder: {0}")]
    Io(#[from] std::io::Error),
}

/// An open database with the schema up to date.
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Opens (or creates) the database at `path`, backs it up if an upgrade is due, migrates it,
    /// and makes sure the owner profile exists.
    pub fn open(path: &Path) -> Result<Self, DbError> {
        register_vectors();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut conn = Connection::open(path)?;
        configure(&conn)?;
        let current = current_version(&conn)?;
        if current > 0 && current < target_version() {
            backup(&conn, path, current)?;
        }
        MIGRATIONS.to_latest(&mut conn)?;
        let db = Self { conn };
        db.owner_profile()?;
        Ok(db)
    }

    /// An in-memory database with the full schema (tests).
    pub fn in_memory() -> Result<Self, DbError> {
        register_vectors();
        let mut conn = Connection::open_in_memory()?;
        configure(&conn)?;
        MIGRATIONS.to_latest(&mut conn)?;
        let db = Self { conn };
        db.owner_profile()?;
        Ok(db)
    }

    /// The owner's profile id, created on first use (UX §8.2: one owner plus guest in the MVP).
    pub fn owner_profile(&self) -> Result<ProfileId, DbError> {
        let existing: Option<String> = self
            .conn
            .query_row("SELECT id FROM profiles WHERE is_owner = 1", [], |r| {
                r.get(0)
            })
            .optional()?;
        if let Some(id) = existing.and_then(|s| s.parse().ok()) {
            return Ok(id);
        }
        let id = ProfileId::new();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        self.conn.execute(
            "INSERT INTO profiles (id, name, is_owner, created_at) VALUES (?1, 'Owner', 1, ?2)",
            params![id.to_string(), i64::try_from(now).unwrap_or(i64::MAX)],
        )?;
        Ok(id)
    }

    /// Stores one benchmark suite's result (`kivo-bench`): the suite, the machine id, when it
    /// started (Unix milliseconds) and the full result as JSON.
    pub fn record_benchmark(
        &self,
        suite: &str,
        machine: &str,
        started_at_ms: i64,
        result_json: &str,
    ) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO benchmarks (suite, machine, started_at, result) VALUES (?1, ?2, ?3, ?4)",
            params![suite, machine, started_at_ms, result_json],
        )?;
        Ok(())
    }

    /// The latest result of `suite` measured on this PC as it is (not an emulated tier): when it
    /// started and its JSON (VOICE-42 reads the speech suites for the engine registry).
    pub fn latest_benchmark(&self, suite: &str) -> Result<Option<(i64, String)>, DbError> {
        Ok(self
            .conn
            .query_row(
                "SELECT started_at, result FROM benchmarks
                 WHERE suite = ?1 AND machine NOT LIKE '%-emulated-%'
                 ORDER BY started_at DESC LIMIT 1",
                params![suite],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    pub fn schema_version(&self) -> Result<usize, DbError> {
        current_version(&self.conn)
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub(crate) fn connection_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }
}

/// Makes sqlite-vec available to every connection opened after this (once per process).
fn register_vectors() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // SAFETY: `sqlite3_vec_init` is sqlite-vec's extension entry point, with the signature
        // `sqlite3_auto_extension` expects; registering it has no other effect.
        unsafe {
            #[allow(
                clippy::missing_transmute_annotations,
                reason = "the FFI entry point's type"
            )]
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }
    });
}

fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(std::time::Duration::from_secs(5))
}

fn current_version(conn: &Connection) -> Result<usize, DbError> {
    let v: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    Ok(usize::try_from(v).unwrap_or(0))
}

fn target_version() -> usize {
    SCHEMA.len()
}

/// A consistent copy of the whole database next to it, before an upgrade.
fn backup(conn: &Connection, path: &Path, version: usize) -> Result<(), DbError> {
    let target = path.with_extension(format!("db.v{version}.bak"));
    if target.exists() {
        std::fs::remove_file(&target)?;
    }
    conn.execute("VACUUM INTO ?1", params![target.to_string_lossy()])?;
    tracing::info!(to = %target.display(), "database backed up before upgrade");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tables that are about the app itself rather than a person.
    const UNSCOPED_TABLES: &[&str] = &["app_meta", "profiles", "benchmarks", "discovery"];

    /// Tables holding personal data that lack a `profile_id` column tied to `profiles`.
    fn unscoped_tables(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
            .unwrap();
        let tables: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        let mut unscoped = Vec::new();
        for table in tables
            .iter()
            .filter(|t| !UNSCOPED_TABLES.contains(&t.as_str()))
        {
            // Virtual FTS tables are indexes over a scoped table, not records of their own.
            let is_virtual: bool = conn
                .query_row(
                    "SELECT sql LIKE 'CREATE VIRTUAL%' FROM sqlite_schema WHERE name = ?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            // FTS5 keeps its index in shadow tables named `<virtual table>_…`.
            let is_shadow: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM sqlite_schema
                     WHERE sql LIKE 'CREATE VIRTUAL%' AND substr(?1, 1, length(name) + 1) = name || '_'",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            if is_virtual || is_shadow {
                continue;
            }
            let fk: Option<String> = conn
                .query_row(
                    "SELECT \"table\" FROM pragma_foreign_key_list(?1) WHERE \"from\" = 'profile_id'",
                    [table],
                    |r| r.get(0),
                )
                .optional()
                .unwrap();
            if fk.as_deref() != Some("profiles") {
                unscoped.push(table.clone());
            }
        }
        unscoped
    }

    #[test]
    fn a_new_database_is_migrated_in_wal_mode_with_an_owner() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data").join("kivo.db");
        let db = Database::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), target_version());
        let mode: String = db
            .connection()
            .pragma_query_value(None, "journal_mode", |r| r.get(0))
            .unwrap();
        assert_eq!(mode.to_lowercase(), "wal");
        let fk: i64 = db
            .connection()
            .pragma_query_value(None, "foreign_keys", |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1);
        let owner = db.owner_profile().unwrap();
        drop(db);
        assert_eq!(
            Database::open(&path).unwrap().owner_profile().unwrap(),
            owner,
            "stable across opens"
        );
    }

    #[test]
    fn only_one_owner_can_exist() {
        let db = Database::in_memory().unwrap();
        let err = db.connection().execute(
            "INSERT INTO profiles (id, name, is_owner, created_at) VALUES ('x', 'Second', 1, 0)",
            [],
        );
        assert!(err.is_err());
    }

    #[test]
    fn the_shipped_schema_is_profile_scoped() {
        assert_eq!(
            unscoped_tables(Database::in_memory().unwrap().connection()),
            Vec::<String>::new()
        );
    }

    #[test]
    fn the_scoping_check_catches_a_table_without_profile_id() {
        let db = Database::in_memory().unwrap();
        let c = db.connection();
        c.execute_batch(
            "CREATE TABLE good (id INTEGER PRIMARY KEY, profile_id TEXT NOT NULL REFERENCES profiles(id)) STRICT;
             CREATE TABLE bad (id INTEGER PRIMARY KEY, note TEXT) STRICT;",
        )
        .unwrap();
        assert_eq!(unscoped_tables(c), ["bad"]);
    }

    #[test]
    fn a_backup_is_a_complete_readable_copy() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kivo.db");
        let db = Database::open(&path).unwrap();
        db.connection()
            .execute("INSERT INTO app_meta VALUES ('marker', 'keep me')", [])
            .unwrap();
        backup(db.connection(), &path, 1).unwrap();
        let copy = Connection::open(path.with_extension("db.v1.bak")).unwrap();
        let kept: String = copy
            .query_row("SELECT value FROM app_meta WHERE key = 'marker'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(kept, "keep me");
        let owners: i64 = copy
            .query_row(
                "SELECT count(*) FROM profiles WHERE is_owner = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(owners, 1);
    }

    #[test]
    fn benchmark_results_are_stored_as_valid_json() {
        let db = Database::in_memory().unwrap();
        db.record_benchmark("ipc", "amd-ryzen-7-6800h", 1, r#"{"p50":1.0}"#)
            .unwrap();
        let (suite, result): (String, String) = db
            .connection()
            .query_row("SELECT suite, result FROM benchmarks", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((suite.as_str(), result.as_str()), ("ipc", r#"{"p50":1.0}"#));
        assert!(db.record_benchmark("ipc", "m", 1, "not json").is_err());
        db.record_benchmark("ipc", "pc", 5, r#"{"p50":2.0}"#)
            .unwrap();
        db.record_benchmark("ipc", "pc-emulated-low", 9, r#"{"p50":3.0}"#)
            .unwrap();
        assert_eq!(
            db.latest_benchmark("ipc").unwrap(),
            Some((5, r#"{"p50":2.0}"#.to_owned())),
            "the newest run on this PC as it is"
        );
        assert_eq!(db.latest_benchmark("stt").unwrap(), None);
    }

    #[test]
    fn a_database_from_a_newer_kivo_is_not_downgraded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kivo.db");
        Connection::open(&path)
            .unwrap()
            .pragma_update(None, "user_version", 99)
            .unwrap();
        assert!(Database::open(&path).is_err());
    }
}
