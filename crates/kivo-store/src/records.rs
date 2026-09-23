//! What the runtime records about its work: turns and their latency spans (ARCH-28), the Activity
//! timeline (ARCH-23), the append-only audit log whose rows are hash-chained (SEC-22), and
//! permission grants (SEC-08). All rows belong to the owner profile until people profiles exist.

use crate::db::{Database, DbError};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The hash before the first audit row.
pub const GENESIS: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// One audit row (SECURITY §7). `args_summary` must already be redacted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditRecord {
    /// Unix milliseconds.
    pub ts: i64,
    pub turn_id: Option<String>,
    pub task_id: Option<String>,
    pub tool: String,
    pub args_summary: String,
    pub risk: String,
    pub decision: String,
    pub confirmed_by: Option<String>,
    pub result: Option<String>,
    pub error: Option<String>,
}

impl AuditRecord {
    /// `sha256(prev_hash || row)` over a fixed field order (SEC-22).
    pub fn chain_hash(&self, prev_hash: &str) -> String {
        let row = serde_json::json!([
            self.ts,
            self.turn_id,
            self.task_id,
            self.tool,
            self.args_summary,
            self.risk,
            self.decision,
            self.confirmed_by,
            self.result,
            self.error,
        ]);
        let mut h = Sha256::new();
        h.update(prev_hash.as_bytes());
        h.update(row.to_string().as_bytes());
        h.finalize().iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// The result of checking the audit chain (SEC-23 shows it in diagnostics).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainCheck {
    pub rows: u64,
    /// The first row whose hash doesn't match (tampering or corruption).
    pub broken_at: Option<i64>,
}

/// One entry of the Activity timeline (plan §83).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityRecord {
    pub id: i64,
    pub ts: i64,
    pub turn_id: Option<String>,
    pub task_id: Option<String>,
    /// `turn`, `transcript`, `tool`, `reply`, `error`, `setting`, …
    pub kind: String,
    pub title: String,
    pub detail: Option<String>,
    /// `running`, `done`, `failed`, `cancelled`, `denied`, `waiting`.
    pub status: String,
    pub data: Option<serde_json::Value>,
}

/// A new Activity entry (the id and profile are filled in when stored).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewActivity {
    pub ts: i64,
    pub turn_id: Option<String>,
    pub task_id: Option<String>,
    pub kind: String,
    pub title: String,
    pub detail: Option<String>,
    pub status: String,
    pub data: Option<serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnRecord {
    pub id: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub source: String,
    pub transcript: Option<String>,
    pub route: Option<String>,
    pub outcome: Option<String>,
    pub reply: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredGrant {
    pub id: i64,
    pub tool: String,
    pub scope: Option<String>,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    /// A `*` pattern over the call's arguments (SEC-08).
    pub pattern: Option<String>,
    /// Set for a grant that lasts only while this runtime session runs.
    pub session: Option<String>,
}

/// What a new grant covers and for how long.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NewGrant<'a> {
    pub tool: &'a str,
    pub scope: Option<&'a str>,
    pub pattern: Option<&'a str>,
    pub now: i64,
    pub expires_at: Option<i64>,
    pub session: Option<&'a str>,
}

