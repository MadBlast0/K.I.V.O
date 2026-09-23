//! A note: YAML front-matter and a Markdown body.
//!
//! The front-matter is the small YAML subset Obsidian writes and reads: `key: value`, lists as
//! `[a, b]` or as `- item` lines under the key, and quoted strings. Keys KIVO doesn't know are
//! kept, in order, so rewriting a note the user edited never loses what they added.

use serde::{Deserialize, Serialize};

/// What a note is. Unknown types read as `Note`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A single fact the user asked KIVO to remember, or accepted when KIVO suggested it.
    Fact,
    /// About the user (`about-me.md`), always sent to brains as personal instructions.
    About,
    /// A workspace's or the user's instructions to KIVO.
    Instructions,
    Person,
    Workspace,
    Topic,
    Decisions,
    /// A day's session notes in a workspace (`workspaces/<name>/log/2026-09-23.md`).
    Log,
    Note,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Fact => "fact",
            Kind::About => "about",
            Kind::Instructions => "instructions",
            Kind::Person => "person",
            Kind::Workspace => "workspace",
            Kind::Topic => "topic",
            Kind::Decisions => "decisions",
            Kind::Log => "log",
            Kind::Note => "note",
        }
    }

    pub fn parse(text: &str) -> Kind {
        match text.trim().to_ascii_lowercase().as_str() {
            "fact" | "memory" => Kind::Fact,
            "about" | "about-me" => Kind::About,
            "instructions" => Kind::Instructions,
            "person" | "people" => Kind::Person,
            "workspace" | "project" | "overview" => Kind::Workspace,
            "topic" => Kind::Topic,
            "decisions" | "decision" => Kind::Decisions,
            "log" | "session" => Kind::Log,
            _ => Kind::Note,
        }
    }

    /// Notes that are about one thing, so they are an entity in the graph.
    pub fn is_entity(self) -> bool {
        matches!(self, Kind::Person | Kind::Workspace | Kind::Topic)
    }

    /// The type a note gets from where it is, when its front-matter doesn't say.
    pub fn from_path(path: &str) -> Kind {
        let lower = path.to_ascii_lowercase();
        let file = lower.rsplit('/').next().unwrap_or(&lower);
        if lower == "about-me.md" {
            Kind::About
        } else if file == "instructions.md" {
            Kind::Instructions
        } else if lower.starts_with("people/") {
            Kind::Person
        } else if lower.starts_with("workspaces/") {
            if lower.contains("/log/") {
                Kind::Log
            } else if file == "decisions.md" {
                Kind::Decisions
            } else if file == "overview.md" {
                Kind::Workspace
            } else {
                Kind::Note
            }
        } else if lower.starts_with("topics/") {
            Kind::Topic
        } else if lower.starts_with("notes/") {
            Kind::Fact
        } else {
            Kind::Note
        }
    }
}

/// The front-matter KIVO reads and writes. Dates are `YYYY-MM-DD` as written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FrontMatter {
    pub kind: Option<Kind>,
    pub tags: Vec<String>,
    pub workspace: Option<String>,
    /// A data class (`normal`, `personal`, `sensitive`, …).
    pub sensitivity: Option<String>,
    /// `global`, `app:<id>` or `project:<path>`.
    pub scope: Option<String>,
    pub created: Option<String>,
    pub updated: Option<String>,
    pub valid_until: Option<String>,
    /// The note that replaced this one (`[[notes/…]]`).
    pub superseded_by: Option<String>,
    /// Where it came from (`turn:<id>`, `task:<id>`, `user`).
    pub source: Option<String>,
    /// The user allowed this sensitive note to go to cloud AI (MEM-07).
    pub share_cloud: bool,
    /// Keys KIVO doesn't know, as written (`key`, raw lines after the key line).
    pub extra: Vec<(String, String)>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Note {
    pub front: FrontMatter,
    pub body: String,
}

fn unquote(value: &str) -> String {
    let v = value.trim();
    if v.len() >= 2
        && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\'')))
    {
        return v[1..v.len() - 1].replace("\\\"", "\"").replace("''", "'");
    }
    v.to_owned()
}

fn list(value: &str, items: &[String]) -> Vec<String> {
    let v = value.trim();
    let raw: Vec<String> =
        if let Some(inner) = v.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            inner.split(',').map(unquote).collect()
        } else if v.is_empty() {
            items.iter().map(|s| unquote(s)).collect()
        } else {
            // `tags: rust, testing` (not YAML, but people write it).
            v.split(',').map(unquote).collect()
        };
    raw.into_iter()
        .map(|t| t.trim().trim_start_matches('#').to_owned())
        .filter(|t| !t.is_empty())
        .collect()
}

