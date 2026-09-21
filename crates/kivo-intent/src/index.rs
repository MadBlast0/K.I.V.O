//! Live indexes the grammar resolves slots against (BRAINS §2): installed apps and open windows.
//! Matching is fuzzy, because transcripts are imperfect ("crome") and people use short names
//! ("Word" for "Microsoft Word").

use serde::{Deserialize, Serialize};

/// Something a slot resolved to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resolved {
    pub id: String,
    pub name: String,
    /// 0–1: how sure the match is.
    pub score: f32,
}

/// An entry in an index: an id, its display name and other names.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexEntry {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
}

/// Matches below this are not trusted: the request goes on to the next router stage instead.
pub const MIN_SCORE: f32 = 0.75;

#[derive(Clone, Debug, Default)]
pub struct Index {
    entries: Vec<(IndexEntry, Vec<Vec<String>>)>,
}

impl Index {
    pub fn new(entries: Vec<IndexEntry>) -> Self {
        let entries = entries
            .into_iter()
            .map(|e| {
                let names = std::iter::once(&e.name)
                    .chain(&e.aliases)
                    .map(|n| name_words(n))
                    .filter(|w| !w.is_empty())
                    .collect();
                (e, names)
            })
            .collect();
        Self { entries }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The best entry for the spoken words, if it scores at least `MIN_SCORE`.
    pub fn find(&self, spoken: &[String]) -> Option<Resolved> {
        let spoken: Vec<String> = spoken.iter().filter(|w| !is_article(w)).cloned().collect();
        if spoken.is_empty() {
            return None;
        }
        let mut best: Option<(f32, usize, &IndexEntry)> = None;
        for (entry, names) in &self.entries {
            for name in names {
                let score = similarity(&spoken, name);
                let len = name.len();
                let better = match best {
                    None => true,
                    // Prefer the higher score; on a tie, the shorter (more exact) name.
                    Some((s, l, _)) => score > s + 1e-6 || ((score - s).abs() <= 1e-6 && len < l),
                };
                if better {
                    best = Some((score, len, entry));
                }
            }
        }
        best.filter(|(s, _, _)| *s >= MIN_SCORE)
            .map(|(score, _, e)| Resolved {
                id: e.id.clone(),
                name: e.name.clone(),
                score,
            })
    }
}

fn is_article(w: &str) -> bool {
    matches!(w, "the" | "a" | "an" | "my")
}

/// A name's words, lower case, without trademark noise ("(x86)", "®").
fn name_words(name: &str) -> Vec<String> {
    let cleaned: String = name
        .to_lowercase()
        .replace("(x86)", " ")
        .replace("(x64)", " ")
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '+' || c == '#' {
                c
            } else {
                ' '
            }
        })
        .collect();
    cleaned.split_whitespace().map(String::from).collect()
}

/// How well spoken words match a name, 0–1.
fn similarity(spoken: &[String], name: &[String]) -> f32 {
    if spoken == name {
        return 1.0;
    }
    let joined_spoken = spoken.join(" ");
    let joined_name = name.join(" ");
    // Spoken as one word, written as two ("powerpoint" / "power point"), or the other way.
    if joined_spoken.replace(' ', "") == joined_name.replace(' ', "") {
        return 0.97;
    }
    // A distinctive part of the name ("chrome" for "google chrome", "word" for "microsoft word").
    let covered = spoken.iter().all(|w| name.contains(w));
    if covered {
        #[allow(clippy::cast_precision_loss, reason = "word counts")]
        let share = spoken.len() as f32 / name.len() as f32;
        return 0.85 + 0.1 * share;
    }
    // Misheard spelling: edit distance over the whole name, or over one word of it for a single
    // spoken word ("crome" for "Google Chrome").
    let whole = ratio(&joined_spoken, &joined_name);
    let part = if spoken.len() == 1 && name.len() > 1 {
        name.iter()
            .map(|w| ratio(&spoken[0], w))
            .fold(0.0, f32::max)
            * 0.97
    } else {
        0.0
    };
    let best = whole.max(part);
    if best >= 0.8 { best * 0.95 } else { best * 0.5 }
}

/// 1 − normalized Levenshtein distance.
fn ratio(a: &str, b: &str) -> f32 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let longest = a.len().max(b.len());
    if longest == 0 {
        return 1.0;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    #[allow(clippy::cast_precision_loss, reason = "string lengths")]
    let r = 1.0 - prev[b.len()] as f32 / longest as f32;
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apps() -> Index {
        let e = |id: &str, name: &str, aliases: &[&str]| IndexEntry {
            id: id.into(),
            name: name.into(),
            aliases: aliases.iter().map(|a| (*a).to_owned()).collect(),
        };
        Index::new(vec![
            e("chrome", "Google Chrome", &[]),
            e("word", "Microsoft Word", &["Word"]),
            e("code", "Visual Studio Code", &["VS Code", "vscode"]),
            e("ppt", "PowerPoint", &[]),
            e("calc", "Calculator", &[]),
            e("spotify", "Spotify", &[]),
            e("chromium", "Chromium", &[]),
        ])
    }

    fn w(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn exact_names_aliases_and_distinctive_parts_match() {
        let index = apps();
        assert_eq!(index.find(&w("google chrome")).unwrap().id, "chrome");
        assert_eq!(index.find(&w("chrome")).unwrap().id, "chrome");
        assert_eq!(index.find(&w("the spotify")).unwrap().id, "spotify");
        assert_eq!(index.find(&w("vs code")).unwrap().id, "code");
        assert_eq!(index.find(&w("word")).unwrap().id, "word");
        assert_eq!(index.find(&w("power point")).unwrap().id, "ppt");
    }

    #[test]
    fn misheard_names_still_match_but_nonsense_does_not() {
        let index = apps();
        assert_eq!(index.find(&w("crome")).unwrap().id, "chrome");
        assert_eq!(index.find(&w("calculater")).unwrap().id, "calc");
        assert!(index.find(&w("this window")).is_none());
        assert!(index.find(&w("spotify and play jazz")).is_none());
        assert!(index.find(&w("the")).is_none());
    }
}
