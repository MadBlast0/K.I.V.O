//! What KIVO writes down (ARCHITECTURE §4.1, SECURITY §7): the Activity timeline the Control
//! Center shows (ARCH-23), the hash-chained audit log of every permission decision and tool result
//! (SEC-22), the turn records with their T0–T10 timings (ARCH-28), and permission grants (SEC-08).
//!
//! Writing is best-effort: a storage problem is logged, never surfaced as a failed request.

use kivo_core::event::TurnSource;
use kivo_core::tool::{ToolCall, ToolResult, ToolSpec};
use kivo_ipc::protocol::{ActivityItem, AuditItem, GrantItem};
use kivo_security::{Decision, Grant};
use kivo_store::Database;
use kivo_store::records::{ActivityRecord, AuditRecord, NewActivity, TurnRecord};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Writes Activity, audit and turn records. Cheap to clone.
#[derive(Clone)]
pub struct Recorder {
    db: Arc<Mutex<Database>>,
    /// Log transcripts and spoken replies (Privacy → conversation history).
    keep_content: bool,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn optional(id: &str) -> Option<String> {
    (!id.is_empty()).then(|| id.to_owned())
}

impl Recorder {
    pub fn new(db: Arc<Mutex<Database>>, keep_content: bool) -> Self {
        Self { db, keep_content }
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Database> {
        self.db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn add(&self, entry: NewActivity) {
        if let Err(e) = self.db().add_activity(&entry) {
            tracing::warn!(%e, "couldn't record activity");
        }
    }

    fn audit(&self, record: &AuditRecord) {
        if let Err(e) = self.db().append_audit(record) {
            tracing::error!(%e, "couldn't write the audit log");
        }
    }

    pub fn turn_started(&self, turn: &str, source: TurnSource) {
        let source = serde_json::to_value(source).unwrap_or_default();
        let source = source.as_str().unwrap_or("unknown").to_owned();
        if let Err(e) = self.db().start_turn(turn, now_ms(), &source) {
            tracing::warn!(%e, "couldn't record the turn");
        }
    }

    /// What KIVO heard (or the typed request).
    pub fn transcript(&self, turn: &str, text: &str) {
        self.add(NewActivity {
            ts: now_ms(),
            turn_id: optional(turn),
            task_id: None,
            kind: "transcript".into(),
            title: if self.keep_content {
                text.to_owned()
            } else {
                kivo_core::text::t("activity.notKept")
            },
            detail: None,
            status: "done".into(),
            data: None,
        });
    }

    /// The permission decision for a call (audited in every mode, SECURITY §7).
    pub fn tool_decision(&self, turn: &str, call: &ToolCall, spec: &ToolSpec, decision: &Decision) {
        let (decision_name, confirmed_by) = match decision {
            Decision::Allow(permit) => (
                "allow",
                Some(format!("{:?}", permit.confirmed_by()).to_lowercase()),
            ),
            Decision::Confirm(_) => ("confirm", None),
            Decision::Deny(_) => ("deny", None),
        };
        let risk = serde_json::to_value(spec.risk).unwrap_or_default();
        self.audit(&AuditRecord {
            ts: now_ms(),
            turn_id: optional(turn),
            task_id: None,
            tool: call.tool.clone(),
            args_summary: summarize(&call.args),
            risk: risk.as_str().unwrap_or("unknown").to_owned(),
            decision: decision_name.to_owned(),
            confirmed_by,
            result: None,
            error: match decision {
                Decision::Deny(d) => Some(d.message.clone()),
                _ => None,
            },
        });
        if let Decision::Deny(denial) = decision {
            self.add(NewActivity {
                ts: now_ms(),
                turn_id: optional(turn),
                task_id: None,
                kind: "tool".into(),
                title: kivo_security::render_title(&spec.title, &call.args),
                detail: Some(denial.message.clone()),
                status: "denied".into(),
                data: None,
            });
        }
    }

    /// A tool KIVO couldn't offer at all (its capability is off, CAP-02).
    pub fn tool_denied(&self, turn: &str, call: &ToolCall, message: &str) {
        self.audit(&AuditRecord {
            ts: now_ms(),
            turn_id: optional(turn),
            task_id: None,
            tool: call.tool.clone(),
            args_summary: summarize(&call.args),
            risk: "unknown".into(),
            decision: "deny".into(),
            confirmed_by: None,
            result: None,
            error: Some(message.to_owned()),
        });
    }

    /// How a call ended.
    pub fn tool_result(&self, turn: &str, call: &ToolCall, result: &ToolResult) {
        let (status, detail) = match &result.status {
            Ok(value) => ("done", summarize(value)),
            Err(e) => ("failed", e.message.clone()),
        };
        self.add(NewActivity {
            ts: now_ms(),
            turn_id: optional(turn),
            task_id: None,
            kind: "tool".into(),
            title: call.tool.clone(),
            detail: Some(detail.clone()),
            status: status.to_owned(),
            data: Some(serde_json::json!({ "durationMs": result.duration_ms })),
        });
        self.audit(&AuditRecord {
            ts: now_ms(),
            turn_id: optional(turn),
            task_id: None,
            tool: call.tool.clone(),
            args_summary: summarize(&call.args),
            risk: "unknown".into(),
            decision: "result".into(),
            confirmed_by: None,
            result: Some(status.to_owned()),
            error: result.status.as_ref().err().map(|e| e.message.clone()),
        });
    }

    /// The user answered a confirmation card.
    pub fn confirmation(
        &self,
        turn: &str,
        call: &ToolCall,
        allowed: bool,
        by: kivo_core::tool::ConfirmedBy,
    ) {
        self.audit(&AuditRecord {
            ts: now_ms(),
            turn_id: optional(turn),
            task_id: None,
            tool: call.tool.clone(),
            args_summary: summarize(&call.args),
            risk: "unknown".into(),
            decision: if allowed { "allow" } else { "deny" }.to_owned(),
            confirmed_by: Some(format!("{by:?}").to_lowercase()),
            result: None,
            error: None,
        });
    }

    /// "Always allow this" (SEC-08).
    pub fn add_grant(&self, call: &ToolCall) {
        let scope = call.targets.iter().find_map(|t| match t {
            kivo_core::tool::Target::App { id, .. } => Some(id.clone()),
            _ => None,
        });
        if let Err(e) = self
            .db()
            .add_grant(&call.tool, scope.as_deref(), now_ms(), None)
        {
            tracing::warn!(%e, "couldn't save the permission");
        }
    }

    /// The grants in force now.
    pub fn grants(&self) -> Vec<Grant> {
        self.db()
            .grants(now_ms())
            .unwrap_or_default()
            .into_iter()
            .map(|g| Grant {
                tool: g.tool,
                scope: g.scope,
            })
            .collect()
    }

    /// The grants as the Permissions page shows them (with their ids, so they can be revoked).
    pub fn grants_in_force(&self) -> Vec<GrantItem> {
        self.db()
            .grants(now_ms())
            .unwrap_or_default()
            .into_iter()
            .map(|g| GrantItem {
                id: g.id,
                tool: g.tool,
                scope: g.scope,
                created_at: g.created_at,
                expires_at: g.expires_at,
            })
            .collect()
    }

    pub fn revoke_grant(&self, id: i64) -> bool {
        self.db().revoke_grant(id).unwrap_or(false)
    }

    /// What KIVO said.
    pub fn answer(&self, turn: &str, text: &str, status: &str) {
        self.add(NewActivity {
            ts: now_ms(),
            turn_id: optional(turn),
            task_id: None,
            kind: "reply".into(),
            title: if self.keep_content {
                text.to_owned()
            } else {
                kivo_core::text::t("activity.notKept")
            },
            detail: None,
            status: status.to_owned(),
            data: None,
        });
    }

    /// Which brain answers and why (PLAN-17): an Activity row and the turn's route.
    pub fn brain_route(&self, turn: &str, reason: &str, data: Value) {
        self.add(NewActivity {
            ts: now_ms(),
            turn_id: optional(turn),
            task_id: None,
            kind: "brain".into(),
            title: reason.to_owned(),
            detail: None,
            status: "done".into(),
            data: Some(data),
        });
        if let Err(e) = self.db().set_turn_route(turn, reason, None) {
            tracing::warn!(%e, "couldn't record the route");
        }
    }

    /// A brain failed or was replaced by a fallback (BRAIN-22, PLAN-05).
    pub fn brain_problem(&self, turn: &str, title: &str, detail: &str) {
        self.add(NewActivity {
            ts: now_ms(),
            turn_id: optional(turn),
            task_id: None,
            kind: "brain".into(),
            title: title.to_owned(),
            detail: Some(detail.to_owned()),
            status: "failed".into(),
            data: None,
        });
    }

    /// The brain's hidden reasoning, kept only when the user turned the reasoning log on
    /// (BRAIN-09). It is never shown in Activity or spoken.
    pub fn reasoning(&self, turn: &str, route: &str, text: &str) {
        if let Err(e) = self.db().set_turn_route(turn, route, Some(text)) {
            tracing::warn!(%e, "couldn't keep the reasoning");
        }
    }

    /// The turn ended: fill in the record and its timings.
    pub fn turn_finished(
        &self,
        turn: &str,
        transcript: &str,
        outcome: &str,
        reply: Option<&str>,
        spans: &serde_json::Map<String, Value>,
    ) {
        let db = self.db();
        let existing = db.turn(turn).ok().flatten();
        let record = TurnRecord {
            id: turn.to_owned(),
            started_at: existing.as_ref().map_or_else(now_ms, |t| t.started_at),
            ended_at: Some(now_ms()),
            source: existing.map_or_else(|| "unknown".into(), |t| t.source),
            transcript: self.keep_content.then(|| transcript.to_owned()),
            route: None,
            outcome: Some(outcome.to_owned()),
            reply: self
                .keep_content
                .then(|| reply.unwrap_or_default().to_owned()),
        };
        if let Err(e) = db.finish_turn(&record) {
            tracing::warn!(%e, "couldn't finish the turn record");
        }
        if let Err(e) = db.record_turn_metrics(turn, &Value::Object(spans.clone())) {
            tracing::warn!(%e, "couldn't record the turn's timings");
        }
    }

    /// A capability was switched (CAP-03).
    pub fn capability_changed(&self, capability: kivo_core::Capability, on: bool) {
        self.audit(&AuditRecord {
            ts: now_ms(),
            turn_id: None,
            task_id: None,
            tool: "capabilities.set".into(),
            args_summary: kivo_core::text::tf(
                "activity.capabilitySet",
                &[
                    ("capability", &capability.label()),
                    (
                        "value",
                        &kivo_core::text::t(if on { "activity.on" } else { "activity.off" }),
                    ),
                ],
            ),
            risk: "medium".into(),
            decision: "allow".into(),
            confirmed_by: Some("click".into()),
            result: Some("ok".into()),
            error: None,
        });
        self.add(NewActivity {
            ts: now_ms(),
            turn_id: None,
            task_id: None,
            kind: "setting".into(),
            title: format!(
                "{} turned {}",
                capability.label(),
                if on { "on" } else { "off" }
            ),
            detail: None,
            status: "done".into(),
            data: None,
        });
    }

    /// Something the user did in the Control Center that the audit log keeps (consent to voice
    /// enrollment, deleting voice data, SECURITY §5): who (a click), what, and that it happened.
    pub fn user_action(&self, tool: &str, summary: &str) {
        self.audit(&AuditRecord {
            ts: now_ms(),
            turn_id: None,
            task_id: None,
            tool: tool.into(),
            args_summary: summary.into(),
            risk: "medium".into(),
            decision: "allow".into(),
            confirmed_by: Some("click".into()),
            result: Some("ok".into()),
            error: None,
        });
        self.add(NewActivity {
            ts: now_ms(),
            turn_id: None,
            task_id: None,
            kind: "setting".into(),
            title: summary.into(),
            detail: None,
            status: "done".into(),
            data: None,
        });
    }

    /// A KIVO process crashed last time (ARCH-10). The report stays on this PC.
    pub fn crash_reported(&self, report: &kivo_store::crashes::CrashReport) {
        self.add(NewActivity {
            ts: i64::try_from(report.at.saturating_mul(1000)).unwrap_or(i64::MAX),
            turn_id: None,
            task_id: None,
            kind: "crash".into(),
            title: kivo_core::text::tf("activity.crashed", &[("process", &report.process)]),
            detail: report.summary.clone(),
            status: "failed".into(),
            data: None,
        });
    }

    /// The emergency stop was used (SECURITY §8).
    pub fn emergency_stop(&self) {
        self.audit(&AuditRecord {
            ts: now_ms(),
            turn_id: None,
            task_id: None,
            tool: "permissions.emergencyStop".into(),
            args_summary: String::new(),
            risk: "safe".into(),
            decision: "allow".into(),
            confirmed_by: Some("click".into()),
            result: Some("stopped".into()),
            error: None,
        });
        self.add(NewActivity {
            ts: now_ms(),
            turn_id: None,
            task_id: None,
            kind: "stop".into(),
            title: kivo_core::text::t("activity.stoppedEverything"),
            detail: None,
            status: "done".into(),
            data: None,
        });
    }

    /// A page of the Activity timeline, newest first (UX-20).
    pub fn recent(&self, before: Option<i64>, limit: u32) -> Vec<ActivityItem> {
        self.db()
            .activity(before, limit.clamp(1, 200))
            .unwrap_or_default()
            .into_iter()
            .map(|a: ActivityRecord| ActivityItem {
                id: a.id,
                ts: a.ts,
                turn_id: a.turn_id,
                kind: a.kind,
                title: a.title,
                detail: a.detail,
                status: a.status,
            })
            .collect()
    }

    /// The newest audit rows (Activity → Audit, SEC-24).
    pub fn audit_rows(&self, limit: u32) -> Vec<AuditItem> {
        self.db()
            .recent_audit(limit.clamp(1, 500))
            .unwrap_or_default()
            .into_iter()
            .map(|a: AuditRecord| AuditItem {
                ts: a.ts,
                turn_id: a.turn_id,
                tool: a.tool,
                args_summary: a.args_summary,
                risk: a.risk,
                decision: a.decision,
                confirmed_by: a.confirmed_by,
                result: a.result,
                error: a.error,
            })
            .collect()
    }
}

/// A short, redacted summary of arguments for the audit log (never secrets, never whole documents).
fn summarize(value: &Value) -> String {
    let mut text = match value {
        Value::Object(map) => map
            .iter()
            .map(|(k, v)| {
                let shown = match v {
                    Value::String(s) => s.clone(),
                    Value::Object(o) => o
                        .get("name")
                        .and_then(|n| n.as_str())
                        .map_or_else(|| "…".to_owned(), str::to_owned),
                    other => other.to_string(),
                };
                format!("{k}={shown}")
            })
            .collect::<Vec<_>>()
            .join(", "),
        Value::Null => String::new(),
        other => other.to_string(),
    };
    if text.chars().count() > 200 {
        text = text.chars().take(197).collect::<String>() + "…";
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use kivo_core::tool::{Initiator, Target};
    use serde_json::json;

    fn recorder(keep: bool) -> Recorder {
        Recorder::new(Arc::new(Mutex::new(Database::in_memory().unwrap())), keep)
    }

    fn call() -> ToolCall {
        ToolCall {
            id: "c1".into(),
            tool: "apps.launch".into(),
            args: json!({"app": {"id": "chrome", "name": "Google Chrome"}}),
            initiated_by: Initiator::UserDirect,
            targets: vec![Target::App {
                id: "chrome".into(),
                name: "Google Chrome".into(),
            }],
        }
    }

    #[test]
    fn a_turn_is_recorded_from_transcript_to_reply() {
        let r = recorder(true);
        r.turn_started("t1", TurnSource::PushToTalk);
        r.transcript("t1", "open chrome");
        r.tool_result(
            "t1",
            &call(),
            &ToolResult {
                status: Ok(json!({"app": "Google Chrome"})),
                provenance: kivo_core::tool::Provenance::System,
                duration_ms: 12,
            },
        );
        r.answer("t1", "Opening Google Chrome.", "done");
        let mut spans = serde_json::Map::new();
        spans.insert("t10Complete".into(), json!(420));
        r.turn_finished(
            "t1",
            "open chrome",
            "done",
            Some("Opening Google Chrome."),
            &spans,
        );

        let timeline = r.recent(None, 10);
        let kinds: Vec<&str> = timeline.iter().map(|a| a.kind.as_str()).collect();
        assert_eq!(kinds, ["reply", "tool", "transcript"], "newest first");
        let db = r.db();
        let turn = db.turn("t1").unwrap().unwrap();
        assert_eq!(turn.outcome.as_deref(), Some("done"));
        assert_eq!(turn.transcript.as_deref(), Some("open chrome"));
        assert_eq!(db.turn_metrics("t1").unwrap().unwrap()["t10Complete"], 420);
    }

    #[test]
    fn transcripts_are_left_out_when_history_is_off() {
        let r = recorder(false);
        r.turn_started("t1", TurnSource::Typed);
        r.transcript("t1", "open my bank account");
        r.turn_finished(
            "t1",
            "open my bank account",
            "done",
            Some("Done."),
            &serde_json::Map::new(),
        );
        assert_eq!(r.recent(None, 1)[0].title, "(not kept)");
        assert_eq!(r.db().turn("t1").unwrap().unwrap().transcript, None);
    }

    #[test]
    fn every_decision_is_audited_and_the_chain_holds() {
        let r = recorder(true);
        let call = call();
        r.tool_denied("t1", &call, "Apps & windows is off. Turn it on?");
        r.confirmation("t1", &call, true, kivo_core::tool::ConfirmedBy::Click);
        r.capability_changed(kivo_core::Capability::Shell, true);
        r.emergency_stop();
        let rows = r.audit_rows(10);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].tool, "permissions.emergencyStop");
        assert_eq!(rows[2].confirmed_by.as_deref(), Some("click"));
        assert!(rows[3].error.as_deref().unwrap().contains("is off"));
        assert_eq!(r.db().verify_audit().unwrap().broken_at, None);
    }

    #[test]
    fn grants_round_trip_and_arguments_are_summarized_short() {
        let r = recorder(true);
        r.add_grant(&call());
        assert_eq!(
            r.grants(),
            [Grant {
                tool: "apps.launch".into(),
                scope: Some("chrome".into())
            }]
        );
        assert_eq!(
            summarize(&json!({"app": {"id": "c", "name": "Chrome"}})),
            "app=Chrome"
        );
        assert_eq!(summarize(&json!({"number": 30})), "number=30");
        let long = summarize(&json!({"text": "x".repeat(400)}));
        assert!(long.chars().count() <= 200 && long.ends_with('…'));
    }
}