fn quote_if_needed(value: &str) -> String {
    let needs = value.is_empty()
        || value.starts_with([
            '[', '{', '"', '\'', '#', '&', '*', '!', '|', '>', '%', '@', '`', '-',
        ])
        || value.contains(": ")
        || value.contains(" #")
        || value.ends_with(':');
    if needs {
        format!("\"{}\"", value.replace('"', "\\\""))
    } else {
        value.to_owned()
    }
}

impl Note {
    /// Reads a note. A file without front-matter is all body.
    pub fn parse(text: &str) -> Note {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let normalized = text.replace("\r\n", "\n");
        let Some(rest) = normalized.strip_prefix("---\n") else {
            return Note {
                front: FrontMatter::default(),
                body: normalized,
            };
        };
        let Some(end) = rest.find("\n---\n").map(|i| (i, i + 5)).or_else(|| {
            rest.strip_suffix("\n---")
                .map(|r| (r.len(), rest.len()))
                .or_else(|| rest.starts_with("---\n").then_some((0, 4)))
        }) else {
            return Note {
                front: FrontMatter::default(),
                body: normalized,
            };
        };
        let (yaml, body) = (&rest[..end.0], &rest[end.1..]);
        let mut front = FrontMatter::default();
        let lines: Vec<&str> = yaml.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            i += 1;
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            if line.starts_with([' ', '\t', '-']) {
                continue;
            }
            // The lines that belong to this key (a block list or folded text).
            let mut block = Vec::new();
            while i < lines.len()
                && (lines[i].starts_with([' ', '\t']) || lines[i].starts_with("- "))
            {
                block.push(lines[i]);
                i += 1;
            }
            let items: Vec<String> = block
                .iter()
                .filter_map(|l| l.trim_start().strip_prefix("- ").map(str::to_owned))
                .collect();
            let key = key.trim();
            let one = || Some(unquote(value)).filter(|v| !v.is_empty());
            match key {
                "type" => front.kind = one().map(|v| Kind::parse(&v)),
                "tags" => front.tags = list(value, &items),
                "workspace" => front.workspace = one(),
                "sensitivity" => front.sensitivity = one(),
                "scope" => front.scope = one(),
                "created" => front.created = one(),
                "updated" => front.updated = one(),
                "valid_until" => front.valid_until = one(),
                "superseded_by" => front.superseded_by = one(),
                "source" => front.source = one(),
                "share_cloud" => front.share_cloud = unquote(value).eq_ignore_ascii_case("true"),
                _ => {
                    let mut raw = value.to_owned();
                    for l in &block {
                        raw.push('\n');
                        raw.push_str(l);
                    }
                    front.extra.push((key.to_owned(), raw));
                }
            }
        }
        Note {
            front,
            body: body.to_owned(),
        }
    }

    /// Writes the note back, known keys first in a fixed order, then the user's own keys.
    pub fn render(&self) -> String {
        let f = &self.front;
        let mut out = String::from("---\n");
        let mut kv = |k: &str, v: &Option<String>| {
            if let Some(v) = v {
                out.push_str(&format!("{k}: {}\n", quote_if_needed(v)));
            }
        };
        kv("type", &f.kind.map(|k| k.as_str().to_owned()));
        kv("workspace", &f.workspace);
        kv("scope", &f.scope);
        kv("sensitivity", &f.sensitivity);
        kv("created", &f.created);
        kv("updated", &f.updated);
        kv("valid_until", &f.valid_until);
        kv("superseded_by", &f.superseded_by);
        kv("source", &f.source);
        if f.share_cloud {
            out.push_str("share_cloud: true\n");
        }
        if !f.tags.is_empty() {
            let tags: Vec<String> = f.tags.iter().map(|t| quote_if_needed(t)).collect();
            out.push_str(&format!("tags: [{}]\n", tags.join(", ")));
        }
        for (k, raw) in &f.extra {
            out.push_str(&format!("{k}:{raw}\n"));
        }
        out.push_str("---\n");
        out.push_str(&self.body);
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out
    }

    /// The note's title: its first `# ` heading, else the file name.
    pub fn title(&self, path: &str) -> String {
        self.body
            .lines()
            .find_map(|l| l.strip_prefix("# ").map(|t| t.trim().to_owned()))
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| {
                let file = path.rsplit('/').next().unwrap_or(path);
                file.strip_suffix(".md").unwrap_or(file).to_owned()
            })
    }

    /// The body without its title heading, for search and for brains.
    pub fn text(&self) -> String {
        let mut skipped = false;
        self.body
            .lines()
            .filter(|l| {
                if !skipped && l.starts_with("# ") {
                    skipped = true;
                    return false;
                }
                true
            })
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_owned()
    }

    /// Tags from the front-matter and `#tags` in the body, lower-case, without duplicates.
    pub fn all_tags(&self) -> Vec<String> {
        let mut tags: Vec<String> = self.front.tags.iter().map(|t| t.to_lowercase()).collect();
        tags.extend(crate::graph::inline_tags(&self.body));
        let mut seen = std::collections::HashSet::new();
        tags.retain(|t| seen.insert(t.clone()));
        tags
    }

    /// The kind from the front-matter, else from where the note is.
    pub fn kind(&self, path: &str) -> Kind {
        self.front.kind.unwrap_or_else(|| Kind::from_path(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_matter_round_trips_and_keeps_unknown_keys() {
        let text = "---\ntype: person\ntags:\n  - friend\n  - \"#design\"\naliases: [Maya S]\ncssclasses:\n  - wide\nsensitivity: personal\ncreated: 2026-09-01\n---\n# Maya\n\n- Design reviews on Thursdays\n";
        let note = Note::parse(text);
        assert_eq!(note.front.kind, Some(Kind::Person));
        assert_eq!(note.front.tags, ["friend", "design"]);
        assert_eq!(note.front.sensitivity.as_deref(), Some("personal"));
        assert_eq!(note.title("people/maya.md"), "Maya");
        assert_eq!(note.text(), "- Design reviews on Thursdays");
        let again = Note::parse(&note.render());
        assert_eq!(again, note);
        let rendered = note.render();
        assert!(rendered.contains("aliases: [Maya S]"), "{rendered}");
        assert!(rendered.contains("cssclasses:\n  - wide"), "{rendered}");
    }

    #[test]
    fn plain_markdown_and_odd_files_are_all_body() {
        let note = Note::parse("Just a line.\r\nAnother.");
        assert_eq!(note.front, FrontMatter::default());
        assert_eq!(note.body, "Just a line.\nAnother.");
        assert_eq!(note.title("topics/rust-testing.md"), "rust-testing");
        // An opening fence with no end is not front-matter.
        let open = Note::parse("---\ntype: fact\nno end");
        assert_eq!(open.front.kind, None);
        // Windows line endings and a BOM.
        let crlf = Note::parse("\u{feff}---\r\ntype: fact\r\ntags: [a, b]\r\n---\r\nBody\r\n");
        assert_eq!(crlf.front.kind, Some(Kind::Fact));
        assert_eq!(crlf.front.tags, ["a", "b"]);
        assert_eq!(crlf.body, "Body\n");
    }

    #[test]
    fn values_that_look_like_yaml_are_quoted() {
        let note = Note {
            front: FrontMatter {
                kind: Some(Kind::Fact),
                source: Some("turn: 42".into()),
                superseded_by: Some("[[notes/project-folder-2]]".into()),
                tags: vec!["c#".into()],
                ..FrontMatter::default()
            },
            body: "My project is in D:\\work.".into(),
        };
        let text = note.render();
        assert!(text.contains("source: \"turn: 42\""), "{text}");
        assert!(
            text.contains("superseded_by: \"[[notes/project-folder-2]]\""),
            "{text}"
        );
        assert_eq!(
            Note::parse(&text),
            Note {
                body: "My project is in D:\\work.\n".into(),
                ..note
            }
        );
    }

    #[test]
    fn kinds_come_from_the_folder() {
        assert_eq!(Kind::from_path("about-me.md"), Kind::About);
        assert_eq!(Kind::from_path("people/maya.md"), Kind::Person);
        assert_eq!(
            Kind::from_path("workspaces/k-i-v-o/overview.md"),
            Kind::Workspace
        );
        assert_eq!(
            Kind::from_path("workspaces/k-i-v-o/log/2026-09-21.md"),
            Kind::Log
        );
        assert_eq!(
            Kind::from_path("workspaces/k-i-v-o/decisions.md"),
            Kind::Decisions
        );
        assert_eq!(
            Kind::from_path("workspaces/k-i-v-o/instructions.md"),
            Kind::Instructions
        );
        assert_eq!(Kind::from_path("topics/rust-testing.md"), Kind::Topic);
        assert_eq!(Kind::from_path("notes/project-folder.md"), Kind::Fact);
        assert_eq!(Kind::from_path("Inbox.md"), Kind::Note);
    }
}