impl Database {
    /// Appends an audit row chained to the previous one; returns its hash.
    pub fn append_audit(&mut self, record: &AuditRecord) -> Result<String, DbError> {
        let owner = self.owner_profile()?.to_string();
        let tx = self.connection_mut().transaction()?;
        let prev: String = tx
            .query_row("SELECT hash FROM audit ORDER BY id DESC LIMIT 1", [], |r| {
                r.get(0)
            })
            .optional()?
            .unwrap_or_else(|| GENESIS.to_owned());
        let hash = record.chain_hash(&prev);
        tx.execute(
            "INSERT INTO audit (profile_id, ts, turn_id, task_id, tool, args_summary, risk, decision,
                                confirmed_by, result, error, prev_hash, hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                owner,
                record.ts,
                record.turn_id,
                record.task_id,
                record.tool,
                record.args_summary,
                record.risk,
                record.decision,
                record.confirmed_by,
                record.result,
                record.error,
                prev,
                hash
            ],
        )?;
        tx.commit()?;
        Ok(hash)
    }

    /// Re-computes every audit hash in order and reports the first mismatch.
    pub fn verify_audit(&self) -> Result<ChainCheck, DbError> {
        let mut stmt = self.connection().prepare(
            "SELECT id, ts, turn_id, task_id, tool, args_summary, risk, decision, confirmed_by, result,
                    error, prev_hash, hash FROM audit ORDER BY id",
        )?;
        let mut rows = stmt.query([])?;
        let (mut count, mut prev) = (0u64, GENESIS.to_owned());
        while let Some(r) = rows.next()? {
            count += 1;
            let id: i64 = r.get(0)?;
            let record = AuditRecord {
                ts: r.get(1)?,
                turn_id: r.get(2)?,
                task_id: r.get(3)?,
                tool: r.get(4)?,
                args_summary: r.get(5)?,
                risk: r.get(6)?,
                decision: r.get(7)?,
                confirmed_by: r.get(8)?,
                result: r.get(9)?,
                error: r.get(10)?,
            };
            let (stored_prev, stored_hash): (String, String) = (r.get(11)?, r.get(12)?);
            if stored_prev != prev || record.chain_hash(&prev) != stored_hash {
                return Ok(ChainCheck {
                    rows: count,
                    broken_at: Some(id),
                });
            }
            prev = stored_hash;
        }
        Ok(ChainCheck {
            rows: count,
            broken_at: None,
        })
    }

    /// The newest audit rows, newest first.
    pub fn recent_audit(&self, limit: u32) -> Result<Vec<AuditRecord>, DbError> {
        let mut stmt = self.connection().prepare(
            "SELECT ts, turn_id, task_id, tool, args_summary, risk, decision, confirmed_by, result, error
             FROM audit ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit], |r| {
            Ok(AuditRecord {
                ts: r.get(0)?,
                turn_id: r.get(1)?,
                task_id: r.get(2)?,
                tool: r.get(3)?,
                args_summary: r.get(4)?,
                risk: r.get(5)?,
                decision: r.get(6)?,
                confirmed_by: r.get(7)?,
                result: r.get(8)?,
                error: r.get(9)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn add_activity(&self, entry: &NewActivity) -> Result<ActivityRecord, DbError> {
        let owner = self.owner_profile()?.to_string();
        self.connection().execute(
            "INSERT INTO activity (profile_id, ts, turn_id, task_id, kind, title, detail, status, data)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                owner,
                entry.ts,
                entry.turn_id,
                entry.task_id,
                entry.kind,
                entry.title,
                entry.detail,
                entry.status,
                entry.data.as_ref().map(serde_json::Value::to_string),
            ],
        )?;
        Ok(ActivityRecord {
            id: self.connection().last_insert_rowid(),
            ts: entry.ts,
            turn_id: entry.turn_id.clone(),
            task_id: entry.task_id.clone(),
            kind: entry.kind.clone(),
            title: entry.title.clone(),
            detail: entry.detail.clone(),
            status: entry.status.clone(),
            data: entry.data.clone(),
        })
    }

    /// Activity entries older than `before` (an id; `None` for the newest), newest first.
    pub fn activity(
        &self,
        before: Option<i64>,
        limit: u32,
    ) -> Result<Vec<ActivityRecord>, DbError> {
        let mut stmt = self.connection().prepare(
            "SELECT id, ts, turn_id, task_id, kind, title, detail, status, data FROM activity
             WHERE ?1 IS NULL OR id < ?1 ORDER BY id DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![before, limit], |r| {
            let data: Option<String> = r.get(8)?;
            Ok(ActivityRecord {
                id: r.get(0)?,
                ts: r.get(1)?,
                turn_id: r.get(2)?,
                task_id: r.get(3)?,
                kind: r.get(4)?,
                title: r.get(5)?,
                detail: r.get(6)?,
                status: r.get(7)?,
                data: data.and_then(|d| serde_json::from_str(&d).ok()),
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn start_turn(&self, id: &str, started_at: i64, source: &str) -> Result<(), DbError> {
        let owner = self.owner_profile()?.to_string();
        self.connection().execute(
            "INSERT INTO turns (id, profile_id, started_at, source) VALUES (?1, ?2, ?3, ?4)",
            params![id, owner, started_at, source],
        )?;
        Ok(())
    }

    /// How a brain turn was routed (the card's reason, PLAN-17), and its hidden reasoning when
    /// the user keeps a reasoning log (BRAIN-09).
    pub fn set_turn_route(
        &self,
        id: &str,
        route: &str,
        reasoning: Option<&str>,
    ) -> Result<(), DbError> {
        self.connection().execute(
            "UPDATE turns SET route = ?2, reasoning = COALESCE(?3, reasoning) WHERE id = ?1",
            params![id, route, reasoning],
        )?;
        Ok(())
    }

    /// Fills in what a turn heard, how it was routed, how it ended and what KIVO said.
    pub fn finish_turn(&self, turn: &TurnRecord) -> Result<(), DbError> {
        self.connection().execute(
            "UPDATE turns SET ended_at = ?2, transcript = ?3, route = COALESCE(?4, route),
                              outcome = ?5, reply = ?6
             WHERE id = ?1",
            params![
                turn.id,
                turn.ended_at,
                turn.transcript,
                turn.route,
                turn.outcome,
                turn.reply
            ],
        )?;
        Ok(())
    }

    pub fn turn(&self, id: &str) -> Result<Option<TurnRecord>, DbError> {
        Ok(self
            .connection()
            .query_row(
                "SELECT id, started_at, ended_at, source, transcript, route, outcome, reply FROM turns WHERE id = ?1",
                [id],
                |r| {
                    Ok(TurnRecord {
                        id: r.get(0)?,
                        started_at: r.get(1)?,
                        ended_at: r.get(2)?,
                        source: r.get(3)?,
                        transcript: r.get(4)?,
                        route: r.get(5)?,
                        outcome: r.get(6)?,
                        reply: r.get(7)?,
                    })
                },
            )
            .optional()?)
    }

    /// The newest turns' route and T0–T10 timings, for the medians in Settings → Performance.
    pub fn recent_turn_timings(
        &self,
        limit: u32,
    ) -> Result<Vec<(Option<String>, String, serde_json::Value)>, DbError> {
        let mut stmt = self.connection().prepare(
            "SELECT t.route, t.source, m.spans FROM turns t JOIN turn_metrics m ON m.turn_id = t.id
             ORDER BY t.started_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit], |r| {
            let spans: String = r.get(2)?;
            Ok((
                r.get(0)?,
                r.get(1)?,
                serde_json::from_str(&spans).unwrap_or(serde_json::Value::Null),
            ))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Stores a turn's T0–T10 timings (ARCH-28).
    pub fn record_turn_metrics(
        &self,
        turn_id: &str,
        spans: &serde_json::Value,
    ) -> Result<(), DbError> {
        let owner = self.owner_profile()?.to_string();
        self.connection().execute(
            "INSERT OR REPLACE INTO turn_metrics (turn_id, profile_id, spans) VALUES (?1, ?2, ?3)",
            params![turn_id, owner, spans.to_string()],
        )?;
        Ok(())
    }

    pub fn turn_metrics(&self, turn_id: &str) -> Result<Option<serde_json::Value>, DbError> {
        let spans: Option<String> = self
            .connection()
            .query_row(
                "SELECT spans FROM turn_metrics WHERE turn_id = ?1",
                [turn_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(spans.and_then(|s| serde_json::from_str(&s).ok()))
    }

    pub fn add_grant(
        &self,
        tool: &str,
        scope: Option<&str>,
        now: i64,
        expires_at: Option<i64>,
    ) -> Result<i64, DbError> {
        self.add_scoped_grant(&NewGrant {
            tool,
            scope,
            now,
            expires_at,
            ..NewGrant::default()
        })
    }

    pub fn add_scoped_grant(&self, g: &NewGrant<'_>) -> Result<i64, DbError> {
        let owner = self.owner_profile()?.to_string();
        self.connection().execute(
            "INSERT INTO permissions_grants (profile_id, tool, scope, created_at, expires_at, pattern, session)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![owner, g.tool, g.scope, g.now, g.expires_at, g.pattern, g.session],
        )?;
        Ok(self.connection().last_insert_rowid())
    }

    /// Grants still in force at `now` (any session's grants included; callers pick theirs).
    pub fn grants(&self, now: i64) -> Result<Vec<StoredGrant>, DbError> {
        let mut stmt = self.connection().prepare(
            "SELECT id, tool, scope, created_at, expires_at, pattern, session FROM permissions_grants
             WHERE expires_at IS NULL OR expires_at > ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map([now], |r| {
            Ok(StoredGrant {
                id: r.get(0)?,
                tool: r.get(1)?,
                scope: r.get(2)?,
                created_at: r.get(3)?,
                expires_at: r.get(4)?,
                pattern: r.get(5)?,
                session: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Removes grants that belonged to earlier runtime sessions.
    pub fn drop_session_grants(&self, keep: &str) -> Result<usize, DbError> {
        Ok(self.connection().execute(
            "DELETE FROM permissions_grants WHERE session IS NOT NULL AND session <> ?1",
            [keep],
        )?)
    }

    /// When each tool last ran (from the audit log): (tool, epoch ms).
    pub fn last_tool_uses(&self) -> Result<Vec<(String, i64)>, DbError> {
        let mut stmt = self
            .connection()
            .prepare("SELECT tool, MAX(ts) FROM audit WHERE result IS NOT NULL GROUP BY tool")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn revoke_grant(&self, id: i64) -> Result<bool, DbError> {
        Ok(self
            .connection()
            .execute("DELETE FROM permissions_grants WHERE id = ?1", [id])?
            > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(tool: &str, ts: i64) -> AuditRecord {
        AuditRecord {
            ts,
            turn_id: Some("t1".into()),
            task_id: None,
            tool: tool.into(),
            args_summary: "{\"app\":\"Chrome\"}".into(),
            risk: "low".into(),
            decision: "allow".into(),
            confirmed_by: Some("policy".into()),
            result: Some("ok".into()),
            error: None,
        }
    }

    #[test]
    fn audit_rows_chain_and_the_chain_verifies() {
        let mut db = Database::in_memory().unwrap();
        let first = db.append_audit(&record("apps.launch", 1)).unwrap();
        let second = db.append_audit(&record("audio.mute", 2)).unwrap();
        assert_ne!(first, second);
        assert_eq!(record("audio.mute", 2).chain_hash(&first), second);
        assert_eq!(
            db.verify_audit().unwrap(),
            ChainCheck {
                rows: 2,
                broken_at: None
            }
        );
        assert_eq!(db.recent_audit(1).unwrap()[0].tool, "audio.mute");
    }

    #[test]
    fn the_audit_log_refuses_edits_and_detects_tampering() {
        let mut db = Database::in_memory().unwrap();
        db.append_audit(&record("apps.launch", 1)).unwrap();
        db.append_audit(&record("system.shutdown", 2)).unwrap();
        let edit = db
            .connection()
            .execute("UPDATE audit SET decision = 'deny' WHERE id = 1", []);
        assert!(edit.is_err(), "updates are refused");
        assert!(
            db.connection().execute("DELETE FROM audit", []).is_err(),
            "deletes are refused"
        );
        // Someone with direct file access drops the trigger and edits a row: verification notices.
        db.connection()
            .execute_batch(
                "DROP TRIGGER audit_is_append_only_update;
                 UPDATE audit SET decision = 'deny' WHERE id = 1;",
            )
            .unwrap();
        assert_eq!(db.verify_audit().unwrap().broken_at, Some(1));
    }

    #[test]
    fn activity_pages_newest_first() {
        let db = Database::in_memory().unwrap();
        for i in 0..5 {
            db.add_activity(&NewActivity {
                ts: i,
                turn_id: None,
                task_id: None,
                kind: "tool".into(),
                title: format!("step {i}"),
                detail: None,
                status: "done".into(),
                data: Some(serde_json::json!({"i": i})),
            })
            .unwrap();
        }
        let page = db.activity(None, 2).unwrap();
        assert_eq!(
            page.iter().map(|a| a.title.as_str()).collect::<Vec<_>>(),
            ["step 4", "step 3"]
        );
        let next = db.activity(Some(page[1].id), 10).unwrap();
        assert_eq!(next.len(), 3);
        assert_eq!(next[2].data, Some(serde_json::json!({"i": 0})));
    }

    #[test]
    fn turns_and_their_metrics_round_trip() {
        let db = Database::in_memory().unwrap();
        db.start_turn("t1", 100, "pushToTalk").unwrap();
        db.finish_turn(&TurnRecord {
            id: "t1".into(),
            started_at: 100,
            ended_at: Some(900),
            source: "pushToTalk".into(),
            transcript: Some("open chrome".into()),
            route: Some("fastPath".into()),
            outcome: Some("done".into()),
            reply: Some("Done.".into()),
        })
        .unwrap();
        assert_eq!(
            db.turn("t1").unwrap().unwrap().reply.as_deref(),
            Some("Done.")
        );
        let spans = serde_json::json!({"t4EndOfSpeech": 0, "t5Final": 120});
        db.record_turn_metrics("t1", &spans).unwrap();
        assert_eq!(db.turn_metrics("t1").unwrap(), Some(spans));
    }

    #[test]
    fn grants_expire_and_can_be_revoked() {
        let db = Database::in_memory().unwrap();
        let always = db
            .add_grant("apps.close", Some("chrome"), 10, None)
            .unwrap();
        db.add_grant("audio.mute", None, 10, Some(20)).unwrap();
        assert_eq!(db.grants(15).unwrap().len(), 2);
        assert_eq!(db.grants(25).unwrap().len(), 1);
        assert!(db.revoke_grant(always).unwrap());
        assert!(db.grants(25).unwrap().is_empty());
    }
}
