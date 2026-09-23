//! MCP servers and skills (M6): what the user set up, and what they decided. An MCP server is
//! kept as a JSON body (its transport, source and per-tool decisions; secrets are only names of
//! Credential Manager entries). A skill is a folder with a `SKILL.md`, kept where it is; the row
//! says where, where it came from, and whether it was reviewed and is on.

use crate::brains::now_ms;
use crate::db::{Database, DbError};
use rusqlite::{OptionalExtension, params};

/// A skill as the store keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredSkill {
    pub id: String,
    pub name: String,
    /// The skill's folder.
    pub path: String,
    /// `kivo`, `claude-code`, `project:<name>`, `import`.
    pub source: String,
    pub enabled: bool,
    pub reviewed: bool,
    pub updated_at: i64,
}

impl Database {
    // ---- MCP servers ------------------------------------------------------------------------

    /// Adds or replaces a server's JSON body.
    pub fn save_mcp_server(&self, id: &str, body: &str) -> Result<(), DbError> {
        let owner = self.owner()?;
        self.connection().execute(
            "INSERT INTO mcp_servers (id, profile_id, body, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (profile_id, id) DO UPDATE SET body = excluded.body,
                                                        updated_at = excluded.updated_at",
            params![id, owner, body, now_ms()],
        )?;
        Ok(())
    }

    /// Every server's (id, body), by id.
    pub fn mcp_servers(&self) -> Result<Vec<(String, String)>, DbError> {
        let owner = self.owner()?;
        let mut stmt = self
            .connection()
            .prepare("SELECT id, body FROM mcp_servers WHERE profile_id = ?1 ORDER BY id")?;
        let rows = stmt
            .query_map(params![owner], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    pub fn mcp_server(&self, id: &str) -> Result<Option<String>, DbError> {
        let owner = self.owner()?;
        Ok(self
            .connection()
            .query_row(
                "SELECT body FROM mcp_servers WHERE profile_id = ?1 AND id = ?2",
                params![owner, id],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn delete_mcp_server(&self, id: &str) -> Result<bool, DbError> {
        let owner = self.owner()?;
        Ok(self.connection().execute(
            "DELETE FROM mcp_servers WHERE profile_id = ?1 AND id = ?2",
            params![owner, id],
        )? > 0)
    }

    // ---- Skills -----------------------------------------------------------------------------

    /// Adds a skill or updates where it is; `enabled` and `reviewed` are kept for a known one.
    pub fn upsert_skill(&self, s: &StoredSkill) -> Result<(), DbError> {
        let owner = self.owner()?;
        self.connection().execute(
            "INSERT INTO skills (id, profile_id, name, path, source, enabled, reviewed, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT (profile_id, id) DO UPDATE SET name = excluded.name,
                                                        path = excluded.path,
                                                        source = excluded.source,
                                                        updated_at = excluded.updated_at",
            params![
                s.id,
                owner,
                s.name,
                s.path,
                s.source,
                s.enabled,
                s.reviewed,
                now_ms()
            ],
        )?;
        Ok(())
    }

    pub fn skills(&self) -> Result<Vec<StoredSkill>, DbError> {
        let owner = self.owner()?;
        let mut stmt = self.connection().prepare(
            "SELECT id, name, path, source, enabled, reviewed, updated_at FROM skills
             WHERE profile_id = ?1 ORDER BY name",
        )?;
        let rows = stmt
            .query_map(params![owner], |r| {
                Ok(StoredSkill {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    path: r.get(2)?,
                    source: r.get(3)?,
                    enabled: r.get(4)?,
                    reviewed: r.get(5)?,
                    updated_at: r.get(6)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Switches a skill on or off; switching on also marks it reviewed (the user saw it first).
    pub fn set_skill_enabled(&self, id: &str, on: bool) -> Result<bool, DbError> {
        let owner = self.owner()?;
        Ok(self.connection().execute(
            "UPDATE skills SET enabled = ?3, reviewed = CASE WHEN ?3 THEN 1 ELSE reviewed END,
                    updated_at = ?4
             WHERE profile_id = ?1 AND id = ?2",
            params![owner, id, on, now_ms()],
        )? > 0)
    }

    pub fn delete_skill(&self, id: &str) -> Result<bool, DbError> {
        let owner = self.owner()?;
        Ok(self.connection().execute(
            "DELETE FROM skills WHERE profile_id = ?1 AND id = ?2",
            params![owner, id],
        )? > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn servers_and_skills_are_kept() {
        let db = Database::in_memory().unwrap();
        db.save_mcp_server("gh", r#"{"id":"gh"}"#).unwrap();
        db.save_mcp_server("gh", r#"{"id":"gh","enabled":true}"#)
            .unwrap();
        db.save_mcp_server("fs", "{}").unwrap();
        let ids: Vec<String> = db
            .mcp_servers()
            .unwrap()
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(ids, ["fs", "gh"]);
        assert_eq!(
            db.mcp_server("gh").unwrap().as_deref(),
            Some(r#"{"id":"gh","enabled":true}"#)
        );
        assert!(db.delete_mcp_server("fs").unwrap());
        assert!(!db.delete_mcp_server("fs").unwrap());

        let skill = StoredSkill {
            id: "claude-code:pdf".into(),
            name: "pdf".into(),
            path: "C:\\Users\\me\\.claude\\skills\\pdf".into(),
            source: "claude-code".into(),
            enabled: false,
            reviewed: false,
            updated_at: 0,
        };
        db.upsert_skill(&skill).unwrap();
        assert!(db.set_skill_enabled("claude-code:pdf", true).unwrap());
        // Found again: the user's decision stays.
        db.upsert_skill(&skill).unwrap();
        let s = &db.skills().unwrap()[0];
        assert!(s.enabled && s.reviewed);
        assert!(db.delete_skill("claude-code:pdf").unwrap());
        assert!(db.skills().unwrap().is_empty());
    }
}
