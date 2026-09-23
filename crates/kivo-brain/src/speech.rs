//! The spoken side of a brain's answer (BRAINS §11, BRAIN-28/29). The card shows the rich text;
//! speech gets a speakable version, phrase by phrase as the answer streams, so KIVO starts
//! talking before the brain has finished:
//!
//! - no markdown in speech (headings, bold, bullets and links become plain words);
//! - a code block is replaced, once, with "I've put the details on screen";
//! - numbers, units, times, money and web addresses are written the way they're said.

/// Splits a streaming answer into phrases for text-to-speech (BRAIN-28).
#[derive(Default)]
pub struct Chunker {
    buffer: String,
    in_code: bool,
    said_code: bool,
    /// Phrases handed out so far (the first one goes out early for quick first audio).
    phrases: usize,
}

/// The line spoken in place of code.
pub const ON_SCREEN: &str = "I've put the details on screen.";

impl Chunker {
    /// Adds streamed text; returns the phrases now complete, ready to speak.
    pub fn push(&mut self, delta: &str) -> Vec<String> {
        self.buffer.push_str(delta);
        let mut out = Vec::new();
        loop {
            // Code fences hold everything until they close; the code itself is never spoken.
            if self.in_code {
                let Some(end) = self.buffer.find("```") else {
                    return out;
                };
                let after = end + 3;
                let rest_line = self.buffer[after..]
                    .find('\n')
                    .map_or(self.buffer.len(), |n| after + n + 1);
                self.buffer.drain(..rest_line.min(self.buffer.len()));
                self.in_code = false;
                continue;
            }
            let fence = self.buffer.find("```");
            let cut = self.cut_point();
            // Sentences before a fence go first, one by one.
            if let Some(end) = cut.filter(|&end| fence.is_none_or(|f| end <= f)) {
                let phrase: String = self.buffer.drain(..end).collect();
                out.extend(self.speakable(&phrase));
                continue;
            }
            if let Some(fence) = fence {
                let before: String = self.buffer[..fence].to_owned();
                self.buffer.drain(..fence + 3);
                out.extend(self.flush_text(&before));
                if !self.said_code {
                    self.said_code = true;
                    out.push(ON_SCREEN.to_owned());
                }
                self.in_code = true;
                continue;
            }
            return out;
        }
    }

    /// The end of the answer: whatever is left.
    pub fn finish(&mut self) -> Vec<String> {
        if self.in_code {
            self.buffer.clear();
            return Vec::new();
        }
        let rest = std::mem::take(&mut self.buffer);
        self.speakable(&rest).into_iter().collect()
    }

    fn flush_text(&mut self, text: &str) -> Option<String> {
        self.speakable(text)
    }

    fn speakable(&mut self, text: &str) -> Option<String> {
        let s = speakable(text);
        if s.trim().is_empty() {
            return None;
        }
        self.phrases += 1;
        Some(s.trim().to_owned())
    }

    /// Where the next phrase ends: a sentence end followed by space, a line break, or — for the
    /// first phrase — a clause break after enough words, so speech starts early.
    fn cut_point(&self) -> Option<usize> {
        let b = &self.buffer;
        let bytes = b.as_bytes();
        for (i, c) in b.char_indices() {
            let next = bytes.get(i + c.len_utf8()).copied();
            let followed_by_space = matches!(next, Some(b' ' | b'\n'));
            match c {
                '\n' => {
                    if b[..i].trim().is_empty() {
                        continue;
                    }
                    return Some(i + 1);
                }
                '.' | '!' | '?' | '。' if followed_by_space => {
                    // "e.g. " and "3.5 " aren't sentence ends.
                    let word = b[..i].rsplit(' ').next().unwrap_or_default().to_lowercase();
                    let abbreviation = c == '.'
                        && (word.contains('.')
                            || ["mr", "mrs", "ms", "dr", "vs", "etc", "st", "no", "approx"]
                                .contains(&word.as_str()));
                    if abbreviation {
                        continue;
                    }
                    return Some(i + 1);
                }
                ',' | ';' | ':'
                    if followed_by_space
                        && self.phrases == 0
                        && b[..i].split_whitespace().count() >= 6 =>
                {
                    return Some(i + 1);
                }
                _ => {}
            }
        }
        None
    }
}

