//! English text → Kokoro phonemes, without espeak (DECISIONS "Kokoro phonemizer"). A port of the
//! lexicon half of misaki (Apache-2.0): its US gold and silver dictionaries, stress handling and
//! the -s / -ed / -ing rules, plus English number words. Words no dictionary knows are spelled if
//! they are short capitals (acronyms) and otherwise go through letter-to-sound rules
//! (`rules.rs`); misaki's own fallback is a neural model this doesn't need. There is no
//! part-of-speech tagger, so words like "read" take their default pronunciation.

use super::{numbers, rules};
use crate::error::{VoiceError, VoiceResult};
use std::collections::HashMap;
use std::path::Path;

const PRIMARY: char = 'ˈ';
const SECONDARY: char = 'ˌ';
const VOWELS: &str = "AIOQWYaiuæɑɒɔəɛɜɪʊʌᵻ";
const CONSONANTS: &str = "bdfhjklmnpstvwzðŋɡɹɾʃʒʤʧθ";
/// Vowels before a /t/ that turns into a flap in US English ("better").
const US_TAUS: &str = "AIOWYiuæɑəɛɪɹʊʌ";
const PUNCTUATION: &str = ";:,.!?—…\"“”()";
/// Symbols read as words.
const SYMBOLS: [(char, &str); 4] = [('%', "percent"), ('&', "and"), ('+', "plus"), ('@', "at")];

pub fn is_vowel(c: char) -> bool {
    VOWELS.contains(c)
}

/// A dictionary entry: one pronunciation, or one per part of speech.
#[derive(Clone, Debug)]
enum Entry {
    Plain(String),
    /// `DEFAULT`, and `None` (used at the end of a phrase) when the dictionary has it.
    Tagged {
        default: Option<String>,
        end: Option<String>,
    },
}

pub struct Lexicon {
    gold: HashMap<String, Entry>,
    silver: HashMap<String, Entry>,
}

/// What comes after a word, for "the", "to" and "a" (misaki's `TokenContext`).
#[derive(Clone, Copy, Debug, Default)]
struct Ahead {
    /// Whether the next sound is a vowel; `None` at the end of a phrase.
    vowel: Option<bool>,
}

impl Lexicon {
    /// Loads `us_gold.json` and `us_silver.json` from `dir`.
    pub fn load(dir: &Path) -> VoiceResult<Self> {
        let read = |name: &str| -> VoiceResult<HashMap<String, Entry>> {
            let text = std::fs::read_to_string(dir.join(name))
                .map_err(|e| VoiceError::Engine(format!("{name}: {e}")))?;
            parse(&text).map_err(|e| VoiceError::Engine(format!("{name}: {e}")))
        };
        Ok(Self {
            gold: grow(read("us_gold.json")?),
            silver: grow(read("us_silver.json")?),
        })
    }

    /// A lexicon from JSON text (tests).
    pub fn from_json(gold: &str, silver: &str) -> Result<Self, serde_json::Error> {
        Ok(Self {
            gold: grow(parse(gold)?),
            silver: grow(parse(silver)?),
        })
    }

    fn plain(&self, word: &str) -> Option<String> {
        match self.gold.get(word) {
            Some(Entry::Plain(p)) => Some(p.clone()),
            Some(Entry::Tagged { default, .. }) => default.clone(),
            None => None,
        }
    }

    /// Letters said one by one ("USB"), as misaki's `get_NNP`.
    fn spell(&self, word: &str) -> Option<String> {
        let mut ps = String::new();
        for c in word.chars().filter(|c| c.is_alphabetic()) {
            ps.push_str(&self.plain(&c.to_uppercase().to_string())?);
        }
        let ps = apply_stress(&ps, Some(0.0));
        Some(match ps.rsplit_once(SECONDARY) {
            Some((a, b)) => format!("{a}{PRIMARY}{b}"),
            None => ps,
        })
    }

    fn is_known(&self, word: &str) -> bool {
        if self.gold.contains_key(word) || self.silver.contains_key(word) {
            return true;
        }
        if !word
            .chars()
            .all(|c| c.is_ascii_alphabetic() || c == '\'' || c == '-')
        {
            return false;
        }
        if word.chars().count() == 1 {
            return true;
        }
        if word == word.to_uppercase() && self.gold.contains_key(&word.to_lowercase()) {
            return true;
        }
        let rest: String = word.chars().skip(1).collect();
        rest == rest.to_uppercase()
    }

