//! "Stop" as a whole request (VOICE-19, SEC-26): when the user talks over KIVO, barge-in often
//! catches "Kivo, stop" before the stop-word spotter finishes it, so the recognizer hears it as a
//! request. A request that is only a stop word ends the turn quietly instead of being answered.

use crate::normalize::words;

/// Whole requests that mean "stop", after the wake word and fillers are removed (EN + HI).
const STOP: &[&str] = &[
    "stop",
    "stop it",
    "stop that",
    "stop talking",
    "cancel",
    "cancel that",
    "never mind",
    "nevermind",
    "forget it",
    "be quiet",
    "quiet",
    "shut up",
    "enough",
    "thats enough",
    "ruko",
    "ruk jao",
    "bas",
    "bas karo",
    "chup",
    "band karo",
];

/// Leading or trailing words that don't change the meaning ("hey Kivo, stop please").
const AROUND: &[&str] = &[
    "hey", "okay", "ok", "kivo", "please", "now", "right", "just",
];

/// True when the whole of `transcript` asks KIVO to stop.
pub fn is_stop_request(transcript: &str) -> bool {
    let all = words(transcript);
    let mut said: &[String] = &all;
    while let [first, rest @ ..] = said
        && AROUND.contains(&first.as_str())
    {
        said = rest;
    }
    while let [rest @ .., last] = said
        && AROUND.contains(&last.as_str())
    {
        said = rest;
    }
    let phrase = said.join(" ");
    !phrase.is_empty() && STOP.contains(&phrase.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_words_on_their_own_are_stop_requests() {
        for said in [
            "Stop.",
            "Kivo, stop.",
            "Hey Kivo, stop it please",
            "Never mind",
            "Cancel!",
            "Ruko",
        ] {
            assert!(is_stop_request(said), "{said}");
        }
    }

    #[test]
    fn requests_that_only_mention_stopping_are_not() {
        for said in [
            "Stop the music",
            "Cancel my meeting",
            "Kivo",
            "",
            "Don't stop",
        ] {
            assert!(!is_stop_request(said), "{said}");
        }
    }
}