/// Markdown removed and numbers, units and addresses written as they're said (BRAIN-29).
pub fn speakable(text: &str) -> String {
    let mut lines = Vec::new();
    for raw in text.lines() {
        let mut line = raw.trim_start();
        // Headings, bullets, numbered lists, quotes.
        line = line.trim_start_matches('#').trim_start();
        for marker in ["- ", "* ", "+ ", "> "] {
            if let Some(rest) = line.strip_prefix(marker) {
                line = rest;
            }
        }
        let numbered = line.split_once(". ").filter(|(n, _)| {
            !n.is_empty() && n.len() <= 3 && n.chars().all(|c| c.is_ascii_digit())
        });
        if let Some((_, rest)) = numbered {
            line = rest;
        }
        if line
            .chars()
            .all(|c| matches!(c, '-' | '*' | '_' | '|' | ' ' | ':'))
        {
            continue; // rules and table separators
        }
        lines.push(line.to_owned());
    }
    let mut s = lines.join(" ");
    // Links: [text](url) → text.
    while let (Some(open), Some(mid)) = (s.find('['), s.find("](")) {
        if mid < open {
            break;
        }
        let Some(close) = s[mid..].find(')') else {
            break;
        };
        let label = s[open + 1..mid].to_owned();
        s.replace_range(open..mid + close + 1, &label);
    }
    for mark in ["**", "__", "`", "*", "~~"] {
        s = s.replace(mark, "");
    }
    s = s.replace('|', ", ");
    normalize(&s)
}

/// Numbers, units, times, money and web addresses, as words a voice reads well.
pub fn normalize(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        out.push(normalize_word(word));
    }
    out.join(" ")
}

fn normalize_word(word: &str) -> String {
    let (core, trail) = split_trailing_punct(word);
    // Web addresses: https://www.example.com/path → example dot com.
    if let Some(host) = url_host(core) {
        return format!("{}{trail}", host.replace('.', " dot "));
    }
    // Money.
    for (sym, name) in [
        ("$", "dollars"),
        ("€", "euros"),
        ("£", "pounds"),
        ("₹", "rupees"),
    ] {
        if let Some(amount) = core.strip_prefix(sym)
            && amount.chars().next().is_some_and(|c| c.is_ascii_digit())
        {
            let amount = amount.replace(',', "");
            return match amount.split_once('.') {
                Some((whole, cents)) if cents.len() == 2 && cents != "00" => {
                    format!("{whole} {name} {cents}{trail}")
                }
                Some((whole, _)) => format!("{whole} {name}{trail}"),
                None => format!("{amount} {name}{trail}"),
            };
        }
    }
    if let Some(p) = core.strip_suffix('%')
        && p.chars().all(|c| c.is_ascii_digit() || c == '.')
        && !p.is_empty()
    {
        return format!("{p} percent{trail}");
    }
    // Units after a number: 5km, 20°C, 3GB.
    let digits_end = core
        .char_indices()
        .find(|(_, c)| !(c.is_ascii_digit() || *c == '.' || *c == ','))
        .map_or(core.len(), |(i, _)| i);
    if digits_end > 0 && digits_end < core.len() {
        let (number, unit) = core.split_at(digits_end);
        let unit_word = match unit {
            "km" => Some("kilometres"),
            "m" => Some("metres"),
            "cm" => Some("centimetres"),
            "kg" => Some("kilograms"),
            "g" => Some("grams"),
            "mb" | "MB" => Some("megabytes"),
            "gb" | "GB" => Some("gigabytes"),
            "tb" | "TB" => Some("terabytes"),
            "ms" => Some("milliseconds"),
            "s" | "sec" => Some("seconds"),
            "min" => Some("minutes"),
            "h" | "hr" | "hrs" => Some("hours"),
            "°C" | "C" => Some("degrees Celsius"),
            "°F" | "F" => Some("degrees Fahrenheit"),
            "mph" => Some("miles per hour"),
            "km/h" => Some("kilometres per hour"),
            _ => None,
        };
        if let Some(unit_word) = unit_word {
            return format!("{number} {unit_word}{trail}");
        }
    }
    // Large numbers with separators read as plain numbers.
    if core.contains(',') && core.chars().all(|c| c.is_ascii_digit() || c == ',') {
        return format!("{}{trail}", core.replace(',', ""));
    }
    word.to_owned()
}