    fn lookup(&self, word: &str, stress: Option<f32>, ahead: Ahead) -> Option<String> {
        let mut word = word.to_owned();
        if word == word.to_uppercase() && !self.gold.contains_key(&word) {
            word = word.to_lowercase();
        }
        let entry = self.gold.get(&word).or_else(|| self.silver.get(&word));
        let ps = match entry {
            Some(Entry::Plain(p)) => Some(p.clone()),
            Some(Entry::Tagged { default, end }) => {
                if ahead.vowel.is_none() && end.is_some() {
                    end.clone()
                } else {
                    default.clone()
                }
            }
            None => None,
        };
        match ps {
            Some(ps) => Some(apply_stress(&ps, stress)),
            None => self.spell(&word),
        }
    }

    fn stem_s(&self, word: &str, stress: Option<f32>, ahead: Ahead) -> Option<String> {
        if word.len() < 3 || !word.ends_with('s') {
            return None;
        }
        let stem = if !word.ends_with("ss") && self.is_known(&word[..word.len() - 1]) {
            word[..word.len() - 1].to_owned()
        } else if (word.ends_with("'s")
            || (word.len() > 4 && word.ends_with("es") && !word.ends_with("ies")))
            && self.is_known(&word[..word.len() - 2])
        {
            word[..word.len() - 2].to_owned()
        } else if word.len() > 4
            && word.ends_with("ies")
            && self.is_known(&format!("{}y", &word[..word.len() - 3]))
        {
            format!("{}y", &word[..word.len() - 3])
        } else {
            return None;
        };
        self.lookup(&stem, stress, ahead).map(|s| add_s(&s))
    }

    fn stem_ed(&self, word: &str, stress: Option<f32>, ahead: Ahead) -> Option<String> {
        if word.len() < 4 || !word.ends_with('d') {
            return None;
        }
        let stem = if !word.ends_with("dd") && self.is_known(&word[..word.len() - 1]) {
            &word[..word.len() - 1]
        } else if word.len() > 4
            && word.ends_with("ed")
            && !word.ends_with("eed")
            && self.is_known(&word[..word.len() - 2])
        {
            &word[..word.len() - 2]
        } else {
            return None;
        };
        self.lookup(stem, stress, ahead).map(|s| add_ed(&s))
    }

    fn stem_ing(&self, word: &str, stress: Option<f32>, ahead: Ahead) -> Option<String> {
        if word.len() < 5 || !word.ends_with("ing") {
            return None;
        }
        let base = &word[..word.len() - 3];
        let doubled = {
            let b = base.as_bytes();
            b.len() >= 2
                && ((b[b.len() - 1] == b[b.len() - 2]
                    && b"bcdgklmnprstvxz".contains(&b[b.len() - 1]))
                    || base.ends_with("ck"))
        };
        let stem = if word.len() > 5 && self.is_known(base) {
            base.to_owned()
        } else if self.is_known(&format!("{base}e")) {
            format!("{base}e")
        } else if word.len() > 5 && doubled && self.is_known(&base[..base.len() - 1]) {
            base[..base.len() - 1].to_owned()
        } else {
            return None;
        };
        self.lookup(&stem, stress, ahead).map(|s| add_ing(&s))
    }

    /// Words with rules of their own (misaki's `get_special_case`, without part-of-speech tags).
    fn special(&self, word: &str, ahead: Ahead) -> Option<String> {
        Some(match word {
            "a" | "A" => "ɐ".into(),
            "an" | "An" | "AN" => "ɐn".into(),
            "I" => format!("{SECONDARY}I"),
            "to" | "To" | "TO" => match ahead.vowel {
                None => self.plain("to")?,
                Some(false) => "tə".into(),
                Some(true) => "tʊ".into(),
            },
            "the" | "The" | "THE" => {
                if ahead.vowel == Some(true) {
                    "ði".into()
                } else {
                    "ðə".into()
                }
            }
            "in" | "In" | "IN" => {
                if ahead.vowel.is_none() {
                    format!("{PRIMARY}ɪn")
                } else {
                    "ɪn".into()
                }
            }
            "am" | "Am" | "AM" if ahead.vowel.is_some() && word == "am" => "ɐm".into(),
            _ => return None,
        })
    }

