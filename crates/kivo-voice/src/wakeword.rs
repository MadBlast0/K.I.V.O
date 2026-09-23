//! Checking a wake phrase before it is used (VOICE §4, VOICE-16/18). A good wake word is long
//! enough and unusual enough that ordinary speech rarely contains it: the research's guidance is
//! reject one syllable, warn at two, prefer three or four, at least six phonemes with varied
//! vowels, not a common word, and not confusable with KIVO's other wake words or commands.
//! Phonemes come from the English letter-to-sound rules, so the check needs no download.

use crate::kokoro::g2p::is_vowel;
use crate::kokoro::rules;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Quality {
    Good,
    Fair,
    Risky,
}

/// Why a phrase isn't Good (keys under `wake.check.*` in the text catalog).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "reason")]
pub enum Concern {
    Empty,
    OneSyllable,
    TwoSyllables,
    FewSounds {
        phonemes: usize,
    },
    SameVowels,
    CommonWords,
    /// Sounds like another wake word.
    LikeWakeWord {
        phrase: String,
    },
    /// Sounds like a KIVO command, which would start requests by itself.
    LikeCommand {
        phrase: String,
    },
    /// The keyword model has no way to spell a sound in it.
    Unspellable,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Assessment {
    pub quality: Quality,
    pub syllables: usize,
    pub phonemes: usize,
    pub concerns: Vec<Concern>,
}

/// Very frequent English words: a phrase made only of these turns up in ordinary talk.
const COMMON: &[&str] = &[
    "a", "about", "after", "again", "all", "also", "an", "and", "any", "are", "as", "at", "back",
    "be", "because", "been", "but", "by", "can", "come", "could", "day", "do", "even", "first",
    "for", "from", "get", "give", "go", "good", "have", "he", "hello", "her", "here", "hey", "hi",
    "him", "his", "how", "i", "if", "in", "into", "is", "it", "its", "just", "know", "like",
    "look", "make", "me", "most", "my", "new", "no", "not", "now", "of", "okay", "ok", "on", "one",
    "only", "or", "other", "our", "out", "over", "people", "please", "say", "see", "she", "so",
    "some", "stop", "take", "than", "thank", "thanks", "that", "the", "their", "them", "then",
    "there", "these", "they", "think", "this", "time", "to", "two", "up", "us", "use", "want",
    "way", "we", "well", "what", "when", "which", "who", "will", "with", "work", "would", "yeah",
    "year", "yes", "you", "your",
];

/// The phonemes of a phrase (stress marks dropped).
pub fn phonemes(phrase: &str) -> Vec<char> {
    phrase
        .split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphabetic() || *c == '\'')
                .flat_map(char::to_lowercase)
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
        .flat_map(|w| rules::phonemes(&w).chars().collect::<Vec<_>>())
        .filter(|c| !matches!(c, 'ˈ' | 'ˌ' | ' ' | '-'))
        .collect()
}

/// Syllables: groups of vowel sounds.
fn syllables(sounds: &[char]) -> usize {
    let mut count = 0;
    let mut in_vowel = false;
    for &c in sounds {
        let v = is_vowel(c);
        if v && !in_vowel {
            count += 1;
        }
        in_vowel = v;
    }
    count
}

fn distance(a: &[char], b: &[char]) -> usize {
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = if ca == cb {
                prev
            } else {
                1 + prev.min(row[j]).min(row[j + 1])
            };
            prev = cur;
        }
    }
    row[b.len()]
}

/// How alike two phrases sound, 0–1 (phoneme edit distance).
pub fn sound_alike(a: &str, b: &str) -> f32 {
    let (pa, pb) = (phonemes(a), phonemes(b));
    let longest = pa.len().max(pb.len());
    if longest == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss, reason = "short phrases")]
    let similarity = 1.0 - distance(&pa, &pb) as f32 / longest as f32;
    similarity
}

/// Phrases at least this alike are confusable.
pub const CONFUSABLE: f32 = 0.75;

