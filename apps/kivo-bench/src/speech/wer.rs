//! Word error rate: (substitutions + deletions + insertions) / reference words, after the usual
//! normalization (lower case, punctuation removed), so engines that punctuate and case their
//! output are compared fairly with plain reference transcripts.

/// Lower-cases and keeps letters, digits and apostrophes; everything else separates words.
pub fn normalize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .map(|w| w.trim_matches('\''))
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Word-level edit distance between a reference and a hypothesis.
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

/// Accumulates errors over a test set.
#[derive(Debug, Default, Clone, Copy)]
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

    /// WER in percent.
    pub fn percent(&self) -> f64 {
        #[allow(clippy::cast_precision_loss, reason = "word counts")]
        if self.words == 0 {
            0.0
        } else {
            self.errors as f64 / self.words as f64 * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_ignores_case_and_punctuation() {
        assert_eq!(
            normalize("Hello, World! It's KIVO."),
            ["hello", "world", "it's", "kivo"]
        );
        assert_eq!(normalize("  "), Vec::<String>::new());
    }

    #[test]
    fn edit_distance_counts_each_kind_of_error() {
        let w = |s: &str| normalize(s);
        assert_eq!(errors(&w("open chrome now"), &w("open chrome now")), 0);
        assert_eq!(errors(&w("open chrome now"), &w("open chrome")), 1); // deletion
        assert_eq!(errors(&w("open chrome"), &w("please open chrome")), 1); // insertion
        assert_eq!(errors(&w("open chrome"), &w("open crome")), 1); // substitution
        assert_eq!(errors(&w("a b c"), &w("")), 3);
    }

    #[test]
    fn tallies_give_word_error_rate() {
        let mut t = Tally::default();
        t.add("OPEN CHROME", "Open Chrome.");
        t.add("MUTE THE VOLUME", "mute volume");
        assert_eq!((t.errors, t.words), (1, 5));
        assert!((t.percent() - 20.0).abs() < 1e-9);
    }
}