    fn word(&self, word: &str, ahead: Ahead) -> Option<String> {
        if let Some(ps) = self.special(word, ahead) {
            return Some(ps);
        }
        // Capitalization raises stress a little (misaki's `cap_stresses`).
        let stress = if word == word.to_lowercase() {
            None
        } else if word == word.to_uppercase() {
            Some(2.0)
        } else {
            Some(0.5)
        };
        let lower = word.to_lowercase();
        let mut word = word.to_owned();
        if word.chars().count() > 1
            && word != lower
            && !self.gold.contains_key(&word)
            && !self.silver.contains_key(&word)
            && (self.gold.contains_key(&lower) || self.silver.contains_key(&lower))
        {
            word = lower;
        }
        if self.is_known(&word) {
            return self.lookup(&word, stress, ahead);
        }
        if let Some(stem) = word.strip_suffix('\'')
            && self.is_known(stem)
        {
            return self.lookup(stem, stress, ahead);
        }
        self.stem_s(&word, stress, ahead)
            .or_else(|| self.stem_ed(&word, stress, ahead))
            .or_else(|| self.stem_ing(&word, stress.or(Some(0.5)), ahead))
    }

    /// Phonemes for `text`, one phrase or sentence.
    pub fn phonemize(&self, text: &str) -> String {
        let tokens = tokenize(text);
        let mut out: Vec<String> = vec![String::new(); tokens.len()];
        let mut ahead = Ahead::default();
        for (i, token) in tokens.iter().enumerate().rev() {
            let ps = match token {
                Token::Punct(p) => p.to_string(),
                Token::Word(w) => self.phonemes_of(w, ahead),
            };
            ahead = next_context(ahead, &ps);
            out[i] = ps;
        }
        let mut result = String::new();
        for (token, ps) in tokens.iter().zip(out) {
            if ps.is_empty() {
                continue;
            }
            let glued = matches!(token, Token::Punct(p) if !"(“\"".contains(*p));
            if !result.is_empty() && !glued {
                result.push(' ');
            }
            result.push_str(&ps);
        }
        // Kokoro 1.0 reads the flap and glottal stop as T and t (misaki before version 2.0).
        result.replace('ɾ', "T").replace('ʔ', "t")
    }

    fn phonemes_of(&self, word: &str, ahead: Ahead) -> String {
        if let Some(ps) = self.word(word, ahead) {
            return ps;
        }
        if word.chars().any(|c| c.is_ascii_digit())
            && let Some(words) = numbers::words(word)
        {
            return words
                .iter()
                .flat_map(|w| w.split('-'))
                .filter_map(|w| {
                    let stress = (w == "point").then_some(-2.0);
                    self.lookup(w, stress, Ahead::default())
                })
                .collect::<Vec<_>>()
                .join(" ");
        }
        if let Some((_, name)) = SYMBOLS.iter().find(|(c, _)| word == c.to_string()) {
            return self.lookup(name, None, ahead).unwrap_or_default();
        }
        // A possessive of an unknown name: the name, then the -s sound ("Spotify's").
        if let Some(stem) = word
            .strip_suffix("'s")
            .or_else(|| word.strip_suffix("s'"))
            .filter(|s| !s.is_empty())
        {
            return add_s(&self.phonemes_of(stem, ahead));
        }
        // Short capitals are said letter by letter ("GPU"); anything else by rule.
        let letters: String = word.chars().filter(char::is_ascii_alphabetic).collect();
        if letters.is_empty() {
            return String::new();
        }
        if letters.len() <= 4
            && letters == letters.to_uppercase()
            && let Some(ps) = self.spell(&letters)
        {
            return ps;
        }
        rules::phonemes(&letters)
    }
}

fn parse(text: &str) -> Result<HashMap<String, Entry>, serde_json::Error> {
    let raw: HashMap<String, serde_json::Value> = serde_json::from_str(text)?;
    Ok(raw
        .into_iter()
        .filter_map(|(k, v)| {
            let entry = match v {
                serde_json::Value::String(s) => Entry::Plain(s),
                serde_json::Value::Object(map) => Entry::Tagged {
                    default: map
                        .get("DEFAULT")
                        .and_then(|v| v.as_str())
                        .map(str::to_owned),
                    end: map.get("None").and_then(|v| v.as_str()).map(str::to_owned),
                },
                _ => return None,
            };
            Some((k, entry))
        })
        .collect())
}

