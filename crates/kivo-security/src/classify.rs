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

/// `text` with every credential the detectors find (keys, tokens, private keys, "password is …")
/// replaced by `[removed]`, for text KIVO keeps (memory notes never hold secrets).
pub fn redact_secrets(text: &str) -> String {
    let mut out = text.to_owned();
    for d in DETECTORS
        .iter()
        .filter(|d| d.class == DataClass::Credential)
    {
        out = d.pattern.replace_all(&out, "[removed]").into_owned();
    }
    out
}

/// `classify`, then the user's labels (SECURITY §6): a label found in the text, in any case,
/// raises it to the label's class.
pub fn classify_labeled(text: &str, labels: &[kivo_core::config::PrivacyLabel]) -> DataClass {
    use kivo_core::config::LabelClass;
    let lower = text.to_lowercase();
    labels
        .iter()
        .filter(|l| !l.text.trim().is_empty() && lower.contains(&l.text.trim().to_lowercase()))
        .map(|l| match l.class {
            LabelClass::Personal => DataClass::Personal,
            LabelClass::Sensitive => DataClass::Sensitive,
            LabelClass::HighlySensitive => DataClass::HighlySensitive,
        })
        .fold(classify(text), DataClass::max)
}

/// The class of a file by where it is (SECURITY §6, by source): inside a folder the user keeps on
/// this PC it's `Sensitive`, else `Normal`. Compared by path components, ignoring case (Windows).
pub fn classify_path(path: &str, sensitive_folders: &[String]) -> DataClass {
    let parts = |p: &str| -> Vec<String> {
        p.split(['\\', '/'])
            .filter(|c| !c.is_empty() && *c != ".")
            .map(|c| c.trim_start_matches("?").to_lowercase())
            .filter(|c| !c.is_empty())
            .collect()
    };
    let file = parts(path);
    let inside = sensitive_folders.iter().any(|folder| {
        let folder = parts(folder);
        !folder.is_empty() && file.len() > folder.len() && file[..folder.len()] == folder[..]
    });
    if inside {
        DataClass::Sensitive
    } else {
        DataClass::Normal
    }
}

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
    fn labels_and_private_folders_classify_by_source() {
        use kivo_core::config::{LabelClass, PrivacyLabel};
        let labels = [PrivacyLabel {
            text: "Project Falcon".into(),
            class: LabelClass::Sensitive,
        }];
        assert_eq!(
            classify_labeled("summarize the project falcon budget", &labels),
            DataClass::Sensitive
        );
        assert_eq!(
            classify_labeled("summarize the budget", &labels),
            DataClass::Normal
        );
        // A detector finding more still wins.
        assert_eq!(
            classify_labeled(
                "Project Falcon key sk-ant-api03-abcdefghijklmnopqrstuvwxyz",
                &labels
            ),
            DataClass::Credential
        );
        let private = ["C:\\Users\\me\\Documents\\Legal".to_owned()];
        assert_eq!(
            classify_path("c:/users/ME/documents/legal/contract.pdf", &private),
            DataClass::Sensitive
        );
        assert_eq!(
            classify_path("\\\\?\\C:\\Users\\me\\Documents\\Legal\\a\\b.txt", &private),
            DataClass::Sensitive
        );
        assert_eq!(
            classify_path("C:\\Users\\me\\Documents\\Legalese\\x.txt", &private),
            DataClass::Normal,
            "a folder with the same start isn't inside"
        );
        assert_eq!(
            classify_path("C:\\Users\\me\\Documents\\Legal", &private),
            DataClass::Normal
        );
    }

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
    fn secrets_are_redacted_and_the_rest_kept() {
        let text = "Deploy key sk-ant-api03-abcdefghijklmnopqrstuvwx and the wifi password is hunter22; mail maya@studio.com";
        let out = redact_secrets(text);
        assert!(
            !out.contains("sk-ant") && !out.contains("hunter22"),
            "{out}"
        );
        assert!(
            out.contains("maya@studio.com") && out.contains("Deploy key [removed]"),
            "{out}"
        );
        assert_eq!(classify(&out), DataClass::Personal);
    }

    #[test]
    fn a_number_that_fails_luhn_is_not_a_card() {
        assert!(!luhn("4111 1111 1111 1112"));
        assert!(luhn("4111-1111-1111-1111"));
    }
}
