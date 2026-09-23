//! Data classes (SECURITY §6) and the local detectors that assign them to text: secrets and keys,
//! card numbers (with the Luhn check), government ID patterns, contact details. Classification by
//! source (password fields, private folders) and user labels add to this where those exist. The
//! brain router sends anything `Sensitive` or above to a local brain (BRAINS §5 rule 3).

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

/// From least to most protected; `Ord` follows that order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataClass {
    Public,
    Normal,
    Personal,
    Sensitive,
    Credential,
    HighlySensitive,
}

impl DataClass {
    /// May this go to a cloud brain (with the privacy mode allowing cloud at all)?
    pub fn cloud_ok(self) -> bool {
        self <= DataClass::Personal
    }
}

struct Detector {
    class: DataClass,
    pattern: Regex,
    /// Extra check on the match (Luhn for cards).
    check: fn(&str) -> bool,
}

fn always(_: &str) -> bool {
    true
}

fn luhn(candidate: &str) -> bool {
    let digits: Vec<u32> = candidate.chars().filter_map(|c| c.to_digit(10)).collect();
    if !(13..=19).contains(&digits.len()) {
        return false;
    }
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &d)| {
            if i % 2 == 1 {
                let x = d * 2;
                if x > 9 { x - 9 } else { x }
            } else {
                d
            }
        })
        .sum();
    sum.is_multiple_of(10)
}

static DETECTORS: LazyLock<Vec<Detector>> = LazyLock::new(|| {
    let d = |class, pattern: &str, check: fn(&str) -> bool| Detector {
        class,
        pattern: Regex::new(pattern).expect("a valid detector"),
        check,
    };
    vec![
        // Keys and tokens with known shapes.
        d(
            DataClass::Credential,
            r"\b(sk-(?:ant-|proj-|or-)?[A-Za-z0-9_\-]{20,}|AKIA[0-9A-Z]{16}|gh[pousr]_[A-Za-z0-9]{36,}|xox[abpr]-[A-Za-z0-9-]{10,}|AIza[0-9A-Za-z_\-]{35})\b",
            always,
        ),
        d(
            DataClass::Credential,
            r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----",
            always,
        ),
        d(
            DataClass::Credential,
            r"(?i)\b(?:password|passcode|passwd|pin)\s*(?:is|:|=)\s*\S+",
            always,
        ),
        // Payment cards (13–19 digits, spaces or dashes allowed, Luhn-valid).
        d(
            DataClass::HighlySensitive,
            r"\b(?:\d[ -]?){12,18}\d\b",
            luhn,
        ),
        // US Social Security, India Aadhaar, IBAN.
        d(DataClass::HighlySensitive, r"\b\d{3}-\d{2}-\d{4}\b", always),
        d(
            DataClass::HighlySensitive,
            r"\b\d{4}\s\d{4}\s\d{4}\b",
            always,
        ),
        d(
            DataClass::HighlySensitive,
            r"\b[A-Z]{2}\d{2}(?:\s?[A-Z0-9]{4}){3,7}\b",
            always,
        ),
        // Contact details.
        d(
            DataClass::Personal,
            r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b",
            always,
        ),
        d(
            DataClass::Personal,
            r"(?:\+\d{1,3}[ -]?)?\(?\d{3}\)?[ -]?\d{3}[ -]?\d{4}\b",
            always,
        ),
    ]
});

/// The highest class any detector finds in `text` (`Normal` when none do).
pub fn classify(text: &str) -> DataClass {
    DETECTORS
        .iter()
        .filter(|d| d.pattern.find_iter(text).any(|m| (d.check)(m.as_str())))
        .map(|d| d.class)
        .max()
        .unwrap_or(DataClass::Normal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_requests_are_normal() {
        for text in [
            "What's the weather in Delhi?",
            "Explain this Rust error",
            "Open Chrome",
            "Call me at 5",
        ] {
            assert_eq!(classify(text), DataClass::Normal, "{text}");
        }
    }

    #[test]
    fn secrets_cards_and_ids_are_caught() {
        assert_eq!(
            classify("my key is sk-ant-api03-abcdefghijklmnopqrstuvwxyz"),
            DataClass::Credential
        );
        assert_eq!(
            classify("the wifi password is hunter2"),
            DataClass::Credential
        );
        assert_eq!(
            classify("card 4111 1111 1111 1111 exp 12/28"),
            DataClass::HighlySensitive
        );
        assert_eq!(
            classify("order 1234 5678 9012 3456"),
            DataClass::HighlySensitive,
            "twelve digits in fours also look like an Aadhaar number"
        );
        assert_eq!(classify("SSN 123-45-6789"), DataClass::HighlySensitive);
        assert_eq!(classify("mail maya@studio.com"), DataClass::Personal);
        assert!(!classify("SSN 123-45-6789").cloud_ok());
        assert!(classify("mail maya@studio.com").cloud_ok());
    }

    #[test]
    fn a_number_that_fails_luhn_is_not_a_card() {
        assert!(!luhn("4111 1111 1111 1112"));
        assert!(luhn("4111-1111-1111-1111"));
    }
}
