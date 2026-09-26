//! The router's second stage (BRAINS §2, BRAIN-03): a paraphrase of a known command ("kill the
//! sound", "pull up Spotify") is matched by meaning, with a small local embedding model and a
//! k-nearest-neighbour search over command exemplars. It is accepted only above a high
//! similarity threshold **and** when the command's slots resolve: the chosen exemplar's
//! canonical command is re-read by the grammar with the app or window the user named, so slot
//! resolution is exactly the grammar's. Requests that join two actions ("open Chrome and search
//! for cats") are left to the brain, which gets the fast-path tools as tools (BRAIN-04).

use crate::grammar::{Context, Grammar, Match};
use crate::index::{Index, MIN_SCORE};
use crate::normalize;
use serde::Deserialize;

/// Turns text into a unit-length vector. The runtime provides one backed by the local model.
pub trait Embedder: Send + Sync {
    fn embed(&self, text: &str) -> Option<Vec<f32>>;
}

/// The acceptance threshold (cosine similarity), calibrated on the bundled model: paraphrases of
/// commands score 0.75–1.0, other requests below 0.55.
pub const THRESHOLD: f32 = 0.72;
/// How much closer the best command must be than any other command.
const MARGIN: f32 = 0.05;
/// Nearest neighbours that, when they all agree, lower the threshold by `CONSENSUS_BONUS`.
const K: usize = 3;
const CONSENSUS_BONUS: f32 = 0.06;
/// A lead over every other command this large counts like consensus.
const CLEAR_MARGIN: f32 = 0.15;

/// The words that stand in for a slot's value when embedding.
const APP: &str = "app";
const WINDOW: &str = "window";

#[derive(Deserialize)]
struct File {
    exemplar: Vec<ExemplarDef>,
}

#[derive(Deserialize)]
struct ExemplarDef {
    /// A phrase the grammar understands, with `{app}` / `{window}` for the slot.
    command: String,
    phrases: Vec<String>,
}

struct Exemplar {
    command: String,
    vector: Vec<f32>,
}

/// The exemplars bundled per language.
const BUNDLED: &[(&str, &str)] = &[("en", include_str!("../grammar/en/exemplars.toml"))];

pub struct Semantic {
    embedder: Box<dyn Embedder>,
    exemplars: Vec<Exemplar>,
    threshold: f32,
}

/// Where a slot's value was found in the request.
struct Slot {
    kind: &'static str,
    /// The words as the user said them.
    spoken: String,
    start: usize,
    len: usize,
}

impl Semantic {
    /// Embeds the language's exemplars (a few hundred milliseconds, once).
    pub fn new(language: &str, embedder: Box<dyn Embedder>) -> Option<Self> {
        let primary = language.split('-').next().unwrap_or(language);
        let (_, text) = BUNDLED
            .iter()
            .find(|(code, _)| code.eq_ignore_ascii_case(primary))?;
        let file: File = toml::from_str(text).ok()?;
        let mut exemplars = Vec::new();
        for def in file.exemplar {
            // The canonical phrase itself is an exemplar too.
            for phrase in std::iter::once(&def.command).chain(&def.phrases) {
                let text = phrase.replace("{app}", APP).replace("{window}", WINDOW);
                let vector = embedder.embed(&text)?;
                exemplars.push(Exemplar {
                    command: def.command.clone(),
                    vector,
                });
            }
        }
        Some(Self {
            embedder,
            exemplars,
            threshold: THRESHOLD,
        })
    }

    /// The best app or window named in the request, as a span of its words.
    fn find_slot(words: &[String], cx: &Context<'_>) -> Option<Slot> {
        let mut best: Option<(f32, Slot)> = None;
        for (kind, index) in [(APP, cx.apps), (WINDOW, cx.windows)] {
            let index: &Index = index;
            for len in (1..=4).rev() {
                for start in 0..words.len().saturating_sub(len - 1) {
                    let span = &words[start..start + len];
                    let Some(found) = index.find(span) else {
                        continue;
                    };
                    if found.score >= MIN_SCORE
                        && best.as_ref().is_none_or(|(s, _)| found.score > *s + 1e-6)
                    {
                        best = Some((
                            found.score,
                            Slot {
                                kind,
                                spoken: span.join(" "),
                                start,
                                len,
                            },
                        ));
                    }
                }
            }
        }
        best.map(|(_, s)| s)
    }

    /// The nearest exemplars' commands and similarities, best first (diagnostics, tuning).
    pub fn nearest(&self, text: &str, cx: &Context<'_>, n: usize) -> Vec<(f32, String)> {
        let words = normalize::words(text);
        let slot = Self::find_slot(&words, cx);
        let probe = match &slot {
            Some(s) => {
                let mut w = words.clone();
                w.splice(s.start..s.start + s.len, [s.kind.to_owned()]);
                w.join(" ")
            }
            None => words.join(" "),
        };
        let Some(vector) = self.embedder.embed(&probe) else {
            return Vec::new();
        };
        let mut scored: Vec<(f32, String)> = self
            .exemplars
            .iter()
            .map(|e| (similarity(&vector, &e.vector), e.command.clone()))
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        scored.truncate(n);
        scored
    }

