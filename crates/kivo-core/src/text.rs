//! The runtime's own words: the tray menu, notifications, spoken replies, action titles and
//! refusals (UX §9, UX-49). The Control Center and the Island translate through i18next; this is
//! the same idea for text the runtime produces, so a language is one more catalog file for each.
//!
//! Catalogs are JSON, nested like the UI's (`locales/en.json`); a key is the dotted path. English
//! is the source language and the fallback for a key another language lacks. Values take `{name}`
//! placeholders, and [`plural`] picks `<key>.one` or `<key>.other`.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::{OnceLock, RwLock};

/// Languages with a runtime catalog, as `(code, catalog)`.
const CATALOGS: &[(&str, &str)] = &[("en", include_str!("../locales/en.json"))];

type Catalog = HashMap<String, String>;

struct Texts {
    by_language: HashMap<&'static str, Catalog>,
    current: RwLock<&'static str>,
}

fn texts() -> &'static Texts {
    static TEXTS: OnceLock<Texts> = OnceLock::new();
    TEXTS.get_or_init(|| Texts {
        by_language: CATALOGS
            .iter()
            .map(|(code, json)| (*code, flatten(json)))
            .collect(),
        current: RwLock::new("en"),
    })
}

/// Dotted keys to strings; a catalog that fails to parse is a build defect the tests catch.
fn flatten(json: &str) -> Catalog {
    fn walk(prefix: &str, value: &serde_json::Value, out: &mut Catalog) {
        match value {
            serde_json::Value::Object(map) => {
                for (k, v) in map {
                    let key = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{prefix}.{k}")
                    };
                    walk(&key, v, out);
                }
            }
            serde_json::Value::String(s) => {
                out.insert(prefix.to_owned(), s.clone());
            }
            _ => {}
        }
    }
    let mut out = Catalog::new();
    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(value) => walk("", &value, &mut out),
        Err(e) => tracing::error!(error = %e, "a runtime text catalog does not parse"),
    }
    out
}

/// Switches the language; returns false (and keeps the current one) when there is no catalog.
pub fn set_language(code: &str) -> bool {
    let t = texts();
    let Some((&code, _)) = t.by_language.get_key_value(code) else {
        return false;
    };
    *t.current
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = code;
    true
}

/// The current language's code.
pub fn language() -> &'static str {
    *texts()
        .current
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Whether the English catalog has `key` (for tests that check every key the code uses).
pub fn has(key: &str) -> bool {
    texts()
        .by_language
        .get("en")
        .is_some_and(|c| c.contains_key(key))
}

fn lookup(key: &str) -> String {
    let t = texts();
    let current = language();
    t.by_language
        .get(current)
        .and_then(|c| c.get(key))
        .or_else(|| t.by_language.get("en").and_then(|c| c.get(key)))
        .cloned()
        .unwrap_or_else(|| {
            tracing::warn!(key, "missing runtime text");
            key.to_owned()
        })
}

/// The catalog key segment for an enum value: its serialized name (`followUp`, `apps-and-windows`).
pub fn key_of<T: serde::Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(s)) => s,
        _ => String::new(),
    }
}

/// The text for `key`.
pub fn t(key: &str) -> String {
    lookup(key)
}

/// The text for `key` with its `{name}` placeholders filled.
pub fn tf(key: &str, args: &[(&str, &dyn Display)]) -> String {
    fill(&lookup(key), args)
}

/// `<key>.one` when `count` is 1, else `<key>.other`; `{count}` and `args` are filled.
pub fn plural(key: &str, count: u64, args: &[(&str, &dyn Display)]) -> String {
    let form = if count == 1 { "one" } else { "other" };
    let mut all: Vec<(&str, &dyn Display)> = vec![("count", &count)];
    all.extend_from_slice(args);
    fill(&lookup(&format!("{key}.{form}")), &all)
}

/// Replaces `{name}` with its value; unknown placeholders stay as written.
pub fn fill(template: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                let name = &after[..close];
                match args.iter().find(|(n, _)| *n == name) {
                    Some((_, value)) => out.push_str(&value.to_string()),
                    None => {
                        out.push('{');
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = &after[close + 1..];
            }
            None => {
                out.push_str(&rest[open..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_english_catalog_parses_and_has_text() {
        assert!(texts().by_language["en"].len() > 50);
        assert_eq!(language(), "en");
    }

    #[test]
    fn fills_placeholders_and_keeps_unknown_ones() {
        assert_eq!(fill("Open {app}.", &[("app", &"Chrome")]), "Open Chrome.");
        assert_eq!(fill("{a} and {b}", &[("a", &1)]), "1 and {b}");
        assert_eq!(fill("no close {", &[]), "no close {");
    }

    #[test]
    fn plural_picks_the_form() {
        assert_eq!(plural("tray.tasks", 1, &[]), "1 task running");
        assert_eq!(plural("tray.tasks", 3, &[]), "3 tasks running");
    }

    #[test]
    fn a_missing_key_reads_as_itself_and_unknown_languages_are_refused() {
        assert_eq!(t("no.such.key"), "no.such.key");
        assert!(!set_language("xx"));
        assert!(set_language("en"));
    }
}
