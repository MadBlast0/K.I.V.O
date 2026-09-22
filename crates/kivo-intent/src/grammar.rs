//! The grammar stage (BRAINS §2, BRAIN-01): slot-based command patterns from per-language data
//! files, matched word by word in microseconds, with slots resolved against the live app and window
//! indexes. A match is only returned when every slot resolves; otherwise the request moves on.

use crate::index::{Index, Resolved};
use crate::normalize;
use serde::Deserialize;
use serde_json::{Map, Value, json};

/// A matched command: the tool to run and its arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct Match {
    pub tool: String,
    pub args: Map<String, Value>,
    /// 0–1: the weakest slot's score (1 for commands without resolved slots).
    pub confidence: f32,
    /// The pattern that matched, for the Activity log and misroute reports.
    pub pattern: String,
}

/// What the grammar resolves slots against.
pub struct Context<'a> {
    pub apps: &'a Index,
    pub windows: &'a Index,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    App,
    Window,
    Number,
    Url,
    Text,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Word(String),
    Slot {
        name: String,
        kind: Kind,
        optional: bool,
    },
}

struct Pattern {
    source: String,
    tokens: Vec<Token>,
}

struct Command {
    tool: String,
    patterns: Vec<Pattern>,
}

pub struct Grammar {
    language: String,
    leading: Vec<Vec<String>>,
    trailing: Vec<Vec<String>>,
    commands: Vec<Command>,
}