fn split_trailing_punct(word: &str) -> (&str, &str) {
    let end = word
        .char_indices()
        .rev()
        .take_while(|(_, c)| matches!(c, '.' | ',' | '!' | '?' | ';' | ':' | ')'))
        .last()
        .map_or(word.len(), |(i, _)| i);
    word.split_at(end)
}

fn url_host(word: &str) -> Option<String> {
    let rest = word
        .strip_prefix("https://")
        .or_else(|| word.strip_prefix("http://"))
        .or_else(|| word.starts_with("www.").then_some(word))?;
    let host = rest.split('/').next()?.trim_start_matches("www.");
    host.contains('.').then(|| host.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(pieces: &[&str]) -> Vec<String> {
        let mut c = Chunker::default();
        let mut out: Vec<String> = pieces.iter().flat_map(|p| c.push(p)).collect();
        out.extend(c.finish());
        out
    }

    #[test]
    fn phrases_come_out_as_sentences_complete() {
        let out = stream(&[
            "It's sunny tod",
            "ay. Highs of 24",
            "°C and a light ",
            "breeze! Enjoy",
        ]);
        assert_eq!(
            out,
            [
                "It's sunny today.",
                "Highs of 24 degrees Celsius and a light breeze!",
                "Enjoy"
            ]
        );
    }

    #[test]
    fn the_first_phrase_goes_out_at_a_clause_break_for_quick_first_audio() {
        let mut c = Chunker::default();
        let first = c.push("Here is what I found about your question, ");
        assert_eq!(first, ["Here is what I found about your question,"]);
        assert!(
            c.push("and more words, ").is_empty(),
            "later phrases wait for a sentence end"
        );
    }

    #[test]
    fn code_is_shown_not_spoken() {
        let out = stream(&[
            "Try this:\n```rust\nfn main() {\n    println!(\"hi\");\n}",
            "\n```\nThen run it. Also:\n```\nmore code\n```\nDone.",
        ]);
        assert_eq!(
            out,
            ["Try this:", ON_SCREEN, "Then run it.", "Also:", "Done."]
        );
        assert!(!out.iter().any(|p| p.contains("println")));
    }

    #[test]
    fn markdown_numbers_and_addresses_become_speakable() {
        assert_eq!(
            speakable(
                "## Summary\n- **Price**: $1,299.99\n- See [the docs](https://docs.example.com/x)"
            ),
            "Summary Price: 1299 dollars 99 See the docs"
        );
        assert_eq!(
            normalize("Download 3.5GB at 20% off from https://www.example.com/sale."),
            "Download 3.5 gigabytes at 20 percent off from example dot com."
        );
        assert_eq!(
            normalize("It took 250ms and 1,000,000 users"),
            "It took 250 milliseconds and 1000000 users"
        );
        assert_eq!(normalize("e.g. this"), "e.g. this");
    }

    #[test]
    fn abbreviations_are_not_sentence_ends() {
        let out = stream(&["Use a tool, e.g. a hammer. Then stop."]);
        assert_eq!(out, ["Use a tool, e.g. a hammer.", "Then stop."]);
    }
}
