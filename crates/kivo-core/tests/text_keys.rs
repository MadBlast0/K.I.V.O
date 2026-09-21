//! Every literal key the runtime asks the text catalog for exists in English (UX-49), so a typo
//! can't reach the user as a raw key.

use std::path::Path;

fn sources(dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if !path.ends_with("target") && !path.ends_with("node_modules") {
                sources(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "rs")
            && let Ok(text) = std::fs::read_to_string(&path)
        {
            out.push(text);
        }
    }
}

/// The first argument of each `text::t("…")`, `text::tf("…"` and `text::plural("…"` call.
fn keys(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for call in ["t(\"", "tf(\"", "plural(\""] {
        let pattern = format!("text::{call}");
        let mut rest = source;
        while let Some(at) = rest.find(&pattern) {
            let after = &rest[at + pattern.len()..];
            if let Some(end) = after.find('"') {
                let key = &after[..end];
                // Keys are dotted identifiers; anything else is prose (docs mentioning the calls).
                if !key.is_empty()
                    && key
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
                {
                    out.push(key.to_owned());
                }
            }
            rest = after;
        }
    }
    out
}

#[test]
fn every_key_used_in_the_code_is_in_the_catalog() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    sources(&root.join("crates"), &mut files);
    sources(&root.join("apps"), &mut files);
    let used: Vec<String> = files.iter().flat_map(|f| keys(f)).collect();
    assert!(used.len() > 50, "found only {} keys", used.len());
    let missing: Vec<&String> = used
        .iter()
        .filter(|k| k.as_str() != "no.such.key")
        .filter(|k| {
            // Plural keys name the parent of `.one` / `.other`.
            !kivo_core::text::has(k) && !kivo_core::text::has(&format!("{k}.other"))
        })
        .collect();
    assert!(
        missing.is_empty(),
        "missing from locales/en.json: {missing:?}"
    );
}
