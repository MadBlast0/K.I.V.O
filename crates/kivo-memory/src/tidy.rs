//! Keeping memory small (CONVERSATION §6 "Staying small"). These are the decisions; the runtime's
//! tidy job reads the vault, applies them, writes the files and reports in Activity.
//!
//! - Near-duplicate facts are merged (the older note stays, the newer one's tags join it).
//! - Of two current facts about the same thing, the older one is marked superseded
//!   (`valid_until` + `superseded_by`), never deleted: history is kept.
//! - Session logs older than 14 days are condensed into the workspace's `overview.md` (what was
//!   done) and `decisions.md` (dated decisions), then archived.
//! - A workspace over 50 KB of notes is condensed harder (logs older than 3 days, and only the
//!   latest bullets of "Earlier work" kept).

use crate::note::{FrontMatter, Kind, Note};
use crate::query::{similarity, subject};

/// Facts this alike are the same fact.
pub const DUPLICATE: f64 = 0.8;
/// Logs older than this many days are condensed.
pub const CONDENSE_AFTER_DAYS: i64 = 14;
/// …or older than this when the workspace is over its size cap.
pub const CONDENSE_HARD_AFTER_DAYS: i64 = 3;
/// A workspace's notes above this many bytes get condensed harder.
pub const WORKSPACE_CAP_BYTES: usize = 50 * 1024;
/// "Earlier work" keeps this many bullets when condensing harder.
const EARLIER_KEEP: usize = 40;
const EARLIER: &str = "## Earlier work";

/// How much a session log entry says (Memory → Note detail).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detail {
    Brief,
    Standard,
    Detailed,
}

impl Detail {
    pub fn parse(text: &str) -> Detail {
        match text {
            "brief" => Detail::Brief,
            "detailed" => Detail::Detailed,
            _ => Detail::Standard,
        }
    }

    /// The most bullets one entry keeps (Brief 1–3, Standard ≤ 10, Detailed everything).
    pub fn max_bullets(self) -> usize {
        match self {
            Detail::Brief => 3,
            Detail::Standard => 10,
            Detail::Detailed => usize::MAX,
        }
    }
}

/// A fact note, as the tidy rules see it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fact {
    pub path: String,
    pub text: String,
    /// Epoch ms.
    pub created: i64,
    /// Already superseded or expired.
    pub current: bool,
}

/// (keep, merge_away) pairs: each newer near-duplicate of an older current fact.
pub fn duplicates(facts: &[Fact]) -> Vec<(String, String)> {
    let mut sorted: Vec<&Fact> = facts.iter().filter(|f| f.current).collect();
    sorted.sort_by_key(|f| (f.created, f.path.clone()));
    let mut gone: Vec<&str> = Vec::new();
    let mut out = Vec::new();
    for (i, older) in sorted.iter().enumerate() {
        if gone.contains(&older.path.as_str()) {
            continue;
        }
        for newer in &sorted[i + 1..] {
            if !gone.contains(&newer.path.as_str())
                && similarity(&older.text, &newer.text) >= DUPLICATE
            {
                gone.push(&newer.path);
                out.push((older.path.clone(), newer.path.clone()));
            }
        }
    }
    out
}

/// (old, new) pairs: a current fact replaced by a newer current fact about the same thing.
pub fn superseded(facts: &[Fact]) -> Vec<(String, String)> {
    let mut sorted: Vec<&Fact> = facts.iter().filter(|f| f.current).collect();
    sorted.sort_by_key(|f| (f.created, f.path.clone()));
    let mut out = Vec::new();
    for (i, old) in sorted.iter().enumerate() {
        let Some(s) = subject(&old.text) else {
            continue;
        };
        if let Some(new) = sorted[i + 1..]
            .iter()
            .rev()
            .find(|n| subject(&n.text).as_deref() == Some(s.as_str()))
        {
            out.push((old.path.clone(), new.path.clone()));
        }
    }
    out
}

/// Marks `old` as superseded by the note at `new_path` from `date` on.
pub fn supersede(old: &mut Note, new_path: &str, date: &str) {
    old.front.valid_until = Some(date.to_owned());
    let target = new_path.strip_suffix(".md").unwrap_or(new_path);
    old.front.superseded_by = Some(format!("[[{target}]]"));
}

/// Merges `from` into `into`: tags join, and the text is added only if it says something new.
pub fn merge(into: &mut Note, from: &Note) {
    for t in &from.front.tags {
        if !into.front.tags.contains(t) {
            into.front.tags.push(t.clone());
        }
    }
    let (a, b) = (into.text(), from.text());
    if similarity(&a, &b) < 1.0 && !a.contains(b.trim()) {
        let merged = format!("{}\n\n{}\n", into.body.trim_end(), b.trim());
        into.body = merged;
    }
    // The stricter choice wins: sharing with cloud AI needs both to have allowed it.
    into.front.share_cloud = into.front.share_cloud && from.front.share_cloud;
}