/// Adds the capitalized and lower-case forms misaki adds (`grow_dictionary`).
fn grow(d: HashMap<String, Entry>) -> HashMap<String, Entry> {
    let mut extra = HashMap::new();
    for (k, v) in &d {
        if k.chars().count() < 2 {
            continue;
        }
        let lower = k.to_lowercase();
        let capital = capitalize(&lower);
        if *k == lower && *k != capital {
            extra.insert(capital, v.clone());
        } else if *k == capital {
            extra.insert(lower, v.clone());
        }
    }
    extra.extend(d);
    extra
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

/// misaki's `apply_stress`.
fn apply_stress(ps: &str, stress: Option<f32>) -> String {
    let Some(stress) = stress else {
        return ps.to_owned();
    };
    let has = |c: char| ps.contains(c);
    if stress < -1.0 {
        return ps.replace([PRIMARY, SECONDARY], "");
    }
    if stress == -1.0 || ((stress == 0.0 || stress == -0.5) && has(PRIMARY)) {
        return ps
            .replace(SECONDARY, "")
            .replace(PRIMARY, &SECONDARY.to_string());
    }
    if (stress == 0.0 || stress == 0.5 || stress == 1.0) && !has(PRIMARY) && !has(SECONDARY) {
        if !ps.chars().any(is_vowel) {
            return ps.to_owned();
        }
        return restress(&format!("{SECONDARY}{ps}"));
    }
    if stress >= 1.0 && !has(PRIMARY) && has(SECONDARY) {
        return ps.replace(SECONDARY, &PRIMARY.to_string());
    }
    if stress > 1.0 && !has(PRIMARY) && !has(SECONDARY) {
        if !ps.chars().any(is_vowel) {
            return ps.to_owned();
        }
        return restress(&format!("{PRIMARY}{ps}"));
    }
    ps.to_owned()
}

/// Moves each stress mark to just before the next vowel.
fn restress(ps: &str) -> String {
    let chars: Vec<char> = ps.chars().collect();
    let mut placed: Vec<(f32, char)> = Vec::with_capacity(chars.len());
    for (i, &c) in chars.iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "phoneme positions")]
        let mut at = i as f32;
        if (c == PRIMARY || c == SECONDARY)
            && let Some(j) = chars[i..].iter().position(|&v| is_vowel(v))
        {
            #[allow(clippy::cast_precision_loss, reason = "phoneme positions")]
            {
                at = (i + j) as f32 - 0.5;
            }
        }
        placed.push((at, c));
    }
    placed.sort_by(|a, b| a.0.total_cmp(&b.0));
    placed.into_iter().map(|(_, c)| c).collect()
}

fn add_s(stem: &str) -> String {
    match stem.chars().last() {
        Some(c) if "ptkfθ".contains(c) => format!("{stem}s"),
        Some(c) if "szʃʒʧʤ".contains(c) => format!("{stem}ᵻz"),
        _ => format!("{stem}z"),
    }
}

fn add_ed(stem: &str) -> String {
    let chars: Vec<char> = stem.chars().collect();
    match chars.last() {
        Some(c) if "pkfθʃsʧ".contains(*c) => format!("{stem}t"),
        Some('d') => format!("{stem}ᵻd"),
        Some(c) if *c != 't' => format!("{stem}d"),
        _ if chars.len() < 2 => format!("{stem}ɪd"),
        _ if US_TAUS.contains(chars[chars.len() - 2]) => {
            format!("{}ɾᵻd", chars[..chars.len() - 1].iter().collect::<String>())
        }
        _ => format!("{stem}ᵻd"),
    }
}

fn add_ing(stem: &str) -> String {
    let chars: Vec<char> = stem.chars().collect();
    if chars.len() > 1 && chars[chars.len() - 1] == 't' && US_TAUS.contains(chars[chars.len() - 2])
    {
        format!("{}ɾɪŋ", chars[..chars.len() - 1].iter().collect::<String>())
    } else {
        format!("{stem}ɪŋ")
    }
}

/// Whether the phonemes start with a vowel (for the word before them).
fn next_context(ahead: Ahead, ps: &str) -> Ahead {
    for c in ps.chars() {
        if ";:,.!?—…".contains(c) {
            return Ahead { vowel: None };
        }
        if is_vowel(c) {
            return Ahead { vowel: Some(true) };
        }
        if CONSONANTS.contains(c) {
            return Ahead { vowel: Some(false) };
        }
    }
    ahead
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Word(String),
    Punct(char),
}