/// Checks `phrase` against the guidance, the other wake words and the fast-path command phrases.
/// `spellable` says whether the keyword model can spell it (`None` if the model isn't here).
pub fn assess(
    phrase: &str,
    other_wake_words: &[String],
    commands: &[String],
    spellable: Option<bool>,
) -> Assessment {
    let sounds = phonemes(phrase);
    let syllables = syllables(&sounds);
    let mut concerns = Vec::new();
    let mut risky = false;
    let mut fair = false;
    if sounds.is_empty() {
        return Assessment {
            quality: Quality::Risky,
            syllables: 0,
            phonemes: 0,
            concerns: vec![Concern::Empty],
        };
    }
    match syllables {
        0 | 1 => {
            concerns.push(Concern::OneSyllable);
            risky = true;
        }
        2 => {
            concerns.push(Concern::TwoSyllables);
            fair = true;
        }
        _ => {}
    }
    if sounds.len() < 6 {
        concerns.push(Concern::FewSounds {
            phonemes: sounds.len(),
        });
        if sounds.len() < 4 {
            risky = true;
        } else {
            fair = true;
        }
    }
    let mut vowels: Vec<char> = sounds.iter().copied().filter(|c| is_vowel(*c)).collect();
    vowels.sort_unstable();
    vowels.dedup();
    if syllables >= 2 && vowels.len() < 2 {
        concerns.push(Concern::SameVowels);
        fair = true;
    }
    let words: Vec<String> = phrase
        .split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|w| !w.is_empty())
        .collect();
    if words.iter().all(|w| COMMON.contains(&w.as_str())) {
        concerns.push(Concern::CommonWords);
        risky = true;
    }
    for other in other_wake_words {
        if !other.eq_ignore_ascii_case(phrase) && sound_alike(phrase, other) >= CONFUSABLE {
            concerns.push(Concern::LikeWakeWord {
                phrase: other.clone(),
            });
            risky = true;
        }
    }
    for command in commands {
        if sound_alike(phrase, command) >= CONFUSABLE {
            concerns.push(Concern::LikeCommand {
                phrase: command.clone(),
            });
            risky = true;
        }
    }
    if spellable == Some(false) {
        concerns.push(Concern::Unspellable);
        risky = true;
    }
    let quality = if risky {
        Quality::Risky
    } else if fair {
        Quality::Fair
    } else {
        Quality::Good
    };
    Assessment {
        quality,
        syllables,
        phonemes: sounds.len(),
        concerns,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(phrase: &str) -> Assessment {
        assess(
            phrase,
            &["Hey Kivo".into()],
            &["mute".into(), "open chrome".into()],
            None,
        )
    }

    #[test]
    fn hey_kivo_is_good() {
        let a = assess("Hey Kivo", &[], &[], Some(true));
        assert_eq!(a.quality, Quality::Good, "{a:?}");
        assert_eq!(a.syllables, 3);
    }

    #[test]
    fn short_and_common_phrases_are_risky() {
        assert_eq!(check("Bob").quality, Quality::Risky);
        assert!(check("Bob").concerns.contains(&Concern::OneSyllable));
        let hello = check("hello there");
        assert_eq!(hello.quality, Quality::Risky, "{hello:?}");
        assert!(hello.concerns.contains(&Concern::CommonWords));
        assert_eq!(check("").concerns, [Concern::Empty]);
    }

    #[test]
    fn two_syllables_are_fair() {
        let a = check("Jarvis");
        assert_eq!(a.quality, Quality::Fair, "{a:?}");
        assert!(a.concerns.contains(&Concern::TwoSyllables));
    }

    #[test]
    fn longer_unusual_phrases_are_good() {
        for phrase in ["Okay Computer", "Hey Aurora", "Wake up Friday"] {
            let a = check(phrase);
            assert_eq!(a.quality, Quality::Good, "{phrase}: {a:?}");
        }
    }

    #[test]
    fn phrases_like_other_words_or_commands_are_risky() {
        let near = check("Hey Keevo");
        assert!(
            near.concerns
                .iter()
                .any(|c| matches!(c, Concern::LikeWakeWord { .. })),
            "{near:?}"
        );
        let command = check("open Chrome");
        assert!(
            command
                .concerns
                .iter()
                .any(|c| matches!(c, Concern::LikeCommand { .. })),
            "{command:?}"
        );
        // A word the model can't spell.
        assert_eq!(
            assess("Hey Aurora", &[], &[], Some(false)).quality,
            Quality::Risky
        );
    }
}