fn bullets(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let t = l.trim_start();
            t.strip_prefix("- ").or_else(|| t.strip_prefix("* "))
        })
        .map(|b| b.trim().to_owned())
        .filter(|b| !b.is_empty())
        .collect()
}

/// A condensed bullet without the date KIVO added (`… (2026-09-02)`, `2026-09-02: …`), so the
/// same bullet isn't condensed twice.
fn core(bullet: &str) -> &str {
    let b = bullet.trim();
    let is_date = |d: &str| d.len() == 10 && crate::date::parse(d, 0).is_some();
    if let Some(open) = b.rfind(" (")
        && b.ends_with(')')
        && is_date(&b[open + 2..b.len() - 1])
    {
        return &b[..open];
    }
    if let Some((d, rest)) = b.split_once(": ")
        && is_date(d)
    {
        return rest;
    }
    b
}

fn is_decision(bullet: &str) -> bool {
    let lower = bullet.to_lowercase();
    lower.starts_with("decided")
        || lower.starts_with("decision")
        || lower.starts_with("we chose")
        || lower.starts_with("chose ")
        || lower.contains("#decision")
}

/// A new note of `kind` for a workspace, with its title.
pub fn workspace_note(kind: Kind, workspace: &str, title: &str, date: &str) -> Note {
    Note {
        front: FrontMatter {
            kind: Some(kind),
            workspace: Some(workspace.to_owned()),
            tags: vec!["workspace".into()],
            created: Some(date.to_owned()),
            updated: Some(date.to_owned()),
            ..FrontMatter::default()
        },
        body: format!("# {title}\n"),
    }
}

/// Condenses dated logs into the overview and decisions notes. Bullets already there (or nearly
/// so) aren't added again. Returns how many bullets were added to each.
pub fn condense(
    overview: &mut Note,
    decisions: &mut Note,
    logs: &[(String, Note)],
    today: &str,
) -> (usize, usize) {
    let mut known: Vec<String> = bullets(&overview.body)
        .iter()
        .chain(bullets(&decisions.body).iter())
        .map(|b| core(b).to_owned())
        .collect();
    let mut done = Vec::new();
    let mut decided = Vec::new();
    for (date, log) in logs {
        for b in bullets(&log.text()) {
            if known.iter().any(|k| similarity(k, &b) >= DUPLICATE) {
                continue;
            }
            known.push(b.clone());
            if is_decision(&b) {
                decided.push(format!("- {date}: {b}"));
            } else {
                done.push(format!("- {b} ({date})"));
            }
        }
    }
    if !done.is_empty() {
        let body = overview.body.trim_end().to_owned();
        overview.body = if body.contains(EARLIER) {
            // Add under the existing section, before any later heading.
            let at = body.find(EARLIER).unwrap_or(0) + EARLIER.len();
            let rest = &body[at..];
            let end = rest.find("\n## ").map_or(body.len(), |i| at + i);
            format!(
                "{}\n{}{}\n",
                body[..end].trim_end(),
                done.join("\n"),
                &body[end..]
            )
        } else {
            format!("{body}\n\n{EARLIER}\n{}\n", done.join("\n"))
        };
        overview.front.updated = Some(today.to_owned());
    }
    if !decided.is_empty() {
        decisions.body = format!("{}\n{}\n", decisions.body.trim_end(), decided.join("\n"));
        decisions.front.updated = Some(today.to_owned());
    }
    (done.len(), decided.len())
}

/// Keeps only the latest bullets of the overview's "Earlier work" (condensing harder). Returns
/// how many were dropped.
pub fn trim_earlier(overview: &mut Note) -> usize {
    let body = overview.body.clone();
    let Some(start) = body.find(EARLIER) else {
        return 0;
    };
    let from = start + EARLIER.len();
    let end = body[from..].find("\n## ").map_or(body.len(), |i| from + i);
    let section: Vec<&str> = body[from..end]
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    if section.len() <= EARLIER_KEEP {
        return 0;
    }
    let dropped = section.len() - EARLIER_KEEP;
    let kept = section[dropped..].join("\n");
    overview.body = format!("{}\n{kept}\n{}", body[..from].trim_end(), &body[end..]);
    dropped
}