/// Words (letters, digits, inner apostrophes and dots), symbols and punctuation. Hyphens and
/// slashes split words; other characters are dropped.
fn tokenize(text: &str) -> Vec<Token> {
    let text = text.replace(['‘', '’'], "'").replace('–', "—");
    let mut out = Vec::new();
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut Vec<Token>| {
        let w = word.trim_matches(|c| c == '\'' || c == '.');
        if !w.is_empty() {
            out.push(Token::Word(w.to_owned()));
        }
        word.clear();
    };
    let chars: Vec<char> = text.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        let next = chars.get(i + 1).copied();
        let inner = !word.is_empty() && next.is_some_and(char::is_alphanumeric);
        if c.is_alphanumeric()
            || ((c == '\'' || c == '.' || c == ',')
                && inner
                && (c != ',' || word.ends_with(|d: char| d.is_ascii_digit())))
        {
            word.push(c);
        } else if SYMBOLS.iter().any(|(s, _)| *s == c) {
            flush(&mut word, &mut out);
            out.push(Token::Word(c.to_string()));
        } else {
            flush(&mut word, &mut out);
            if PUNCTUATION.contains(c) {
                out.push(Token::Punct(c));
            }
        }
    }
    flush(&mut word, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lexicon() -> Lexicon {
        let gold = r#"{
            "hello": "həlˈO", "world": "wˈɜɹld", "the": "ði", "to": "tʊ", "open": "ˈOpᵊn",
            "chrome": "kɹˈOm", "google": "ɡˈuɡᵊl", "play": "plˈA", "volume": "vˈɑljˌum",
            "read": {"DEFAULT": "ɹˈid", "VBD": "ɹˈɛd"}, "one": "wˈʌn", "twenty": "twˈɛnti",
            "six": "sˈɪks", "percent": "pɜɹsˈɛnt", "fifty": "fˈɪfti", "want": "wˈɑnt",
            "U": "jˈu", "S": "ˈɛs", "B": "bˈi", "apple": "ˈæpᵊl", "box": "bˈɑks", "stop": "stˈɑp",
            "get": "ɡˈɛt", "on": "ˈɔn"
        }"#;
        Lexicon::from_json(gold, r#"{"playing": "plˈAɪŋ"}"#).unwrap()
    }

    #[test]
    fn dictionary_words_and_punctuation() {
        let l = lexicon();
        assert_eq!(l.phonemize("Hello, world!"), "həlˈO, wˈɜɹld!");
        assert_eq!(l.phonemize("Open Chrome."), "ˈOpᵊn kɹˈOm.");
    }

    #[test]
    fn the_and_to_follow_the_next_sound() {
        let l = lexicon();
        assert!(l.phonemize("the apple").starts_with("ði "));
        assert!(l.phonemize("the box").starts_with("ðə "));
        assert!(l.phonemize("want to stop").contains(" tə "));
        assert!(l.phonemize("want to open").contains(" tʊ "));
    }

    #[test]
    fn plurals_past_tenses_and_ing_come_from_the_stem() {
        let l = lexicon();
        assert_eq!(l.phonemize("boxes"), "bˈɑksᵻz");
        assert_eq!(l.phonemize("plays"), "plˈAz");
        assert_eq!(l.phonemize("played"), "plˈAd");
        assert_eq!(l.phonemize("playing"), "plˈAɪŋ", "silver first");
        assert_eq!(l.phonemize("stopping"), "stˈɑpɪŋ");
        assert_eq!(l.phonemize("getting"), "ɡˈɛTɪŋ", "the US flap");
    }

    #[test]
    fn numbers_symbols_and_acronyms() {
        let l = lexicon();
        assert_eq!(l.phonemize("26"), "twˈɛnti sˈɪks");
        assert_eq!(l.phonemize("50%"), "fˈɪfti pɜɹsˈɛnt");
        assert_eq!(l.phonemize("USB"), "jˌuˌɛsbˈi");
    }

    #[test]
    fn unknown_words_are_pronounced_by_rule() {
        let l = lexicon();
        let ps = l.phonemize("Kivo");
        assert!(ps.contains('ˈ') && ps.contains('k'), "{ps}");
        let possessive = l.phonemize("Kivo's");
        assert_eq!(possessive, format!("{ps}z"), "the name, then /z/");
    }

    #[test]
    fn stress_moves_to_the_vowel() {
        assert_eq!(apply_stress("həlO", Some(2.0)), "hˈəlO");
        assert_eq!(apply_stress("ˈOpᵊn", Some(-1.0)), "ˌOpᵊn");
        assert_eq!(apply_stress("ˈOpᵊn", Some(-2.0)), "Opᵊn");
    }
}
