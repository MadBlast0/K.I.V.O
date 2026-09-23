//! The knowledge graph a note describes (CONV-17): links, tags, and what the graph tables hold.
//!
//! - An **entity** is a person, workspace or topic note (its title is the entity's name), or the
//!   target of a `[[wikilink]]`.
//! - A **relation** is a link from an entity note to another entity (`links_to`), or from a
//!   workspace note to a person or topic it mentions.
//! - An **observation** is a fact about an entity: each bullet of an entity note, and each fact
//!   note about the entities it links to. Observations carry the note's validity window, so a
//!   superseded fact stays as history ("was true until …").

use crate::note::{Kind, Note};
use regex::Regex;
use std::sync::LazyLock;

static WIKILINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([^\]\|#]+)(?:#[^\]\|]*)?(?:\|[^\]]*)?\]\]").expect("regex"));
static TAG: LazyLock<Regex> = LazyLock::new(|| {
    // `#tag` at the start or after a space; not headings (`# `), not `#123` alone, not in links.
    Regex::new(r"(?:^|\s)#([A-Za-z][\w\-/]*)").expect("regex")
});

/// The targets of `[[wikilinks]]` (`[[Maya]]`, `[[people/maya|Maya]]`, `[[Island#States]]`), in
/// order, without duplicates.
pub fn links(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in WIKILINK.captures_iter(body) {
        let target = c[1].trim().to_owned();
        if !target.is_empty() && !out.iter().any(|t| t.eq_ignore_ascii_case(&target)) {
            out.push(target);
        }
    }
    out
}

/// `#tags` written in the body (outside code), lower-case.
pub fn inline_tags(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_code = false;
    for line in body.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        let without_code: String = line.split('`').step_by(2).collect();
        for c in TAG.captures_iter(&without_code) {
            let tag = c[1].to_lowercase();
            if !out.contains(&tag) {
                out.push(tag);
            }
        }
    }
    out
}

/// The entity name a link target stands for: `people/maya` → `maya`, `Maya.md` → `Maya`.
pub fn entity_name(target: &str) -> String {
    let last = target.rsplit('/').next().unwrap_or(target);
    last.strip_suffix(".md").unwrap_or(last).trim().to_owned()
}

/// One fact about an entity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub entity: String,
    pub text: String,
}

/// What one note adds to the graph.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Contribution {
    /// Entities the note defines or mentions, with their kind (`person`, `workspace`, `topic`,
    /// or `thing` for a link to something without a note).
    pub entities: Vec<(String, String)>,
    /// (from, kind, to).
    pub relations: Vec<(String, String, String)>,
    pub observations: Vec<Observation>,
}

fn bullets(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let t = l.trim_start();
            t.strip_prefix("- ")
                .or_else(|| t.strip_prefix("* "))
                .map(|b| b.trim().to_owned())
        })
        .filter(|b| !b.is_empty())
        .collect()
}

/// The graph contribution of the note at `path`.
pub fn contribution(path: &str, note: &Note) -> Contribution {
    let mut out = Contribution::default();
    let kind = note.kind(path);
    let title = note.title(path);
    let targets: Vec<String> = links(&note.body).iter().map(|t| entity_name(t)).collect();
    for t in &targets {
        out.entities.push((t.clone(), "thing".into()));
    }
    if kind.is_entity() {
        out.entities
            .insert(0, (title.clone(), kind.as_str().to_owned()));
        for t in &targets {
            if !t.eq_ignore_ascii_case(&title) {
                out.relations
                    .push((title.clone(), "links_to".into(), t.clone()));
            }
        }
        for b in bullets(&note.text()) {
            out.observations.push(Observation {
                entity: title.clone(),
                text: b,
            });
        }
    } else if matches!(kind, Kind::Fact | Kind::Decisions | Kind::Log | Kind::Note) {
        // A fact is about the things it links to.
        let facts = if kind == Kind::Fact {
            vec![note.text()]
        } else {
            bullets(&note.text())
        };
        for fact in facts {
            for t in links(&fact).iter().map(|t| entity_name(t)) {
                out.observations.push(Observation {
                    entity: t,
                    text: fact.clone(),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_and_tags_are_found() {
        let body = "# K.I.V.O\nOverlay: [[Island]], see [[Island#States]] and [[people/maya|Maya]].\n#rust and #design/ui, not # heading or `#code` or #1\n```\n#nope\n```\n";
        assert_eq!(links(body), ["Island", "people/maya"]);
        assert_eq!(inline_tags(body), ["rust", "design/ui"]);
        assert_eq!(entity_name("people/maya"), "maya");
    }

    #[test]
    fn entity_notes_give_relations_and_observations() {
        let note = Note::parse(
            "---\ntype: workspace\n---\n# K.I.V.O\n- Overlay concept: [[Island]], top center\n- Design reviews with [[Maya]] on Thursdays\n",
        );
        let c = contribution("workspaces/k-i-v-o/overview.md", &note);
        assert_eq!(c.entities[0], ("K.I.V.O".into(), "workspace".into()));
        assert!(
            c.relations
                .contains(&("K.I.V.O".into(), "links_to".into(), "Maya".into()))
        );
        assert_eq!(c.observations.len(), 2);
        assert_eq!(c.observations[1].entity, "K.I.V.O");
    }

    #[test]
    fn a_fact_is_about_what_it_links_to() {
        let note = Note::parse("---\ntype: fact\n---\n[[Maya]] prefers dark mode.\n");
        let c = contribution("notes/maya-dark-mode.md", &note);
        assert!(c.relations.is_empty());
        assert_eq!(
            c.observations,
            [Observation {
                entity: "Maya".into(),
                text: "[[Maya]] prefers dark mode.".into()
            }]
        );
    }
}