    /// The command this request paraphrases, resolved by the grammar; `None` when nothing is
    /// close enough or its slots don't resolve.
    pub fn match_text(&self, text: &str, grammar: &Grammar, cx: &Context<'_>) -> Option<Match> {
        let words = normalize::words(text);
        if words.is_empty() || joins_actions(&words) {
            return None;
        }
        let slot = Self::find_slot(&words, cx);
        let probe = match &slot {
            Some(s) => {
                let mut w = words.clone();
                w.splice(s.start..s.start + s.len, [s.kind.to_owned()]);
                w.join(" ")
            }
            None => words.join(" "),
        };
        let vector = self.embedder.embed(&probe)?;
        let mut scored: Vec<(f32, &str)> = self
            .exemplars
            .iter()
            .map(|e| (similarity(&vector, &e.vector), e.command.as_str()))
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        let (best_score, command) = *scored.first()?;
        // kNN: when the nearest exemplars all name the same command, or no other command comes
        // close, a little less similarity is enough.
        let runner_up = scored
            .iter()
            .find(|(_, c)| *c != command)
            .map_or(0.0, |(s, _)| *s);
        let consensus = scored.len() >= K && scored.iter().take(K).all(|(_, c)| *c == command);
        let clear = best_score - runner_up >= CLEAR_MARGIN;
        let needed = if consensus || clear {
            self.threshold - CONSENSUS_BONUS
        } else {
            self.threshold
        };
        if best_score < needed {
            return None;
        }
        // The nearest exemplar of any *other* command must be further away: an ambiguous
        // request ("make it quieter, it's loud": mute or volume down?) goes to a brain.
        if best_score - runner_up < MARGIN {
            return None;
        }
        let needs = if command.contains("{app}") {
            Some(APP)
        } else if command.contains("{window}") {
            Some(WINDOW)
        } else {
            None
        };
        let canonical = match (needs, &slot) {
            (None, _) => command.to_owned(),
            (Some(kind), Some(s)) if s.kind == kind => command
                .replace("{app}", &s.spoken)
                .replace("{window}", &s.spoken),
            // The command needs an app or window the request doesn't name.
            _ => return None,
        };
        let mut matched = grammar.match_text(&canonical, cx)?;
        matched.confidence = matched.confidence.min(best_score);
        matched.pattern = format!("≈ {command}");
        Some(matched)
    }
}

/// "…and then…", "…and also…": two requests in one, for the brain.
fn joins_actions(words: &[String]) -> bool {
    words
        .windows(2)
        .any(|w| w[0] == "and" && !matches!(w[1].as_str(), "the" | "a" | "my"))
        || words.iter().any(|w| w == "then")
}

fn similarity(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::IndexEntry;

    /// A stand-in embedder: bag of words over a tiny vocabulary, enough to test the stage's
    /// logic (the real model is tested in the runtime).
    struct Bag;

    impl Embedder for Bag {
        fn embed(&self, text: &str) -> Option<Vec<f32>> {
            const WORDS: &[&[&str]] = &[
                &["mute", "silence", "quiet", "shh", "sound", "audio"],
                &["off", "kill", "stop"],
                &["open", "launch", "pull", "load", "start"],
                &["up"],
                &["volume", "louder"],
                &["app"],
                &["weather", "rain"],
                &["window"],
            ];
            let lower = text.to_lowercase();
            let mut v: Vec<f32> = WORDS
                .iter()
                .map(|group| {
                    #[allow(clippy::cast_precision_loss, reason = "a word count")]
                    let n = lower
                        .split_whitespace()
                        .filter(|w| group.contains(w))
                        .count() as f32;
                    n
                })
                .collect();
            let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm == 0.0 {
                v[0] = 0.0;
                return Some(v);
            }
            for x in &mut v {
                *x /= norm;
            }
            Some(v)
        }
    }

    #[test]
    fn paraphrases_resolve_through_the_grammar_and_the_rest_goes_on() {
        let grammar = Grammar::bundled("en").unwrap();
        let semantic = Semantic::new("en", Box::new(Bag)).expect("exemplars load");
        let apps = Index::new(vec![IndexEntry {
            id: "Spotify".into(),
            name: "Spotify".into(),
            aliases: Vec::new(),
        }]);
        let windows = Index::default();
        let cx = Context {
            apps: &apps,
            windows: &windows,
        };
        let m = semantic
            .match_text("kill the sound", &grammar, &cx)
            .expect("a paraphrase of mute");
        assert_eq!(m.tool, "audio.mute");
        let m = semantic
            .match_text("pull up spotify", &grammar, &cx)
            .expect("a paraphrase of open");
        assert_eq!(m.tool, "apps.launch");
        assert_eq!(m.args["app"]["id"], "Spotify");
        // No app named that it knows: the slot doesn't resolve, so no match.
        assert!(
            semantic
                .match_text("pull up zorblax", &grammar, &cx)
                .is_none()
        );
        // Two actions in one: for the brain (BRAIN-04).
        assert!(
            semantic
                .match_text("open spotify and search for cats", &grammar, &cx)
                .is_none()
        );
        assert!(semantic.match_text("will it rain", &grammar, &cx).is_none());
    }
}
