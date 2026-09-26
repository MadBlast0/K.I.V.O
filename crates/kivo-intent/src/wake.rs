//! Removing the wake phrase from a transcript (VOICE-05). Recognition starts a little before the
//! wake word ends, so "Hey Kivo, open Chrome" said in one breath is heard whole; the transcript
//! then begins with the end of the wake phrase ("Kivo, open Chrome"), often misheard ("Keyvo",
//! "Kiva"). Aligning the transcript's first words with the phrase's last ones removes it.

/// Edit distance between two short words.
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
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

/// A word's consonant skeleton: vowels and the soft letters (y, h, w) dropped, letters that
/// sound alike merged, doubles collapsed. "Kivo", "Keyvo", "Kevo" and "Kiva" all become "kv";
/// "kill" becomes "kl".
fn skeleton(word: &str) -> String {
    let mut out = String::new();
    for c in word.chars() {
        let c = match c {
            'a' | 'e' | 'i' | 'o' | 'u' | 'y' | 'h' | 'w' => continue,
            'c' | 'q' => 'k',
            'z' => 's',
            other => other,
        };
        if !out.ends_with(c) {
            out.push(c);
        }
    }
    out
}

/// A transcript word that sounds like a wake-phrase word: the same consonant skeleton, or (for
/// words that are nearly all vowels, like "hey") at most one edit away.
fn sounds_like(heard: &str, expected: &str) -> bool {
    if heard.is_empty() {
        return false;
    }
    let (a, b) = (skeleton(heard), skeleton(expected));
    // The tail of the word, clipped at its start by recognition starting mid-word ("Vivo" for
    // "Kivo"): the same last two letters, at most two edits away.
    let tail = |w: &str| w.chars().rev().take(2).collect::<String>();
    let clipped = expected.chars().count() >= 4
        && tail(heard) == tail(expected)
        && distance(heard, expected) <= 2;
    if b.chars().count() >= 2 || !a.is_empty() && !b.is_empty() {
        a == b || clipped
    } else {
        distance(heard, expected) <= 1
    }
}

fn core(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Whether `transcript` is only KIVO's name ("Kivo,", "Hey Kivo"): someone who says the name and
/// pauses before the request hasn't finished, whatever the pause sounds like (VOICE-33).
pub fn only_the_name(transcript: &str) -> bool {
    !transcript.trim().is_empty()
        && !strip_wake_phrase(transcript, "Hey Kivo")
            .chars()
            .any(char::is_alphanumeric)
}

/// `transcript` without the wake phrase (or its tail) at its start. The rest keeps its own
/// spelling and punctuation.
pub fn strip_wake_phrase(transcript: &str, phrase: &str) -> String {
    let expected: Vec<String> = phrase.split_whitespace().map(core).collect();
    // Words of the transcript with where each ends.
    let mut words = Vec::new();
    let mut at = 0;
    for piece in transcript.split_inclusive(char::is_whitespace) {
        at += piece.len();
        if !piece.trim().is_empty() {
            words.push((core(piece), at));
        }
    }
    for k in (1..=expected.len().min(words.len())).rev() {
        let tail = &expected[expected.len() - k..];
        if words[..k]
            .iter()
            .zip(tail)
            .all(|((heard, _), want)| sounds_like(heard, want))
        {
            return after(transcript, words[k - 1].1);
        }
    }
    // Recognizers split a name their own way ("A key vo" for "Hey Kivo"): align on sound, the
    // leading words' joined consonant skeleton spelling the phrase's.
    let target = joined_skeleton(expected.iter().map(String::as_str));
    if !target.is_empty() {
        for k in 1..=words.len().min(expected.len() + 2) {
            let heard = joined_skeleton(words[..k].iter().map(|(w, _)| w.as_str()));
            if heard == target {
                return after(transcript, words[k - 1].1);
            }
            if heard.len() > target.len() {
                break;
            }
        }
    }
    transcript.trim().to_owned()
}

/// The words' consonant skeletons joined, doubles across words collapsed.
fn joined_skeleton<'a>(words: impl Iterator<Item = &'a str>) -> String {
    let mut out = String::new();
    for c in words.flat_map(|w| skeleton(w).chars().collect::<Vec<_>>()) {
        if !out.ends_with(c) {
            out.push(c);
        }
    }
    out
}

/// What follows byte `at`, without the punctuation between.
fn after(transcript: &str, at: usize) -> String {
    transcript[at..]
        .trim_start_matches(|c: char| c.is_whitespace() || ",.!?;:-".contains(c))
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_alone_is_not_a_request() {
        for only in ["Kivo.", "Kivo,", "Hey Kivo", "hey, Kivo!", "Kevo", "Keyvo."] {
            assert!(only_the_name(only), "{only}");
        }
        for more in ["Kivo, mute.", "Kivo mute", "mute", "Hey", "", "  "] {
            assert!(!only_the_name(more), "{more:?}");
        }
    }

    #[test]
    fn the_whole_phrase_or_its_tail_is_removed() {
        let p = "Hey Kivo";
        assert_eq!(
            strip_wake_phrase("Hey Kivo, open Chrome.", p),
            "open Chrome."
        );
        assert_eq!(strip_wake_phrase("Kivo, open Chrome.", p), "open Chrome.");
        assert_eq!(strip_wake_phrase("hey kivo open chrome", p), "open chrome");
    }

    #[test]
    fn mishearings_are_removed_too() {
        let p = "Hey Kivo";
        assert_eq!(strip_wake_phrase("Keyvo, mute.", p), "mute.");
        // Recognition started mid-word.
        assert_eq!(strip_wake_phrase("Vivo mute", p), "mute");
        assert_eq!(strip_wake_phrase("ivo, open Chrome", p), "open Chrome");
        assert_eq!(
            strip_wake_phrase("Hay Kiva open Spotify", p),
            "open Spotify"
        );
        assert_eq!(
            strip_wake_phrase("Kevo. What time is it?", p),
            "What time is it?"
        );
    }

    #[test]
    fn a_phrase_split_differently_is_removed_by_sound() {
        let p = "Hey Kivo";
        assert_eq!(strip_wake_phrase("A key vo mute", p), "mute");
        assert_eq!(
            strip_wake_phrase("Hey, key, vo. Open Chrome.", p),
            "Open Chrome."
        );
        assert_eq!(strip_wake_phrase("Okay, Kee Voh, lock it", p), "lock it");
    }

    #[test]
    fn words_that_only_look_alike_are_kept() {
        let p = "Hey Kivo";
        assert_eq!(strip_wake_phrase("kill Chrome", p), "kill Chrome");
        assert_eq!(
            strip_wake_phrase("give me the weather", p),
            "give me the weather"
        );
    }

    #[test]
    fn a_request_that_doesnt_start_with_the_phrase_is_kept() {
        let p = "Hey Kivo";
        assert_eq!(strip_wake_phrase("Open Chrome.", p), "Open Chrome.");
        assert_eq!(strip_wake_phrase("Mute.", p), "Mute.");
        assert_eq!(strip_wake_phrase("", p), "");
        // Only the wake word: nothing left to do.
        assert_eq!(strip_wake_phrase("Hey Kivo.", p), "");
    }

    #[test]
    fn custom_phrases_work_the_same_way() {
        let p = "Okay Computer";
        assert_eq!(
            strip_wake_phrase("computer, lock the screen", p),
            "lock the screen"
        );
        assert_eq!(
            strip_wake_phrase("OK computer lock the screen", p),
            "lock the screen"
        );
    }
}
