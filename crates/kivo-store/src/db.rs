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

    pub fn schema_version(&self) -> Result<usize, DbError> {
        current_version(&self.conn)
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }
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
    const UNSCOPED_TABLES: &[&str] = &["app_meta", "profiles"];

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
            if is_virtual {
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
