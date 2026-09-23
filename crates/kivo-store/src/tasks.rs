//! Tasks, routines, workspaces and instructions (MEMORY §1, ROUTINES §5, CONVERSATION §4).
//!
//! Tasks are kept until the user deletes them; finished ones older than 90 days keep only a
//! one-line summary (MEM-02). Routines are a versioned JSON body. Instructions are the text the
//! user keeps for KIVO ("About me") and per workspace, mirrored as Markdown by the runtime.

use crate::brains::now_ms;
use crate::db::{Database, DbError};
use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

/// How long a finished task keeps its steps before it is summarized (MEM-02).
pub const TASK_DETAIL_KEPT_MS: i64 = 90 * 24 * 60 * 60 * 1000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredTask {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub owner: String,
    pub status: String,
    /// The `TaskSpec` as JSON (`{}` once summarized).
    pub spec: String,
    pub result: Option<String>,
    pub error: Option<String>,
    pub summary: Option<String>,
    pub routine_id: Option<String>,
    pub turn_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub finished_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredStep {
    pub step_id: String,
    pub position: i64,
    pub title: String,
    pub status: String,
    pub detail: Option<String>,
    pub result: Option<String>,
    pub attempts: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
}

/// A new task's row.
pub struct NewTask<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub kind: &'a str,
    pub owner: &'a str,
    pub status: &'a str,
    pub spec: &'a str,
    pub routine_id: Option<&'a str>,
    pub turn_id: Option<&'a str>,
    /// (step id, title), in order.
    pub steps: &'a [(String, String)],
}

/// A step's change.
#[derive(Default)]
pub struct StepUpdate<'a> {
    pub status: &'a str,
    pub detail: Option<&'a str>,
    pub result: Option<&'a str>,
    pub attempts: Option<i64>,
    pub started: bool,
    pub finished: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredRoutine {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub version: i64,
    /// The `Routine` as JSON.
    pub body: String,
    pub starter: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_run: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub path: String,
    pub name: String,
    /// The user said yes to remembering it (no: KIVO doesn't ask again).
    pub remembered: bool,
    pub preferred_agent: Option<String>,
    pub agent_mode: Option<String>,
    pub created_at: i64,
    pub last_used: i64,
}

const TASK_COLUMNS: &str = "id, title, kind, owner, status, spec, result, error, summary, \
     routine_id, turn_id, created_at, updated_at, finished_at";

fn task_row(r: &Row<'_>) -> rusqlite::Result<StoredTask> {
    Ok(StoredTask {
        id: r.get(0)?,
        title: r.get(1)?,
        kind: r.get(2)?,
        owner: r.get(3)?,
        status: r.get(4)?,
        spec: r.get(5)?,
        result: r.get(6)?,
        error: r.get(7)?,
        summary: r.get(8)?,
        routine_id: r.get(9)?,
        turn_id: r.get(10)?,
        created_at: r.get(11)?,
        updated_at: r.get(12)?,
        finished_at: r.get(13)?,
    })
}

fn routine_row(r: &Row<'_>) -> rusqlite::Result<StoredRoutine> {
    Ok(StoredRoutine {
        id: r.get(0)?,
        name: r.get(1)?,
        enabled: r.get::<_, i64>(2)? == 1,
        version: r.get(3)?,
        body: r.get(4)?,
        starter: r.get(5)?,
        created_at: r.get(6)?,
        updated_at: r.get(7)?,
        last_run: r.get(8)?,
    })
}

fn workspace_row(r: &Row<'_>) -> rusqlite::Result<Workspace> {
    Ok(Workspace {
        id: r.get(0)?,
        path: r.get(1)?,
        name: r.get(2)?,
        remembered: r.get::<_, i64>(3)? == 1,
        preferred_agent: r.get(4)?,
        agent_mode: r.get(5)?,
        created_at: r.get(6)?,
        last_used: r.get(7)?,
    })
}

/// Statuses a task is finished in (`kivo_core::task::TaskStatus::is_final`).
const FINAL: &str = "('done', 'failed', 'cancelled', 'interrupted')";

/// A finished task to summarize: id, title, status, result, error.
type OldTask = (String, String, String, Option<String>, Option<String>);

impl Database {
    fn person(&self) -> Result<String, DbError> {
        Ok(self.owner_profile()?.to_string())
    }

    // ---- Tasks ------------------------------------------------------------------------------

