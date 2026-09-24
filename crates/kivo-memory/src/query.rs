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

/// What pins a fact down: its names (capitalized words, not sentence openers), numbers, times and
/// paths. Two facts that say the same thing in other words have the same specifics; "the standup
/// is at 10am" and "… at 11am" don't, however alike they read.
pub fn specifics(text: &str) -> std::collections::BTreeSet<String> {
    // Capitalized words that open sentences rather than name something.
    const OPENERS: &[&str] = &[
        "i", "my", "the", "a", "an", "our", "we", "it", "this", "that", "his", "her", "their",
        "your", "he", "she", "they", "there",
    ];
    text.split_whitespace()
        .filter_map(|w| {
            let w = w
                .trim_matches(|c: char| matches!(c, '.' | ',' | ';' | '!' | '?' | '"' | '(' | ')'));
            let w = w
                .strip_suffix("'s")
                .or_else(|| w.strip_suffix("’s"))
                .unwrap_or(w);
            let pinned = w.chars().any(|c| c.is_ascii_digit())
                || w.contains(['/', '\\', ':', '@'])
                || (w.chars().next().is_some_and(char::is_uppercase)
                    && !OPENERS.contains(&w.to_lowercase().as_str()));
            (pinned && !w.is_empty()).then(|| w.to_lowercase())
        })
        .collect()
}

/// Hybrid search (CONV-23): merges ranked lists (keyword matches, meaning matches) by reciprocal
/// rank fusion, so an item high in either list, or present in both, comes first. `key` names an
/// item across lists; each item is kept once, from the first list that has it.
pub fn fuse<T: Clone>(lists: &[Vec<T>], key: impl Fn(&T) -> String) -> Vec<T> {
    // The usual constant: dampens the difference between the very top ranks.
    const K: f64 = 60.0;
    let mut scores: Vec<(String, f64, T)> = Vec::new();
    for list in lists {
        for (rank, item) in list.iter().enumerate() {
            #[allow(clippy::cast_precision_loss, reason = "ranks are small")]
            let score = 1.0 / (K + rank as f64 + 1.0);
            let k = key(item);
            match scores.iter_mut().find(|(id, _, _)| *id == k) {
                Some(entry) => entry.1 += score,
                None => scores.push((k, score, item.clone())),
            }
        }
    }
    scores.sort_by(|a, b| b.1.total_cmp(&a.1));
    scores.into_iter().map(|(_, _, item)| item).collect()
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

    #[test]
    fn fusion_puts_items_in_both_lists_first() {
        let words = vec!["a", "b", "c"];
        let meaning = vec!["d", "c", "a"];
        let fused = fuse(&[words, meaning], |s| (*s).to_owned());
        assert_eq!(fused[..2], ["a", "c"]);
        assert_eq!(fused.len(), 4);
        assert_eq!(fuse::<&str>(&[], |s| (*s).to_owned()), Vec::<&str>::new());
    }

    #[test]
    fn specifics_pin_a_fact_down() {
        assert_eq!(
            specifics("My sister's name is Priya"),
            specifics("My sister is called Priya.")
        );
        assert_ne!(
            specifics("The standup is at 10am"),
            specifics("The standup is at 11am")
        );
        assert_ne!(
            specifics("My project is in D:/work"),
            specifics("My project is in C:/code")
        );
        assert!(specifics("I prefer dark mode").is_empty());
    }
}
