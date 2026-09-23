//! Spoken answers to a decision (CONVERSATION §7, CONV-27): a small local grammar, so answering
//! "yes" costs no AI call and is instant. English and Hindi (in Latin letters as people type it,
//! and in Devanagari as recognizers write it).

/// What the user said to a decision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    Approve,
    /// "Always for this …": approve and remember (where the card offers it).
    ApproveAlways,
    Deny,
    /// "Wait": the card stays, with no timeout ("Waiting for you").
    Defer,
    /// "Why?": KIVO explains the action and its risk, then asks again.
    Explain,
    /// "Change it to …": the rest goes to the brain (M3).
    Edit(String),
}

struct Phrases {
    approve: &'static [&'static str],
    always: &'static [&'static str],
    deny: &'static [&'static str],
    defer: &'static [&'static str],
    explain: &'static [&'static str],
    edit: &'static [&'static str],
}

const EN: Phrases = Phrases {
    approve: &[
        "yes",
        "yeah",
        "yep",
        "yup",
        "sure",
        "ok",
        "okay",
        "approve",
        "approved",
        "go ahead",
        "proceed",
        "do it",
        "send it",
        "send",
        "send that",
        "confirm",
        "allow",
        "go for it",
        "please do",
        "of course",
        "absolutely",
        "correct",
        "that's right",
        "thats right",
    ],
    always: &[
        "always",
        "always allow",
        "always for this project",
        "allow for this session",
        "for this session",
        "every time",
        "don't ask again",
        "dont ask again",
    ],
    deny: &[
        "no",
        "nope",
        "nah",
        "cancel",
        "don't",
        "dont",
        "deny",
        "stop",
        "never mind",
        "nevermind",
        "don't do it",
        "dont do it",
        "not now",
        "abort",
    ],
    defer: &[
        "wait",
        "hold on",
        "hang on",
        "let me think",
        "one moment",
        "one sec",
        "one second",
        "just a moment",
        "later",
        "give me a moment",
        "give me a second",
    ],
    explain: &[
        "why",
        "what will it do",
        "what does it do",
        "explain",
        "what is it",
        "what's that",
        "whats that",
        "tell me more",
        "what happens",
    ],
    edit: &[
        "change it to",
        "change it",
        "edit",
        "add",
        "make it",
        "instead",
    ],
};

const HI: Phrases = Phrases {
    approve: &[
        "haan",
        "han",
        "ha",
        "haa",
        "karo",
        "kar do",
        "theek hai",
        "thik hai",
        "theek",
        "chalo",
        "bilkul",
        "ji haan",
        "jee haan",
        "हाँ",
        "हां",
        "करो",
        "कर दो",
        "ठीक है",
        "बिल्कुल",
        "जी हाँ",
    ],
    always: &["hamesha", "hamesha ke liye", "हमेशा", "हमेशा के लिए"],
    deny: &[
        "nahi",
        "nahin",
        "na",
        "mat karo",
        "rehne do",
        "rahne do",
        "cancel karo",
        "नहीं",
        "मत करो",
        "रहने दो",
    ],
    defer: &[
        "ruko",
        "ruk jao",
        "ek minute",
        "ek second",
        "thoda ruko",
        "रुको",
        "रुक जाओ",
        "एक मिनट",
    ],
    explain: &[
        "kyon",
        "kyun",
        "kya karega",
        "kya hoga",
        "क्यों",
        "क्या करेगा",
        "क्या होगा",
    ],
    edit: &["badal do", "badlo", "बदल दो", "बदलो"],
};

fn words(text: &str) -> Vec<String> {
    crate::normalize::words(text)
}

/// Where `phrase` starts in `said` (as whole words), if it does.
fn find(said: &[String], phrase: &str) -> Option<usize> {
    let want = words(phrase);
    if want.is_empty() || want.len() > said.len() {
        return None;
    }
    (0..=said.len() - want.len()).find(|&i| said[i..i + want.len()] == want[..])
}