    pub fn insert_task(&self, t: &NewTask<'_>) -> Result<(), DbError> {
        let owner = self.person()?;
        let now = now_ms();
        let conn = self.connection();
        conn.execute(
            "INSERT INTO tasks (id, profile_id, title, kind, owner, status, spec, routine_id,
                                turn_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
            params![
                t.id,
                owner,
                t.title,
                t.kind,
                t.owner,
                t.status,
                t.spec,
                t.routine_id,
                t.turn_id,
                now
            ],
        )?;
        for (i, (step, title)) in t.steps.iter().enumerate() {
            conn.execute(
                "INSERT INTO task_steps (task_id, profile_id, step_id, position, title, status)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'pending')",
                params![t.id, owner, step, i64::try_from(i).unwrap_or(0), title],
            )?;
        }
        Ok(())
    }

    /// A task's status, with its result or error when it has one.
    pub fn set_task_status(
        &self,
        id: &str,
        status: &str,
        result: Option<&str>,
        error: Option<&str>,
    ) -> Result<(), DbError> {
        let now = now_ms();
        let finished = matches!(status, "done" | "failed" | "cancelled" | "interrupted");
        self.connection().execute(
            "UPDATE tasks SET status = ?2, updated_at = ?3,
                    result = COALESCE(?4, result), error = COALESCE(?5, error),
                    finished_at = CASE WHEN ?6 THEN ?3 ELSE NULL END
             WHERE id = ?1",
            params![id, status, now, result, error, finished],
        )?;
        Ok(())
    }

    pub fn set_step(&self, task: &str, step: &str, u: &StepUpdate<'_>) -> Result<(), DbError> {
        let now = now_ms();
        self.connection().execute(
            "UPDATE task_steps SET status = ?3,
                    detail = COALESCE(?4, detail), result = COALESCE(?5, result),
                    attempts = COALESCE(?6, attempts),
                    started_at = CASE WHEN ?7 THEN ?9 ELSE started_at END,
                    finished_at = CASE WHEN ?8 THEN ?9 ELSE finished_at END
             WHERE task_id = ?1 AND step_id = ?2",
            params![
                task, step, u.status, u.detail, u.result, u.attempts, u.started, u.finished, now
            ],
        )?;
        self.connection().execute(
            "UPDATE tasks SET updated_at = ?2 WHERE id = ?1",
            params![task, now],
        )?;
        Ok(())
    }

    pub fn task(&self, id: &str) -> Result<Option<StoredTask>, DbError> {
        Ok(self
            .connection()
            .query_row(
                &format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id = ?1"),
                params![id],
                task_row,
            )
            .optional()?)
    }

