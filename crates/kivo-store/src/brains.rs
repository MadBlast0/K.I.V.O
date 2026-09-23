//! What the brains keep (MEMORY §1, CONVERSATION §1–2, BRAINS §9): conversations and their
//! messages, searchable and pruned by the retention setting; stated preferences; metered usage;
//! CLI agent sessions to resume; misroute reports; the user's vocabulary; and the discovery cache.

use crate::db::{Database, DbError};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Unix milliseconds.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    /// `voice` (the Island) or `chat` (typed in Chat).
    pub kind: String,
    pub title: String,
    /// The running summary of turns that no longer fit (CONV-06).
    pub summary: String,
    /// How many of its messages the summary covers.
    pub summarized: i64,
    /// The brain it last used.
    pub brain: Option<String>,
    pub pinned: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredMessage {
    pub id: i64,
    pub conversation_id: String,
    pub ts: i64,
    /// `user`, `assistant` or `tool`.
    pub role: String,
    pub text: String,
    pub brain: Option<String>,
    pub turn_id: Option<String>,
}

/// One metered call (BRAIN-34).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRow {
    pub ts: i64,
    /// `brain`, `stt`, `tts`, `realtime`, `computerUse`, `agent`.
    pub kind: String,
    pub provider: String,
    pub model: String,
    pub brain_profile: Option<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
    pub audio_seconds: f64,
    pub images: u32,
    pub requests: u32,
    /// Estimated dollars; `None` when the model has no known price.
    pub cost: Option<f64>,
    pub turn_id: Option<String>,
    pub task_id: Option<String>,
    pub routine_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionRow {
    pub id: String,
    pub agent: String,
    pub workspace: String,
    pub conversation_id: Option<String>,
    pub created_at: i64,
    pub last_used: i64,
}

/// A discovered item (DISC-02), with when it was last checked and whether it's new.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Discovered {
    pub section: String,
    pub id: String,
    pub data: serde_json::Value,
    pub checked_at: i64,
    pub first_seen: i64,
    /// Seen by the user already (no "New" label).
    pub viewed: bool,
}

fn i64_of(n: u64) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

impl Database {
    fn owner(&self) -> Result<String, DbError> {
        Ok(self.owner_profile()?.to_string())
    }

    pub fn create_conversation(
        &self,
        id: &str,
        kind: &str,
        title: &str,
    ) -> Result<Conversation, DbError> {
        let owner = self.owner()?;
        let now = now_ms();
        self.connection().execute(
            "INSERT INTO conversations (id, profile_id, kind, title, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![id, owner, kind, title, now],
        )?;
        Ok(Conversation {
            id: id.into(),
            kind: kind.into(),
            title: title.into(),
            summary: String::new(),
            summarized: 0,
            brain: None,
            pinned: false,
            created_at: now,
            updated_at: now,
        })
    }

    fn conversation_from(r: &rusqlite::Row<'_>) -> rusqlite::Result<Conversation> {
        Ok(Conversation {
            id: r.get(0)?,
            kind: r.get(1)?,
            title: r.get(2)?,
            summary: r.get(3)?,
            summarized: r.get(4)?,
            brain: r.get(5)?,
            pinned: r.get::<_, i64>(6)? == 1,
            created_at: r.get(7)?,
            updated_at: r.get(8)?,
        })
    }

    const CONVERSATION_COLUMNS: &str =
        "id, kind, title, summary, summarized, brain, pinned, created_at, updated_at";

    pub fn conversation(&self, id: &str) -> Result<Option<Conversation>, DbError> {
        Ok(self
            .connection()
            .query_row(
                &format!(
                    "SELECT {} FROM conversations WHERE id = ?1",
                    Self::CONVERSATION_COLUMNS
                ),
                [id],
                Self::conversation_from,
            )
            .optional()?)
    }