/// The earliest phrase of any intent in `said`: its position, length and intent.
fn earliest(said: &[String], phrases: &Phrases) -> Option<(usize, usize, Answer)> {
    let mut best: Option<(usize, usize, Answer)> = None;
    let mut consider = |list: &[&str], make: &dyn Fn(usize) -> Answer| {
        for phrase in list {
            if let Some(at) = find(said, phrase) {
                let len = words(phrase).len();
                // Earliest wins; at the same place, the longer phrase ("always allow" over "allow").
                let better = best
                    .as_ref()
                    .is_none_or(|(b_at, b_len, _)| at < *b_at || (at == *b_at && len > *b_len));
                if better {
                    best = Some((at, len, make(at + len)));
                }
            }
        }
    };
    consider(phrases.always, &|_| Answer::ApproveAlways);
    consider(phrases.approve, &|_| Answer::Approve);
    consider(phrases.deny, &|_| Answer::Deny);
    consider(phrases.defer, &|_| Answer::Defer);
    consider(phrases.explain, &|_| Answer::Explain);
    consider(phrases.edit, &|after| {
        Answer::Edit(said[after.min(said.len())..].join(" "))
    });
    best
}

/// The answer in `said`, in `language` (with English understood too).
pub fn parse_answer(said: &str, language: &str) -> Option<Answer> {
    let said = words(said);
    if said.is_empty() {
        return None;
    }
    let local = match language.split('-').next().unwrap_or("") {
        "hi" => Some(&HI),
        _ => None,
    };
    let found = [local, Some(&EN)]
        .into_iter()
        .flatten()
        .filter_map(|phrases| earliest(&said, phrases))
        .min_by_key(|(at, len, _)| (*at, usize::MAX - len));
    let (_, _, answer) = found?;
    // "Yes, always" is approval with scope.
    if answer == Answer::Approve
        && [local, Some(&EN)]
            .into_iter()
            .flatten()
            .any(|p| p.always.iter().any(|a| find(&said, a).is_some()))
    {
        return Some(Answer::ApproveAlways);
    }
    Some(answer)
}

/// A spoken change to a Draft card's prompt (CONVERSATION §5.4, CONV-15).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DraftEdit {
    /// "Add: also run the tests", "also …".
    Add(String),
    RemoveLastSentence,
    ReadBack,
    /// "Change it to …", "replace it with …".
    Replace(String),
}

/// Reads a Draft edit from what was said, before it is taken as an answer.
pub fn draft_edit(said: &str) -> Option<DraftEdit> {
    let trimmed = said.trim().trim_end_matches(['.', '!']);
    let lower = trimmed.to_lowercase();
    let lower = lower
        .trim_start_matches("kivo")
        .trim_start_matches([',', ' ']);
    let offset = trimmed.len() - lower.len();
    let rest = |prefix: &str| {
        let start = offset + prefix.len();
        trimmed
            .get(start..)
            .map(|r| r.trim_start_matches([':', ',', ' ']).trim().to_owned())
            .filter(|r| !r.is_empty())
    };
    for p in [
        "read it back",
        "read it to me",
        "read the prompt",
        "what does it say",
    ] {
        if lower == p || lower.starts_with(&format!("{p} ")) {
            return Some(DraftEdit::ReadBack);
        }
    }
    for p in [
        "remove the last sentence",
        "delete the last sentence",
        "take out the last sentence",
        "drop the last sentence",
    ] {
        if lower.starts_with(p) {
            return Some(DraftEdit::RemoveLastSentence);
        }
    }
    for p in [
        "change it to",
        "replace it with",
        "make it say",
        "instead say",
    ] {
        if lower.starts_with(p) {
            return rest(p).map(DraftEdit::Replace);
        }
    }
    for p in ["and add", "add", "also"] {
        if lower.starts_with(&format!("{p} ")) || lower.starts_with(&format!("{p}:")) {
            return rest(p).map(|r| {
                // "also run the tests" keeps its "also".
                if p == "also" {
                    DraftEdit::Add(format!("Also {r}"))
                } else {
                    DraftEdit::Add(r)
                }
            });
        }
    }
    None
}