#[derive(Debug, thiserror::Error)]
pub enum GrammarError {
    #[error("the grammar file is invalid: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("unknown slot kind in `{0}`")]
    Slot(String),
    #[error("no grammar for language {0}")]
    Language(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    fillers: Fillers,
    command: Vec<CommandFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fillers {
    leading: Vec<String>,
    trailing: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandFile {
    tool: String,
    patterns: Vec<String>,
}

/// The grammars KIVO ships, by language (VOICE §9: `grammar/<lang>/*.toml`).
const BUNDLED: &[(&str, &str)] = &[("en", include_str!("../grammar/en/commands.toml"))];

impl Grammar {
    /// The bundled grammar for `language` (a BCP-47 tag).
    pub fn bundled(language: &str) -> Result<Self, GrammarError> {
        let primary = language.split('-').next().unwrap_or(language);
        let (code, text) = BUNDLED
            .iter()
            .find(|(code, _)| code.eq_ignore_ascii_case(primary))
            .ok_or_else(|| GrammarError::Language(language.into()))?;
        Self::parse(code, text)
    }

    pub fn parse(language: &str, text: &str) -> Result<Self, GrammarError> {
        let file: File = toml::from_str(text)?;
        let split = |phrases: Vec<String>| phrases.iter().map(|p| normalize::words(p)).collect();
        let mut commands = Vec::new();
        for c in file.command {
            let patterns = c
                .patterns
                .into_iter()
                .map(|p| compile(&p).map(|tokens| Pattern { source: p, tokens }))
                .collect::<Result<_, _>>()?;
            commands.push(Command {
                tool: c.tool,
                patterns,
            });
        }
        Ok(Self {
            language: language.into(),
            leading: split(file.fillers.leading),
            trailing: split(file.fillers.trailing),
            commands,
        })
    }

    pub fn language(&self) -> &str {
        &self.language
    }

    /// Every tool the grammar can produce.
    pub fn tools(&self) -> impl Iterator<Item = &str> {
        self.commands.iter().map(|c| c.tool.as_str())
    }

    /// Every fixed phrase (patterns without slots), for wake-word collision checks (VOICE-18).
    pub fn phrases(&self) -> Vec<String> {
        self.commands
            .iter()
            .flat_map(|c| &c.patterns)
            .filter(|p| p.tokens.iter().all(|t| matches!(t, Token::Word(_))))
            .map(|p| p.source.clone())
            .collect()
    }

    /// The best command for `text`, if one matches with every slot resolved.
    pub fn match_text(&self, text: &str, cx: &Context<'_>) -> Option<Match> {
        let words = normalize::strip_fillers(normalize::words(text), &self.leading, &self.trailing);
        if words.is_empty() {
            return None;
        }
        let mut best: Option<Match> = None;
        for command in &self.commands {
            for pattern in &command.patterns {
                let Some((args, confidence)) = match_tokens(&pattern.tokens, &words, cx) else {
                    continue;
                };
                // Earlier commands win ties; a later one must be strictly more confident.
                if best
                    .as_ref()
                    .is_none_or(|b| confidence > b.confidence + 1e-6)
                {
                    best = Some(Match {
                        tool: command.tool.clone(),
                        args,
                        confidence,
                        pattern: pattern.source.clone(),
                    });
                }
            }
        }
        best
    }
}

fn compile(pattern: &str) -> Result<Vec<Token>, GrammarError> {
    pattern
        .split_whitespace()
        .map(|part| {
            let Some(inner) = part.strip_prefix('{').and_then(|p| p.strip_suffix('}')) else {
                // Normalized like the words it's matched against ("what's" → "whats").
                return Ok(Token::Word(part.to_lowercase().replace(['\'', '’'], "")));
            };
            let (name, optional) = inner
                .strip_suffix('?')
                .map_or((inner, false), |n| (n, true));
            let kind = match name {
                "app" => Kind::App,
                "window" => Kind::Window,
                "number" => Kind::Number,
                "url" => Kind::Url,
                "text" => Kind::Text,
                _ => return Err(GrammarError::Slot(pattern.into())),
            };
            Ok(Token::Slot {
                name: name.into(),
                kind,
                optional,
            })
        })
        .collect()
}

/// Matches `tokens` against all of `words`, trying each way to split the words among the slots.
fn match_tokens(
    tokens: &[Token],
    words: &[String],
    cx: &Context<'_>,
) -> Option<(Map<String, Value>, f32)> {
    match tokens.split_first() {
        None => words.is_empty().then(|| (Map::new(), 1.0)),
        Some((Token::Word(w), rest)) => (words.first() == Some(w))
            .then(|| match_tokens(rest, &words[1..], cx))
            .flatten(),
        Some((
            Token::Slot {
                name,
                kind,
                optional,
            },
            rest,
        )) => {
            if *optional && let Some(found) = match_tokens(rest, words, cx) {
                return Some(found);
            }
            // A slot takes at least one word; try the longest capture first.
            for take in (1..=words.len()).rev() {
                let Some((value, score)) = resolve(*kind, &words[..take], cx) else {
                    continue;
                };
                if let Some((mut args, rest_score)) = match_tokens(rest, &words[take..], cx) {
                    args.insert(name.clone(), value);
                    return Some((args, score.min(rest_score)));
                }
            }
            None
        }
    }
}

fn resolve(kind: Kind, words: &[String], cx: &Context<'_>) -> Option<(Value, f32)> {
    let found = |r: Resolved| (json!({ "id": r.id, "name": r.name }), r.score);
    match kind {
        Kind::App => cx.apps.find(words).map(found),
        Kind::Window => cx.windows.find(words).map(found),
        Kind::Number => normalize::number(words).map(|n| (json!(n), 1.0)),
        Kind::Url => normalize::url(words).map(|u| (json!(u), 1.0)),
        Kind::Text => Some((json!(words.join(" ")), 0.9)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::IndexEntry;

    fn index(names: &[(&str, &str)]) -> Index {
        Index::new(
            names
                .iter()
                .map(|(id, name)| IndexEntry {
                    id: (*id).into(),
                    name: (*name).into(),
                    aliases: Vec::new(),
                })
                .collect(),
        )
    }

    fn run(text: &str) -> Option<Match> {
        let grammar = Grammar::bundled("en").unwrap();
        let apps = index(&[
            ("chrome", "Google Chrome"),
            ("spotify", "Spotify"),
            ("notepad", "Notepad"),
        ]);
        let windows = index(&[("100", "Inbox - Outlook"), ("200", "Spotify Premium")]);
        grammar.match_text(
            text,
            &Context {
                apps: &apps,
                windows: &windows,
            },
        )
    }

    #[test]
    fn the_m1_journeys_match() {
        let open = run("Open Chrome.").unwrap();
        assert_eq!(open.tool, "apps.launch");
        assert_eq!(open.args["app"]["id"], "chrome");
        assert_eq!(run("Mute").unwrap().tool, "audio.mute");
        assert_eq!(run("Take a screenshot.").unwrap().tool, "screen.screenshot");
        assert_eq!(
            run("Screenshot this window").unwrap().tool,
            "screen.screenshot_window"
        );
        assert_eq!(run("Restart Chrome").unwrap().tool, "apps.restart");
        assert_eq!(run("Restart").unwrap().tool, "system.restart");
        assert_eq!(run("whats playing").unwrap().tool, "media.now_playing");
        assert_eq!(
            run("Hey Kivo, can you open Spotify for me please?")
                .unwrap()
                .args["app"]["id"],
            "spotify"
        );
    }

    #[test]
    fn slots_parse_numbers_urls_and_text() {
        let volume = run("Set the volume to 30%").unwrap();
        assert_eq!(
            (volume.tool.as_str(), &volume.args["number"]),
            ("audio.volume_set", &json!(30))
        );
        let site = run("Go to YouTube.com").unwrap();
        assert_eq!(
            (site.tool.as_str(), &site.args["url"]),
            ("browser.open_url", &json!("https://youtube.com"))
        );
        let search = run("Search for rust tutorials").unwrap();
        assert_eq!(search.args["text"], "rust tutorials");
    }

    #[test]
    fn app_names_beat_web_addresses_only_when_they_are_not_addresses() {
        assert_eq!(
            run("open youtube dot com").unwrap().tool,
            "browser.open_url"
        );
        assert_eq!(run("open notepad").unwrap().tool, "apps.launch");
    }

    #[test]
    fn literal_window_commands_beat_weak_app_guesses() {
        assert_eq!(run("close this window").unwrap().tool, "windows.close");
        assert_eq!(run("close spotify").unwrap().tool, "apps.close");
        assert_eq!(run("minimize").unwrap().tool, "windows.minimize");
        let focus = run("switch to outlook").unwrap();
        assert_eq!(
            (focus.tool.as_str(), &focus.args["window"]["id"]),
            ("windows.focus", &json!("100"))
        );
    }

    #[test]
    fn unresolvable_or_compound_requests_go_to_the_next_stage() {
        assert!(run("open photoshop").is_none(), "not installed");
        assert!(
            run("open chrome and search for cats").is_none(),
            "hybrid → brain"
        );
        assert!(run("what's the weather like").is_none());
        assert!(run("").is_none());
    }

    #[test]
    fn destructive_commands_are_recognized_like_any_other() {
        // They still go through the permission engine (BRAIN-02): the grammar only names the tool.
        assert_eq!(
            run("shut down the computer").unwrap().tool,
            "system.shutdown"
        );
        assert_eq!(run("restart").unwrap().tool, "system.restart");
    }

    #[test]
    fn fixed_phrases_are_listed_for_collision_checks() {
        let grammar = Grammar::bundled("en").unwrap();
        let phrases = grammar.phrases();
        assert!(phrases.contains(&"mute".to_owned()));
        assert!(!phrases.iter().any(|p| p.contains('{')));
        assert!(Grammar::bundled("xx").is_err());
    }
}
