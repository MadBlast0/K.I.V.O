//! Word error rate for the owner's own voice (VOICE-23): the enrollment prompts are known text,
//! so each installed recognizer can be scored on how well it hears *this* user. Numbers are
//! compared digit by digit in words ("4815" = "four eight one five"), since recognizers differ
//! in how they write them.

const DIGITS: [&str; 10] = [
    "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
];

/// Lower case, punctuation removed, digits spelled out one by one.
pub fn normalize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for word in text
        .to_lowercase()
        .split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .map(|w| w.trim_matches('\''))
        .filter(|w| !w.is_empty())
    {
        if word.chars().all(|c| c.is_ascii_digit()) {
            out.extend(
                word.chars()
                    .filter_map(|c| c.to_digit(10))
                    .map(|d| DIGITS[d as usize].to_owned()),
            );
        } else {
            out.push(word.to_owned());
        }
    }
    out
}

/// Word-level edit distance.
pub fn errors(reference: &[String], hypothesis: &[String]) -> usize {
    let mut previous: Vec<usize> = (0..=hypothesis.len()).collect();
    for (i, r) in reference.iter().enumerate() {
        let mut current = vec![i + 1; hypothesis.len() + 1];
        for (j, h) in hypothesis.iter().enumerate() {
            let substitution = previous[j] + usize::from(r != h);
            current[j + 1] = substitution.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        previous = current;
    }
    previous[hypothesis.len()]
}

/// Errors and words over several utterances.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Tally {
    pub errors: usize,
    pub words: usize,
}

impl Tally {
    pub fn add(&mut self, reference: &str, hypothesis: &str) {
        let (r, h) = (normalize(reference), normalize(hypothesis));
        self.errors += errors(&r, &h);
        self.words += r.len();
    }

    /// 0–1 (can exceed 1 with many insertions).
    pub fn rate(&self) -> f64 {
        #[allow(clippy::cast_precision_loss, reason = "word counts")]
        if self.words == 0 {
            0.0
        } else {
            self.errors as f64 / self.words as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_and_punctuation_compare_fairly() {
        let mut t = Tally::default();
        t.add(
            "My code is four eight one five, and today is the twenty-third",
            "My code is 4815 and today is the twenty third.",
        );
        assert_eq!(t.errors, 0);
        t.add("Hey Kivo, open my music", "hey keyvo open music");
        assert_eq!(t.errors, 2);
        assert_eq!(t.words, 18);
        assert!((t.rate() - 2.0 / 18.0).abs() < 1e-9);
    }
}