/// A session-log entry at the chosen detail: a timestamped heading and its bullets.
pub fn log_entry(time: &str, what: &str, bullets: &[String], detail: Detail) -> String {
    let mut out = format!("\n## {time} · {what}\n");
    for b in bullets.iter().take(detail.max_bullets()) {
        out.push_str(&format!("- {}\n", b.trim()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fact(path: &str, text: &str, created: i64) -> Fact {
        Fact {
            path: path.into(),
            text: text.into(),
            created,
            current: true,
        }
    }

    #[test]
    fn newer_near_duplicates_merge_into_the_older_fact() {
        let facts = [
            fact("notes/b.md", "Run tests with cargo nextest", 2),
            fact("notes/a.md", "Tests run with cargo nextest", 1),
            fact("notes/c.md", "Maya prefers dark mode", 3),
            fact("notes/d.md", "tests: cargo nextest, run them", 4),
        ];
        assert_eq!(
            duplicates(&facts),
            [
                ("notes/a.md".to_owned(), "notes/b.md".to_owned()),
                ("notes/a.md".to_owned(), "notes/d.md".to_owned())
            ]
        );
        let mut a = Note::parse("---\ntags: [rust]\n---\nTests run with cargo nextest\n");
        let d = Note::parse("---\ntags: [testing]\n---\nAlso needs --workspace.\n");
        merge(&mut a, &d);
        assert_eq!(a.front.tags, ["rust", "testing"]);
        assert!(a.body.contains("--workspace"));
    }

    #[test]
    fn a_newer_fact_about_the_same_thing_supersedes_the_older() {
        let facts = [
            fact("notes/project-1.md", "My project is in D:\\work", 1),
            fact("notes/project-2.md", "my project is in E:\\code", 5),
            fact("notes/maya.md", "Maya works at Acme", 2),
        ];
        assert_eq!(
            superseded(&facts),
            [(
                "notes/project-1.md".to_owned(),
                "notes/project-2.md".to_owned()
            )]
        );
        let mut old = Note::parse("---\ntype: fact\n---\nMy project is in D:\\work\n");
        supersede(&mut old, "notes/project-2.md", "2026-09-23");
        assert_eq!(old.front.valid_until.as_deref(), Some("2026-09-23"));
        assert_eq!(
            old.front.superseded_by.as_deref(),
            Some("[[notes/project-2]]")
        );
    }

    #[test]
    fn old_logs_condense_into_the_overview_and_decisions() {
        let mut overview = workspace_note(Kind::Workspace, "k-i-v-o", "K.I.V.O", "2026-09-01");
        overview.body.push_str("- Rust runtime and Tauri UI\n");
        let mut decisions = workspace_note(Kind::Decisions, "k-i-v-o", "Decisions", "2026-09-01");
        let logs = vec![
            (
                "2026-09-02".to_owned(),
                Note::parse(
                    "# 2026-09-02\n## 10:00 · Fix tests\n- Fixed the overflow in add()\n- Decided to use nextest\n- Rust runtime and Tauri UI\n",
                ),
            ),
            (
                "2026-09-03".to_owned(),
                Note::parse(
                    "# 2026-09-03\n- Fixed the overflow in add()\n- Added the Island states\n",
                ),
            ),
        ];
        let (done, decided) = condense(&mut overview, &mut decisions, &logs, "2026-09-20");
        assert_eq!((done, decided), (2, 1));
        assert!(overview.body.contains("## Earlier work\n- Fixed the overflow in add() (2026-09-02)\n- Added the Island states (2026-09-03)"), "{}", overview.body);
        assert!(
            decisions
                .body
                .contains("- 2026-09-02: Decided to use nextest")
        );
        assert_eq!(overview.front.updated.as_deref(), Some("2026-09-20"));
        // Condensing again adds nothing.
        assert_eq!(
            condense(&mut overview, &mut decisions, &logs, "2026-09-21"),
            (0, 0)
        );
    }

    #[test]
    fn condensing_harder_keeps_the_latest_bullets() {
        let mut overview = workspace_note(Kind::Workspace, "w", "W", "2026-09-01");
        overview.body.push_str("\n## Earlier work\n");
        for i in 0..50 {
            overview.body.push_str(&format!("- item {i}\n"));
        }
        overview.body.push_str("\n## Links\n- [[Maya]]\n");
        assert_eq!(trim_earlier(&mut overview), 10);
        assert!(!overview.body.contains("- item 9\n") && overview.body.contains("- item 10\n"));
        assert!(overview.body.contains("## Links\n- [[Maya]]"));
    }

    #[test]
    fn log_entries_follow_the_detail_level() {
        let bullets: Vec<String> = (1..=12).map(|i| format!("step {i}")).collect();
        let brief = log_entry("10:02", "Fix tests", &bullets, Detail::Brief);
        assert_eq!(brief.matches("\n- ").count(), 3);
        assert_eq!(
            log_entry("10:02", "x", &bullets, Detail::Standard)
                .matches("\n- ")
                .count(),
            10
        );
        assert_eq!(
            log_entry("10:02", "x", &bullets, Detail::Detailed)
                .matches("\n- ")
                .count(),
            12
        );
    }
}