/// The prompt after an edit.
pub fn apply_draft_edit(text: &str, edit: &DraftEdit) -> String {
    match edit {
        DraftEdit::ReadBack => text.to_owned(),
        DraftEdit::Replace(t) => t.clone(),
        DraftEdit::Add(more) => {
            let base = text.trim_end();
            if base.is_empty() {
                return capitalized(more);
            }
            let joiner = if base.ends_with(['.', '!', '?']) {
                " "
            } else {
                ". "
            };
            format!("{base}{joiner}{}", capitalized(more))
        }
        DraftEdit::RemoveLastSentence => {
            let base = text.trim_end().trim_end_matches(['.', '!', '?']);
            match base.rfind(['.', '!', '?']) {
                Some(i) => text[..=i].trim_end().to_owned(),
                None => String::new(),
            }
        }
    }
}

fn capitalized(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_edits_are_read_from_speech() {
        assert_eq!(
            draft_edit("Add: also run the tests"),
            Some(DraftEdit::Add("also run the tests".into()))
        );
        assert_eq!(
            draft_edit("also check the lint"),
            Some(DraftEdit::Add("Also check the lint".into()))
        );
        assert_eq!(draft_edit("Kivo, read it back."), Some(DraftEdit::ReadBack));
        assert_eq!(
            draft_edit("remove the last sentence"),
            Some(DraftEdit::RemoveLastSentence)
        );
        assert_eq!(
            draft_edit("change it to fix the failing test"),
            Some(DraftEdit::Replace("fix the failing test".into()))
        );
        assert_eq!(draft_edit("send"), None, "an answer, not an edit");
        assert_eq!(parse_answer("send", "en"), Some(Answer::Approve));
        let t = "Fix the failing test in K.I.V.O.";
        let t = apply_draft_edit(t, &DraftEdit::Add("also run the tests".into()));
        assert_eq!(t, "Fix the failing test in K.I.V.O. Also run the tests");
        let t = apply_draft_edit(&t, &DraftEdit::RemoveLastSentence);
        assert_eq!(t, "Fix the failing test in K.I.V.O.");
        assert_eq!(
            apply_draft_edit("One sentence", &DraftEdit::RemoveLastSentence),
            ""
        );
    }

    #[test]
    fn english_answers_are_understood() {
        let cases = [
            ("Yes.", Answer::Approve),
            ("Go ahead, please.", Answer::Approve),
            ("Yeah do it", Answer::Approve),
            ("No.", Answer::Deny),
            ("Cancel that.", Answer::Deny),
            ("Hold on.", Answer::Defer),
            ("Let me think.", Answer::Defer),
            ("Why?", Answer::Explain),
            ("What will it do?", Answer::Explain),
            ("Always allow.", Answer::ApproveAlways),
            ("Yes, always for this project.", Answer::ApproveAlways),
        ];
        for (said, want) in cases {
            assert_eq!(parse_answer(said, "en-US"), Some(want), "{said}");
        }
    }

    #[test]
    fn the_first_intent_said_wins() {
        assert_eq!(parse_answer("No, wait.", "en"), Some(Answer::Deny));
        assert_eq!(parse_answer("Wait, no.", "en"), Some(Answer::Defer));
    }

    #[test]
    fn edits_carry_what_to_change() {
        assert_eq!(
            parse_answer("Change it to Firefox", "en"),
            Some(Answer::Edit("firefox".into()))
        );
    }

    #[test]
    fn hindi_answers_are_understood() {
        let cases = [
            ("haan", Answer::Approve),
            ("theek hai", Answer::Approve),
            ("हाँ, कर दो", Answer::Approve),
            ("nahi", Answer::Deny),
            ("mat karo", Answer::Deny),
            ("ruko", Answer::Defer),
            ("एक मिनट", Answer::Defer),
            ("kyon", Answer::Explain),
        ];
        for (said, want) in cases {
            assert_eq!(parse_answer(said, "hi-IN"), Some(want), "{said}");
        }
        // English still works when the language is Hindi (code-mixing).
        assert_eq!(parse_answer("okay go ahead", "hi"), Some(Answer::Approve));
    }

    #[test]
    fn unrelated_speech_is_no_answer() {
        assert_eq!(parse_answer("what's the weather tomorrow", "en"), None);
        assert_eq!(parse_answer("", "en"), None);
    }
}
