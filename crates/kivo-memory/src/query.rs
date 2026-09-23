//! Text helpers for finding and comparing memories: the FTS5 query for a request, a token
//! estimate for the memory budget, and what a fact is *about* (so "my project is in E:\code"
//! supersedes "my project is in D:\work").

use std::collections::HashSet;

/// Words that say nothing about what to look for.
const STOP: &[&str] = &[
    "a", "about", "after", "again", "all", "also", "am", "an", "and", "any", "are", "as", "at",
    "be", "been", "but", "by", "can", "could", "did", "do", "does", "for", "from", "get", "give",
    "had", "has", "have", "he", "her", "here", "him", "his", "how", "i", "if", "in", "into", "is",
    "it", "its", "just", "kivo", "know", "let", "like", "me", "my", "no", "not", "now", "of", "on",
    "or", "our", "please", "she", "should", "so", "some", "tell", "than", "that", "the", "their",
    "them", "then", "there", "these", "they", "this", "to", "up", "us", "want", "was", "we",
    "were", "what", "when", "where", "which", "who", "why", "will", "with", "would", "you", "your",
    "hey",
];

/// The words of `text` worth searching for, lower-case, without duplicates.
pub fn keywords(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    text.split(|c: char| !c.is_alphanumeric() && c != '\'' && c != '-')
        .map(|w| w.trim_matches(['\'', '-']).to_lowercase())
        .filter(|w| w.chars().count() >= 2 && !STOP.contains(&w.as_str()))
        .filter(|w| seen.insert(w.clone()))
        .collect()
}

/// An FTS5 `MATCH` expression for `text`: its keywords as quoted prefix terms joined by OR
/// (FTS5's bm25 ranks notes with more of them first). `None` when nothing is worth searching.
pub fn fts_query(text: &str) -> Option<String> {
    let words = keywords(text);
    if words.is_empty() {
        return None;
    }
    Some(
        words
            .iter()
            .take(16)
            .map(|w| format!("\"{}\"*", w.replace('"', "")))
            .collect::<Vec<_>>()
            .join(" OR "),
    )
}

/// About 4 characters per token, as the context budgets count (CONVERSATION §8).
pub fn tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

/// Verbs that join a fact's subject to its value ("my project **is in** D:\work").
const LINKS: &[&str] = &[
    " is ",
    " are ",
    " was ",
    " lives ",
    " live ",
    " uses ",
    " use ",
    " prefers ",
    " prefer ",
    " likes ",
    " like ",
    " works ",
    " work ",
    " has ",
    " have ",
    " = ",
    ": ",
    " goes ",
    " called ",
    " named ",
];

/// What a fact is about: its words up to the first linking verb, normalized (`my project`).
/// `None` when the fact has no such shape, so it never supersedes anything.
pub fn subject(fact: &str) -> Option<String> {
    let lower = format!(" {} ", fact.trim().to_lowercase());
    let at = LINKS
        .iter()
        .filter_map(|l| lower.find(l).map(|i| (i, l.len())))
        .min_by_key(|(i, _)| *i)?;
    let head: Vec<&str> = lower[..at.0]
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty() && !matches!(*w, "the" | "a" | "an" | "remember" | "that"))
        .collect();
    // The verb itself belongs to the subject ("lives" vs "works" are different facts).
    let verb = lower[at.0..at.0 + at.1].trim().to_owned();
    (!head.is_empty() && head.len() <= 6).then(|| format!("{} {verb}", head.join(" ")))
}

/// How alike two texts are (Jaccard over their keywords), for near-duplicate notes.
#[allow(clippy::cast_precision_loss, reason = "word counts are small")]
pub fn similarity(a: &str, b: &str) -> f64 {
    let a: HashSet<String> = keywords(a).into_iter().collect();
    let b: HashSet<String> = keywords(b).into_iter().collect();
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    a.intersection(&b).count() as f64 / a.union(&b).count() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_become_fts_queries() {
        assert_eq!(
            fts_query("Hey Kivo, where is my project folder?").as_deref(),
            Some("\"project\"* OR \"folder\"*")
        );
        assert_eq!(fts_query("what is it?"), None);
        assert_eq!(
            keywords("Rust's \"tests\" -- rust"),
            ["rust's", "tests", "rust"]
        );
    }

    #[test]
    fn facts_about_the_same_thing_have_the_same_subject() {
        assert_eq!(
            subject("My project is in D:\\work").as_deref(),
            Some("my project is")
        );
        assert_eq!(
            subject("Remember that my project is in E:\\code now"),
            subject("my project is in D:\\work")
        );
        assert_ne!(subject("Maya works at Acme"), subject("Maya lives in Pune"));
        assert_eq!(subject("Thursday design reviews"), None);
    }

    #[test]
    fn near_duplicates_are_alike() {
        assert!(
            similarity(
                "Tests run with cargo nextest",
                "Run tests with cargo nextest"
            ) > 0.7
        );
        assert!(similarity("Tests run with cargo nextest", "Maya prefers dark mode") < 0.1);
        assert_eq!(tokens("12345678"), 2);
    }
}