    pub fn task_steps(&self, id: &str) -> Result<Vec<StoredStep>, DbError> {
        let mut stmt = self.connection().prepare(
            "SELECT step_id, position, title, status, detail, result, attempts, started_at,
                    finished_at
             FROM task_steps WHERE task_id = ?1 ORDER BY position",
        )?;
        let rows = stmt.query_map(params![id], |r| {
            Ok(StoredStep {
                step_id: r.get(0)?,
                position: r.get(1)?,
                title: r.get(2)?,
                status: r.get(3)?,
                detail: r.get(4)?,
                result: r.get(5)?,
                attempts: r.get(6)?,
                started_at: r.get(7)?,
                finished_at: r.get(8)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Tasks, newest first: the active ones (not finished) or the finished ones.
    pub fn tasks(&self, finished: bool, limit: u32) -> Result<Vec<StoredTask>, DbError> {
        let owner = self.person()?;
        let filter = if finished { "IN" } else { "NOT IN" };
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {TASK_COLUMNS} FROM tasks
             WHERE profile_id = ?1 AND status {filter} {FINAL}
             ORDER BY created_at DESC LIMIT ?2"
        ))?;
        let rows = stmt.query_map(params![owner, limit], task_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// After a crash or a restart (ARCH-27): tasks that were running or starting are marked
    /// interrupted and returned, so the user can be told; waiting watchers and paused tasks are
    /// left as they were (watching has no side effect, and the runtime arms them again).
    pub fn interrupt_unfinished(&self) -> Result<Vec<StoredTask>, DbError> {
        let owner = self.person()?;
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {TASK_COLUMNS} FROM tasks
             WHERE profile_id = ?1 AND status IN ('pending', 'running', 'needsYou')
             ORDER BY created_at"
        ))?;
        let hit: Vec<StoredTask> = stmt
            .query_map(params![owner], task_row)?
            .collect::<Result<_, _>>()?;
        drop(stmt);
        for t in &hit {
            self.set_task_status(&t.id, "interrupted", None, None)?;
            self.connection().execute(
                "UPDATE task_steps SET status = 'failed', detail = COALESCE(detail, 'interrupted')
                 WHERE task_id = ?1 AND status = 'running'",
                params![t.id],
            )?;
        }
        Ok(hit)
    }

    /// Watchers that were waiting when KIVO last stopped.
    pub fn waiting_tasks(&self) -> Result<Vec<StoredTask>, DbError> {
        let owner = self.person()?;
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {TASK_COLUMNS} FROM tasks WHERE profile_id = ?1 AND status = 'waiting'
             ORDER BY created_at"
        ))?;
        let rows = stmt.query_map(params![owner], task_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// MEM-02: finished tasks older than `before` keep a one-line summary; their steps and
    /// arguments go. Returns how many were summarized.
    pub fn summarize_old_tasks(&self, before: i64) -> Result<usize, DbError> {
        let owner = self.person()?;
        let mut stmt = self.connection().prepare(&format!(
            "SELECT id, title, status, result, error FROM tasks
             WHERE profile_id = ?1 AND status IN {FINAL} AND summary IS NULL
               AND finished_at IS NOT NULL AND finished_at < ?2"
        ))?;
        let old: Vec<OldTask> = stmt
            .query_map(params![owner, before], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?
            .collect::<Result<_, _>>()?;
        drop(stmt);
        for (id, title, status, result, error) in &old {
            let steps: i64 = self.connection().query_row(
                "SELECT COUNT(*) FROM task_steps WHERE task_id = ?1",
                params![id],
                |r| r.get(0),
            )?;
            let outcome = result.as_deref().or(error.as_deref()).unwrap_or_default();
            let mut summary = format!("{title} — {status}, {steps} steps");
            if !outcome.is_empty() {
                summary.push_str(": ");
                summary.push_str(&outcome.chars().take(200).collect::<String>());
            }
            self.connection()
                .execute("DELETE FROM task_steps WHERE task_id = ?1", params![id])?;
            self.connection().execute(
                "UPDATE tasks SET summary = ?2, spec = '{}', result = NULL WHERE id = ?1",
                params![id, summary],
            )?;
        }
        Ok(old.len())
    }

    pub fn delete_task(&self, id: &str) -> Result<bool, DbError> {
        Ok(self
            .connection()
            .execute("DELETE FROM tasks WHERE id = ?1", params![id])?
            > 0)
    }

    /// Deletes every finished task.
    pub fn clear_finished_tasks(&self) -> Result<usize, DbError> {
        let owner = self.person()?;
        Ok(self.connection().execute(
            &format!("DELETE FROM tasks WHERE profile_id = ?1 AND status IN {FINAL}"),
            params![owner],
        )?)
    }

    // ---- Routines ---------------------------------------------------------------------------

    /// Adds or replaces a routine.
    pub fn save_routine(
        &self,
        id: &str,
        name: &str,
        enabled: bool,
        version: u32,
        body: &str,
        starter: Option<&str>,
    ) -> Result<(), DbError> {
        let owner = self.person()?;
        let now = now_ms();
        self.connection().execute(
            "INSERT INTO routines (id, profile_id, name, enabled, version, body, starter,
                                   created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
             ON CONFLICT (id) DO UPDATE SET name = ?3, enabled = ?4, version = ?5, body = ?6,
                                            updated_at = ?8",
            params![id, owner, name, enabled, version, body, starter, now],
        )?;
        Ok(())
    }

    pub fn routines(&self) -> Result<Vec<StoredRoutine>, DbError> {
        let owner = self.person()?;
        let mut stmt = self.connection().prepare(
            "SELECT id, name, enabled, version, body, starter, created_at, updated_at, last_run
             FROM routines WHERE profile_id = ?1 ORDER BY name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map(params![owner], routine_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn routine(&self, id: &str) -> Result<Option<StoredRoutine>, DbError> {
        Ok(self
            .connection()
            .query_row(
                "SELECT id, name, enabled, version, body, starter, created_at, updated_at,
                        last_run
                 FROM routines WHERE id = ?1",
                params![id],
                routine_row,
            )
            .optional()?)
    }

    pub fn delete_routine(&self, id: &str) -> Result<bool, DbError> {
        Ok(self
            .connection()
            .execute("DELETE FROM routines WHERE id = ?1", params![id])?
            > 0)
    }

    pub fn routine_ran(&self, id: &str) -> Result<(), DbError> {
        self.connection().execute(
            "UPDATE routines SET last_run = ?2 WHERE id = ?1",
            params![id, now_ms()],
        )?;
        Ok(())
    }

    /// Whether a starter routine (ROUT-10) was already added (even if the user deleted it since,
    /// it isn't added again: the key is remembered in `app_meta`).
    pub fn starter_seen(&self, key: &str) -> Result<bool, DbError> {
        let meta_key = format!("starter:{key}");
        let seen: Option<String> = self
            .connection()
            .query_row(
                "SELECT value FROM app_meta WHERE key = ?1",
                params![meta_key],
                |r| r.get(0),
            )
            .optional()?;
        if seen.is_none() {
            self.connection().execute(
                "INSERT INTO app_meta (key, value) VALUES (?1, '1')",
                params![meta_key],
            )?;
        }
        Ok(seen.is_some())
    }

    // ---- Workspaces and instructions --------------------------------------------------------

    pub fn workspace_by_path(&self, path: &str) -> Result<Option<Workspace>, DbError> {
        let owner = self.person()?;
        Ok(self
            .connection()
            .query_row(
                "SELECT id, path, name, remembered, preferred_agent, agent_mode, created_at,
                        last_used
                 FROM workspaces WHERE profile_id = ?1 AND path = ?2 COLLATE NOCASE",
                params![owner, path],
                workspace_row,
            )
            .optional()?)
    }

    pub fn workspace(&self, id: &str) -> Result<Option<Workspace>, DbError> {
        Ok(self
            .connection()
            .query_row(
                "SELECT id, path, name, remembered, preferred_agent, agent_mode, created_at,
                        last_used
                 FROM workspaces WHERE id = ?1",
                params![id],
                workspace_row,
            )
            .optional()?)
    }

    /// The remembered workspaces, most recently used first.
    pub fn workspaces(&self) -> Result<Vec<Workspace>, DbError> {
        let owner = self.person()?;
        let mut stmt = self.connection().prepare(
            "SELECT id, path, name, remembered, preferred_agent, agent_mode, created_at,
                    last_used
             FROM workspaces WHERE profile_id = ?1 AND remembered = 1 ORDER BY last_used DESC",
        )?;
        let rows = stmt.query_map(params![owner], workspace_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Records the answer to "Remember … as a workspace?" (yes or no; either way it isn't asked
    /// again).
    pub fn remember_workspace(
        &self,
        id: &str,
        path: &str,
        name: &str,
        remembered: bool,
    ) -> Result<Workspace, DbError> {
        let owner = self.person()?;
        let now = now_ms();
        self.connection().execute(
            "INSERT INTO workspaces (id, profile_id, path, name, remembered, created_at, last_used)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT (profile_id, path) DO UPDATE SET name = ?4, remembered = ?5",
            params![id, owner, path, name, remembered, now],
        )?;
        self.workspace_by_path(path)?
            .ok_or(DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows))
    }

    pub fn touch_workspace(&self, id: &str) -> Result<(), DbError> {
        self.connection().execute(
            "UPDATE workspaces SET last_used = ?2 WHERE id = ?1",
            params![id, now_ms()],
        )?;
        Ok(())
    }

    pub fn set_workspace_agent(
        &self,
        id: &str,
        agent: Option<&str>,
        mode: Option<&str>,
    ) -> Result<(), DbError> {
        self.connection().execute(
            "UPDATE workspaces SET preferred_agent = ?2, agent_mode = ?3 WHERE id = ?1",
            params![id, agent, mode],
        )?;
        Ok(())
    }

    pub fn forget_workspace(&self, id: &str) -> Result<(), DbError> {
        let owner = self.person()?;
        self.connection()
            .execute("DELETE FROM workspaces WHERE id = ?1", params![id])?;
        self.connection().execute(
            "DELETE FROM instructions WHERE profile_id = ?1 AND scope = ?2",
            params![owner, format!("workspace:{id}")],
        )?;
        Ok(())
    }

    /// Instructions: `global` ("About me") or `workspace:<id>` (CONV-09).
    pub fn instructions(&self, scope: &str) -> Result<Option<(String, i64)>, DbError> {
        let owner = self.person()?;
        Ok(self
            .connection()
            .query_row(
                "SELECT text, updated_at FROM instructions WHERE profile_id = ?1 AND scope = ?2",
                params![owner, scope],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    pub fn set_instructions(&self, scope: &str, text: &str) -> Result<i64, DbError> {
        let owner = self.person()?;
        let now = now_ms();
        self.connection().execute(
            "INSERT INTO instructions (profile_id, scope, text, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (profile_id, scope) DO UPDATE SET text = ?3, updated_at = ?4",
            params![owner, scope, text, now],
        )?;
        Ok(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(db: &Database, id: &str, status: &str) {
        db.insert_task(&NewTask {
            id,
            title: "Watch the build",
            kind: "watch",
            owner: "you",
            status,
            spec: r#"{"steps":[]}"#,
            routine_id: None,
            turn_id: Some("t1"),
            steps: &[
                ("w".into(), "Wait for cargo".into()),
                ("s".into(), "Tell you".into()),
            ],
        })
        .unwrap();
    }

    #[test]
    fn tasks_and_steps_round_trip() {
        let db = Database::in_memory().unwrap();
        task(&db, "a", "running");
        db.set_step(
            "a",
            "w",
            &StepUpdate {
                status: "running",
                started: true,
                ..Default::default()
            },
        )
        .unwrap();
        db.set_step(
            "a",
            "w",
            &StepUpdate {
                status: "done",
                detail: Some("cargo exited"),
                finished: true,
                ..Default::default()
            },
        )
        .unwrap();
        let steps = db.task_steps("a").unwrap();
        assert_eq!(steps[0].status, "done");
        assert!(steps[0].started_at.is_some() && steps[0].finished_at.is_some());
        assert_eq!(steps[1].status, "pending");
        db.set_task_status("a", "done", Some("The build finished."), None)
            .unwrap();
        let t = db.task("a").unwrap().unwrap();
        assert!(t.finished_at.is_some());
        assert_eq!(t.result.as_deref(), Some("The build finished."));
        assert_eq!(db.tasks(true, 10).unwrap().len(), 1);
        assert!(db.tasks(false, 10).unwrap().is_empty());
    }

    #[test]
    fn a_restart_interrupts_running_tasks_but_keeps_watchers() {
        let db = Database::in_memory().unwrap();
        task(&db, "run", "running");
        task(&db, "wait", "waiting");
        task(&db, "paused", "paused");
        let hit = db.interrupt_unfinished().unwrap();
        assert_eq!(hit.len(), 1);
        assert_eq!(db.task("run").unwrap().unwrap().status, "interrupted");
        assert_eq!(db.waiting_tasks().unwrap().len(), 1);
        assert_eq!(db.task("paused").unwrap().unwrap().status, "paused");
    }

    #[test]
    fn old_finished_tasks_keep_a_summary_only() {
        let db = Database::in_memory().unwrap();
        task(&db, "old", "running");
        db.set_task_status("old", "done", Some("Built in 42 s."), None)
            .unwrap();
        task(&db, "new", "running");
        assert_eq!(db.summarize_old_tasks(now_ms() + 1).unwrap(), 1);
        let t = db.task("old").unwrap().unwrap();
        assert_eq!(
            t.summary.as_deref(),
            Some("Watch the build — done, 2 steps: Built in 42 s.")
        );
        assert_eq!(t.spec, "{}");
        assert!(db.task_steps("old").unwrap().is_empty());
        assert_eq!(db.task_steps("new").unwrap().len(), 2, "unfinished: kept");
        assert_eq!(db.summarize_old_tasks(now_ms() + 1).unwrap(), 0, "once");
    }

    #[test]
    fn routines_workspaces_and_instructions() {
        let db = Database::in_memory().unwrap();
        db.save_routine(
            "r1",
            "Work mode",
            false,
            1,
            r#"{"id":"r1"}"#,
            Some("work-mode"),
        )
        .unwrap();
        db.save_routine("r1", "Work mode", true, 1, r#"{"id":"r1","x":1}"#, None)
            .unwrap();
        let r = db.routine("r1").unwrap().unwrap();
        assert!(r.enabled && r.body.contains("\"x\"") && r.starter.as_deref() == Some("work-mode"));
        assert!(!db.starter_seen("work-mode").unwrap());
        assert!(db.starter_seen("work-mode").unwrap());
        db.routine_ran("r1").unwrap();
        assert!(db.routine("r1").unwrap().unwrap().last_run.is_some());

        let w = db
            .remember_workspace("w1", r"C:\work\kivo", "kivo", true)
            .unwrap();
        assert!(w.remembered);
        assert_eq!(
            db.workspace_by_path(r"c:\WORK\kivo").unwrap().unwrap().id,
            "w1",
            "paths compare without case"
        );
        db.remember_workspace("w2", r"C:\tmp", "tmp", false)
            .unwrap();
        assert_eq!(
            db.workspaces().unwrap().len(),
            1,
            "declined ones aren't listed"
        );

        assert!(db.instructions("global").unwrap().is_none());
        db.set_instructions("global", "Call me Sam.").unwrap();
        db.set_instructions("workspace:w1", "Tests: cargo nextest.")
            .unwrap();
        assert_eq!(
            db.instructions("global").unwrap().unwrap().0,
            "Call me Sam."
        );
        db.forget_workspace("w1").unwrap();
        assert!(db.instructions("workspace:w1").unwrap().is_none());
    }
}