    /// Newest first; pinned ones first of all.
    pub fn conversations(&self, limit: u32) -> Result<Vec<Conversation>, DbError> {
        let owner = self.owner()?;
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {} FROM conversations WHERE profile_id = ?1
             ORDER BY pinned DESC, updated_at DESC LIMIT ?2",
            Self::CONVERSATION_COLUMNS
        ))?;
        let rows = stmt
            .query_map(params![owner, limit], Self::conversation_from)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// The most recent conversation of `kind` updated since `since` (CONV-01: voice sessions
    /// within 30 minutes join the same thread).
    pub fn latest_conversation(
        &self,
        kind: &str,
        since: i64,
    ) -> Result<Option<Conversation>, DbError> {
        let owner = self.owner()?;
        Ok(self
            .connection()
            .query_row(
                &format!(
                    "SELECT {} FROM conversations WHERE profile_id = ?1 AND kind = ?2 AND updated_at >= ?3
                     ORDER BY updated_at DESC LIMIT 1",
                    Self::CONVERSATION_COLUMNS
                ),
                params![owner, kind, since],
                Self::conversation_from,
            )
            .optional()?)
    }

    pub fn update_conversation(
        &self,
        id: &str,
        title: Option<&str>,
        brain: Option<&str>,
        pinned: Option<bool>,
    ) -> Result<(), DbError> {
        let now = now_ms();
        self.connection().execute(
            "UPDATE conversations SET
                 title = COALESCE(?2, title), brain = COALESCE(?3, brain),
                 pinned = COALESCE(?4, pinned), updated_at = ?5
             WHERE id = ?1",
            params![id, title, brain, pinned.map(i64::from), now],
        )?;
        Ok(())
    }

    /// Stores the running summary and how many messages it covers (CONV-06).
    pub fn set_summary(&self, id: &str, summary: &str, summarized: i64) -> Result<(), DbError> {
        self.connection().execute(
            "UPDATE conversations SET summary = ?2, summarized = ?3 WHERE id = ?1",
            params![id, summary, summarized],
        )?;
        Ok(())
    }

    pub fn delete_conversation(&self, id: &str) -> Result<(), DbError> {
        self.connection()
            .execute("DELETE FROM conversations WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn add_message(
        &self,
        conversation: &str,
        role: &str,
        text: &str,
        brain: Option<&str>,
        turn: Option<&str>,
    ) -> Result<i64, DbError> {
        let owner = self.owner()?;
        let now = now_ms();
        self.connection().execute(
            "INSERT INTO messages (conversation_id, profile_id, ts, role, text, brain, turn_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![conversation, owner, now, role, text, brain, turn],
        )?;
        let id = self.connection().last_insert_rowid();
        self.connection().execute(
            "UPDATE conversations SET updated_at = ?2 WHERE id = ?1",
            params![conversation, now],
        )?;
        Ok(id)
    }

    fn message_from(r: &rusqlite::Row<'_>) -> rusqlite::Result<StoredMessage> {
        Ok(StoredMessage {
            id: r.get(0)?,
            conversation_id: r.get(1)?,
            ts: r.get(2)?,
            role: r.get(3)?,
            text: r.get(4)?,
            brain: r.get(5)?,
            turn_id: r.get(6)?,
        })
    }

    /// A conversation's messages in order.
    pub fn messages(&self, conversation: &str) -> Result<Vec<StoredMessage>, DbError> {
        let mut stmt = self.connection().prepare(
            "SELECT id, conversation_id, ts, role, text, brain, turn_id FROM messages
             WHERE conversation_id = ?1 ORDER BY id",
        )?;
        let rows = stmt
            .query_map([conversation], Self::message_from)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Full-text recall over every kept message (CONV-06, Chat's search).
    pub fn search_messages(&self, query: &str, limit: u32) -> Result<Vec<StoredMessage>, DbError> {
        // Words only, each quoted, so user text can't form FTS syntax.
        let terms: Vec<String> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() > 1)
            .map(|w| format!("\"{w}\""))
            .collect();
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let owner = self.owner()?;
        let mut stmt = self.connection().prepare(
            "SELECT m.id, m.conversation_id, m.ts, m.role, m.text, m.brain, m.turn_id
             FROM messages_fts f JOIN messages m ON m.id = f.rowid
             WHERE messages_fts MATCH ?1 AND m.profile_id = ?2
             ORDER BY rank LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(
                params![terms.join(" OR "), owner, limit],
                Self::message_from,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Removes conversations not updated for `days` (the retention setting; pinned ones stay).
    /// `days == 0` means "never keep": everything unpinned goes.
    pub fn prune_conversations(&self, days: u32, now: i64) -> Result<usize, DbError> {
        let cutoff = if days == 0 {
            i64::MAX
        } else {
            now - i64::from(days) * 86_400_000
        };
        Ok(self.connection().execute(
            "DELETE FROM conversations WHERE pinned = 0 AND updated_at < ?1",
            [cutoff],
        )?)
    }

    pub fn set_preference(&self, key: &str, value: &str, source: &str) -> Result<(), DbError> {
        let owner = self.owner()?;
        self.connection().execute(
            "INSERT INTO preferences (profile_id, key, value, source, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (profile_id, key) DO UPDATE SET value = ?3, source = ?4, updated_at = ?5",
            params![owner, key, value, source, now_ms()],
        )?;
        Ok(())
    }

    pub fn preferences(&self) -> Result<Vec<(String, String)>, DbError> {
        let owner = self.owner()?;
        let mut stmt = self
            .connection()
            .prepare("SELECT key, value FROM preferences WHERE profile_id = ?1 ORDER BY key")?;
        let rows = stmt
            .query_map([owner], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn delete_preference(&self, key: &str) -> Result<(), DbError> {
        let owner = self.owner()?;
        self.connection().execute(
            "DELETE FROM preferences WHERE profile_id = ?1 AND key = ?2",
            params![owner, key],
        )?;
        Ok(())
    }

    pub fn record_usage(&self, u: &UsageRow) -> Result<(), DbError> {
        let owner = self.owner()?;
        self.connection().execute(
            "INSERT INTO usage (profile_id, ts, kind, provider, model, brain_profile, input_tokens,
                 output_tokens, cached_tokens, audio_seconds, images, requests, cost, turn_id, task_id, routine_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                owner,
                u.ts,
                u.kind,
                u.provider,
                u.model,
                u.brain_profile,
                i64_of(u.input_tokens),
                i64_of(u.output_tokens),
                i64_of(u.cached_tokens),
                u.audio_seconds,
                u.images,
                u.requests,
                u.cost,
                u.turn_id,
                u.task_id,
                u.routine_id
            ],
        )?;
        Ok(())
    }

    /// Usage since `since` (Unix ms), oldest first.
    pub fn usage_since(&self, since: i64) -> Result<Vec<UsageRow>, DbError> {
        let owner = self.owner()?;
        let mut stmt = self.connection().prepare(
            "SELECT ts, kind, provider, model, brain_profile, input_tokens, output_tokens, cached_tokens,
                    audio_seconds, images, requests, cost, turn_id, task_id, routine_id
             FROM usage WHERE profile_id = ?1 AND ts >= ?2 ORDER BY ts",
        )?;
        let rows = stmt
            .query_map(params![owner, since], |r| {
                let n = |i: usize| -> rusqlite::Result<u64> {
                    Ok(u64::try_from(r.get::<_, i64>(i)?).unwrap_or(0))
                };
                Ok(UsageRow {
                    ts: r.get(0)?,
                    kind: r.get(1)?,
                    provider: r.get(2)?,
                    model: r.get(3)?,
                    brain_profile: r.get(4)?,
                    input_tokens: n(5)?,
                    output_tokens: n(6)?,
                    cached_tokens: n(7)?,
                    audio_seconds: r.get(8)?,
                    images: r.get(9)?,
                    requests: r.get(10)?,
                    cost: r.get(11)?,
                    turn_id: r.get(12)?,
                    task_id: r.get(13)?,
                    routine_id: r.get(14)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Every stored agent session, most recently used first (the Agents page, UX-25).
    pub fn agent_sessions(&self, limit: u32) -> Result<Vec<AgentSessionRow>, DbError> {
        let owner = self.owner()?;
        let mut stmt = self.connection().prepare(
            "SELECT id, agent, workspace, conversation_id, created_at, last_used FROM agent_sessions
             WHERE profile_id = ?1 ORDER BY last_used DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![owner, limit], |r| {
            Ok(AgentSessionRow {
                id: r.get(0)?,
                agent: r.get(1)?,
                workspace: r.get(2)?,
                conversation_id: r.get(3)?,
                created_at: r.get(4)?,
                last_used: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn save_agent_session(&self, row: &AgentSessionRow) -> Result<(), DbError> {
        let owner = self.owner()?;
        self.connection().execute(
            "INSERT INTO agent_sessions (id, profile_id, agent, workspace, conversation_id, created_at, last_used)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT (id) DO UPDATE SET last_used = ?7, conversation_id = COALESCE(?5, conversation_id)",
            params![row.id, owner, row.agent, row.workspace, row.conversation_id, row.created_at, row.last_used],
        )?;
        Ok(())
    }

    /// The session to resume for `agent` in `workspace` (CONV-02), newest first.
    pub fn agent_session(
        &self,
        agent: &str,
        workspace: &str,
    ) -> Result<Option<AgentSessionRow>, DbError> {
        let owner = self.owner()?;
        Ok(self
            .connection()
            .query_row(
                "SELECT id, agent, workspace, conversation_id, created_at, last_used FROM agent_sessions
                 WHERE profile_id = ?1 AND agent = ?2 AND workspace = ?3 ORDER BY last_used DESC LIMIT 1",
                params![owner, agent, workspace],
                |r| {
                    Ok(AgentSessionRow {
                        id: r.get(0)?,
                        agent: r.get(1)?,
                        workspace: r.get(2)?,
                        conversation_id: r.get(3)?,
                        created_at: r.get(4)?,
                        last_used: r.get(5)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn add_misroute(
        &self,
        turn: &str,
        transcript: &str,
        route: &str,
        note: &str,
    ) -> Result<(), DbError> {
        let owner = self.owner()?;
        self.connection().execute(
            "INSERT INTO misroutes (profile_id, ts, turn_id, transcript, route, note) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![owner, now_ms(), turn, transcript, route, note],
        )?;
        Ok(())
    }

    pub fn misroute_count(&self) -> Result<i64, DbError> {
        Ok(self
            .connection()
            .query_row("SELECT COUNT(*) FROM misroutes", [], |r| r.get(0))?)
    }

    /// Counts a word the user uses (names, jargon), for recognition hints and repair (VOICE-23).
    pub fn add_vocabulary(&self, word: &str) -> Result<(), DbError> {
        let owner = self.owner()?;
        self.connection().execute(
            "INSERT INTO user_vocabulary (profile_id, word) VALUES (?1, ?2)
             ON CONFLICT (profile_id, word) DO UPDATE SET uses = uses + 1",
            params![owner, word],
        )?;
        Ok(())
    }

    /// The user's most-used words, most used first.
    pub fn delete_vocabulary(&self, word: &str) -> Result<(), DbError> {
        let owner = self.owner()?;
        self.connection().execute(
            "DELETE FROM user_vocabulary WHERE profile_id = ?1 AND word = ?2",
            params![owner, word],
        )?;
        Ok(())
    }

    pub fn vocabulary(&self, limit: u32) -> Result<Vec<String>, DbError> {
        let owner = self.owner()?;
        let mut stmt = self.connection().prepare(
            "SELECT word FROM user_vocabulary WHERE profile_id = ?1 ORDER BY uses DESC, word LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![owner, limit], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Replaces a section's cached results (DISC-02): items keep their first-seen time and
    /// viewed flag; ones no longer found are dropped.
    pub fn save_discovery(
        &self,
        section: &str,
        items: &[(String, serde_json::Value)],
    ) -> Result<(), DbError> {
        let now = now_ms();
        let ids: Vec<&str> = items.iter().map(|(id, _)| id.as_str()).collect();
        for (id, data) in items {
            self.connection().execute(
                "INSERT INTO discovery (section, id, data, checked_at, first_seen) VALUES (?1, ?2, ?3, ?4, ?4)
                 ON CONFLICT (section, id) DO UPDATE SET data = ?3, checked_at = ?4",
                params![section, id, data.to_string(), now],
            )?;
        }
        let existing: Vec<String> = {
            let mut stmt = self
                .connection()
                .prepare("SELECT id FROM discovery WHERE section = ?1")?;
            stmt.query_map([section], |r| r.get(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        for gone in existing.iter().filter(|e| !ids.contains(&e.as_str())) {
            self.connection().execute(
                "DELETE FROM discovery WHERE section = ?1 AND id = ?2",
                params![section, gone],
            )?;
        }
        // A section with nothing found still records when it was checked.
        self.connection().execute(
            "INSERT INTO app_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = ?2",
            params![format!("discovery.checked.{section}"), now.to_string()],
        )?;
        Ok(())
    }

    pub fn discovery(&self, section: &str) -> Result<(Vec<Discovered>, Option<i64>), DbError> {
        let mut stmt = self.connection().prepare(
            "SELECT section, id, data, checked_at, first_seen, viewed FROM discovery
             WHERE section = ?1 ORDER BY first_seen, id",
        )?;
        let rows = stmt
            .query_map([section], |r| {
                Ok(Discovered {
                    section: r.get(0)?,
                    id: r.get(1)?,
                    data: serde_json::from_str(&r.get::<_, String>(2)?).unwrap_or_default(),
                    checked_at: r.get(3)?,
                    first_seen: r.get(4)?,
                    viewed: r.get::<_, i64>(5)? == 1,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let checked: Option<i64> = self
            .connection()
            .query_row(
                "SELECT value FROM app_meta WHERE key = ?1",
                [format!("discovery.checked.{section}")],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .and_then(|v| v.parse().ok());
        Ok((rows, checked))
    }

    /// A small value kept by the runtime (brain profiles, limits, price overrides, the refreshed
    /// price table).
    pub fn meta(&self, key: &str) -> Result<Option<String>, DbError> {
        Ok(self
            .connection()
            .query_row("SELECT value FROM app_meta WHERE key = ?1", [key], |r| {
                r.get::<_, String>(0)
            })
            .optional()?)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<(), DbError> {
        self.connection().execute(
            "INSERT INTO app_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = ?2",
            params![key, value],
        )?;
        Ok(())
    }

    /// The user has seen these items: no more "New" label (DISC-12).
    pub fn mark_discovery_viewed(&self, section: &str) -> Result<(), DbError> {
        self.connection().execute(
            "UPDATE discovery SET viewed = 1 WHERE section = ?1",
            [section],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn conversations_keep_messages_and_find_them_again() {
        let db = Database::in_memory().unwrap();
        db.create_conversation("c1", "voice", "Weather").unwrap();
        db.add_message(
            "c1",
            "user",
            "What's the weather in Pune tomorrow?",
            None,
            Some("t1"),
        )
        .unwrap();
        db.add_message(
            "c1",
            "assistant",
            "Sunny, 31 degrees.",
            Some("anthropic"),
            Some("t1"),
        )
        .unwrap();
        db.create_conversation("c2", "chat", "Code").unwrap();
        db.add_message("c2", "user", "Refactor the parser", None, None)
            .unwrap();
        assert_eq!(db.messages("c1").unwrap().len(), 2);
        let found = db.search_messages("weather Pune", 5).unwrap();
        assert_eq!(found[0].conversation_id, "c1");
        assert!(
            db.search_messages("\" OR 1=1 --", 5).is_ok(),
            "user text never forms FTS syntax"
        );
        db.set_summary("c1", "They asked about Pune's weather.", 2)
            .unwrap();
        let c = db.conversation("c1").unwrap().unwrap();
        assert_eq!(
            (c.summary.as_str(), c.summarized),
            ("They asked about Pune's weather.", 2)
        );
        assert_eq!(
            db.latest_conversation("voice", 0).unwrap().unwrap().id,
            "c1"
        );
        assert!(
            db.latest_conversation("voice", now_ms() + 60_000)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn retention_prunes_old_unpinned_conversations() {
        let db = Database::in_memory().unwrap();
        db.create_conversation("old", "voice", "").unwrap();
        db.create_conversation("pinned", "voice", "").unwrap();
        db.update_conversation("pinned", None, None, Some(true))
            .unwrap();
        db.add_message("old", "user", "hello there", None, None)
            .unwrap();
        let later = now_ms() + 31 * 86_400_000;
        assert_eq!(db.prune_conversations(30, later).unwrap(), 1);
        assert!(db.conversation("old").unwrap().is_none());
        assert!(
            db.messages("old").unwrap().is_empty(),
            "messages go with it"
        );
        assert!(
            db.search_messages("hello", 5).unwrap().is_empty(),
            "and the search index too"
        );
        assert!(db.conversation("pinned").unwrap().is_some());
        db.create_conversation("now", "chat", "").unwrap();
        assert_eq!(
            db.prune_conversations(0, now_ms()).unwrap(),
            1,
            "never keep"
        );
    }

    #[test]
    fn preferences_usage_sessions_and_vocabulary() {
        let db = Database::in_memory().unwrap();
        db.set_preference("name", "Sam", "user said \"call me Sam\"")
            .unwrap();
        db.set_preference("units", "metric", "settings").unwrap();
        db.set_preference("name", "Samuel", "user").unwrap();
        assert_eq!(
            db.preferences().unwrap(),
            [
                ("name".into(), "Samuel".into()),
                ("units".into(), "metric".into())
            ]
        );
        db.delete_preference("units").unwrap();
        assert_eq!(db.preferences().unwrap().len(), 1);

        db.record_usage(&UsageRow {
            ts: 1000,
            kind: "brain".into(),
            provider: "anthropic".into(),
            model: "claude-sonnet-5".into(),
            input_tokens: 1200,
            output_tokens: 80,
            cached_tokens: 1000,
            requests: 1,
            cost: Some(0.0012),
            turn_id: Some("t1".into()),
            ..UsageRow::default()
        })
        .unwrap();
        let rows = db.usage_since(0).unwrap();
        assert_eq!((rows[0].input_tokens, rows[0].cost), (1200, Some(0.0012)));
        assert!(db.usage_since(2000).unwrap().is_empty());

        let row = AgentSessionRow {
            id: "sess-1".into(),
            agent: "claude-code".into(),
            workspace: "C:/work/kivo".into(),
            conversation_id: None,
            created_at: 1,
            last_used: 2,
        };
        db.save_agent_session(&row).unwrap();
        db.save_agent_session(&AgentSessionRow {
            last_used: 9,
            ..row.clone()
        })
        .unwrap();
        assert_eq!(
            db.agent_session("claude-code", "C:/work/kivo")
                .unwrap()
                .unwrap()
                .last_used,
            9
        );
        assert!(db.agent_session("codex", "C:/work/kivo").unwrap().is_none());

        for w in ["Kivo", "Moonshine", "Kivo"] {
            db.add_vocabulary(w).unwrap();
        }
        assert_eq!(db.vocabulary(10).unwrap(), ["Kivo", "Moonshine"]);
        db.add_misroute("t1", "open photos", "brain", "wanted the app")
            .unwrap();
        assert_eq!(db.misroute_count().unwrap(), 1);
    }

    #[test]
    fn discovery_is_cached_with_first_seen_and_viewed() {
        let db = Database::in_memory().unwrap();
        assert_eq!(db.discovery("cli").unwrap(), (vec![], None));
        db.save_discovery("cli", &[("claude".into(), json!({ "version": "2.1" }))])
            .unwrap();
        let (items, checked) = db.discovery("cli").unwrap();
        assert!(checked.is_some());
        assert!(!items[0].viewed);
        db.mark_discovery_viewed("cli").unwrap();
        db.save_discovery(
            "cli",
            &[
                ("claude".into(), json!({ "version": "2.2" })),
                ("gemini".into(), json!({})),
            ],
        )
        .unwrap();
        let (items, _) = db.discovery("cli").unwrap();
        assert_eq!(items.len(), 2);
        assert!(
            items[0].viewed && items[0].data["version"] == "2.2",
            "kept its viewed flag"
        );
        assert!(!items[1].viewed, "the new one is New");
        db.save_discovery("cli", &[]).unwrap();
        assert!(
            db.discovery("cli").unwrap().0.is_empty(),
            "gone when no longer found"
        );
    }
}
