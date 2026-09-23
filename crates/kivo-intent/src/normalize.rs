//! Turning a transcript into matchable words (BRAINS §2): lower case, punctuation removed except
//! inside web addresses, apostrophes dropped, `%` spoken as "percent", fillers stripped.

/// Words of `text`, normalized.
pub fn words(text: &str) -> Vec<String> {
    let lower = text
        .to_lowercase()
        .replace('%', " percent ")
        .replace(['’', '‘'], "'");
    let mut out = Vec::new();
    for raw in lower.split_whitespace() {
        // Keep dots only between letters or digits ("youtube.com"). Apostrophes are dropped
        // ("what's" → "whats"): people type contractions both ways.
        let chars: Vec<char> = raw.chars().collect();
        let mut word = String::new();
        for (i, &c) in chars.iter().enumerate() {
            let inner = |c: char| c.is_alphanumeric();
            let between =
                i > 0 && i + 1 < chars.len() && inner(chars[i - 1]) && inner(chars[i + 1]);
            if c == '\'' {
                continue;
            }
            if c.is_alphanumeric() || ((c == '.' || c == '/') && between) || c == '-' && between {
                word.push(c);
            } else if !word.is_empty() && !c.is_alphanumeric() && c != '.' {
                // A symbol inside a word splits it ("rock&roll" → "rock", "roll").
                out.push(std::mem::take(&mut word));
            }
        }
        if !word.is_empty() {
            out.push(word);
        }
    }
    out
}

/// Removes leading and trailing filler phrases (each given as words), repeatedly.
pub fn strip_fillers(
    mut words: Vec<String>,
    leading: &[Vec<String>],
    trailing: &[Vec<String>],
) -> Vec<String> {
    loop {
        let before = words.len();
        for filler in leading {
            if words.len() > filler.len() && words.starts_with(filler) {
                words.drain(..filler.len());
            }
        }
        for filler in trailing {
            if words.len() > filler.len() && words.ends_with(filler) {
                words.truncate(words.len() - filler.len());
            }
        }
        if words.len() == before {
            return words;
        }
    }
}

/// A number 0–100 spoken as digits or words: "50", "fifty", "twenty five", "a hundred", with an
/// optional trailing "percent".
pub fn number(words: &[String]) -> Option<u32> {
    let mut words: Vec<&str> = words.iter().map(String::as_str).collect();
    if words.last() == Some(&"percent") {
        words.pop();
    }
    match words.as_slice() {
        [digits] if digits.chars().all(|c| c.is_ascii_digit()) => {
            digits.parse().ok().filter(|n| *n <= 100)
        }
        ["a" | "one", "hundred"] | ["hundred"] | ["max" | "maximum" | "full"] => Some(100),
        ["zero" | "none" | "nothing"] => Some(0),
        [tens, ones] => Some(tens_value(tens)? + unit_value(ones).filter(|u| (1..10).contains(u))?),
        [one] => unit_value(one)
            .or_else(|| teen_value(one))
            .or_else(|| tens_value(one)),
        _ => None,
    }
}

fn unit_value(w: &str) -> Option<u32> {
    const UNITS: [&str; 10] = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
    ];
    UNITS
        .iter()
        .position(|u| *u == w)
        .and_then(|i| u32::try_from(i).ok())
}

fn teen_value(w: &str) -> Option<u32> {
    const TEENS: [&str; 10] = [
        "ten",
        "eleven",
        "twelve",
        "thirteen",
        "fourteen",
        "fifteen",
        "sixteen",
        "seventeen",
        "eighteen",
        "nineteen",
    ];
    TEENS
        .iter()
        .position(|t| *t == w)
        .and_then(|i| u32::try_from(i + 10).ok())
}

fn tens_value(w: &str) -> Option<u32> {
    const TENS: [&str; 8] = [
        "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
    ];
    TENS.iter()
        .position(|t| *t == w)
        .and_then(|i| u32::try_from((i + 2) * 10).ok())
}

/// A web address: "youtube.com", "www.bbc.co.uk/news", or spoken "youtube dot com". Returns an
/// `https://` URL.
pub fn url(words: &[String]) -> Option<String> {
    let joined = words.join(" ").replace(" dot ", ".").replace(' ', "");
    let host = joined
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let domain = host.split('/').next()?;
    let labels: Vec<&str> = domain.split('.').collect();
    let tld = labels.last()?;
    let valid = labels.len() >= 2
        && labels
            .iter()
            .all(|l| !l.is_empty() && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
        && tld.len() >= 2
        && tld.chars().all(|c| c.is_ascii_alphabetic());
    valid.then(|| format!("https://{host}"))
}

/// The stretch of the user's own words that normalizes to `slot` (a text slot's words), with
/// its case, punctuation and paths intact: "Remember that my project is in D:\work." with the
/// slot "my project is in d work" gives "my project is in D:\work.". `None` if no stretch does.
pub fn original_span(original: &str, slot: &str) -> Option<String> {
    let tokens: Vec<&str> = original.split_whitespace().collect();
    let wanted: Vec<String> = slot.split_whitespace().map(str::to_owned).collect();
    if wanted.is_empty() {
        return None;
    }
    for start in 0..tokens.len() {
        for end in (start + 1..=tokens.len()).rev() {
            if words(&tokens[start..end].join(" ")) == wanted {
                return Some(tokens[start..end].join(" "));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_is_found_in_the_users_own_words() {
        assert_eq!(
            original_span(
                "Kivo, remember that my project is in D:\\work.",
                "my project is in d work"
            )
            .as_deref(),
            Some("my project is in D:\\work.")
        );
        assert_eq!(
            original_span(
                "remember Maya's birthday is May 3rd please",
                "mayas birthday is may 3rd"
            )
            .as_deref(),
            Some("Maya's birthday is May 3rd")
        );
        assert_eq!(original_span("remember nothing", "something else"), None);
    }

    fn w(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn transcripts_become_plain_words() {
        assert_eq!(words("Open Chrome."), ["open", "chrome"]);
        assert_eq!(
            words("Set the volume to 50%!"),
            ["set", "the", "volume", "to", "50", "percent"]
        );
        assert_eq!(
            words("Go to YouTube.com, please"),
            ["go", "to", "youtube.com", "please"]
        );
        assert_eq!(words("What’s playing?"), ["whats", "playing"]);
        assert_eq!(words("whats playing"), words("what's playing"));
        assert_eq!(words("  --  "), Vec::<String>::new());
    }

    #[test]
    fn fillers_come_off_both_ends_but_never_empty_the_command() {
        let leading = [w("hey kivo"), w("please"), w("can you")];
        let trailing = [w("please"), w("for me")];
        assert_eq!(
            strip_fillers(
                w("hey kivo can you open chrome for me please"),
                &leading,
                &trailing
            ),
            w("open chrome")
        );
        assert_eq!(strip_fillers(w("please"), &leading, &trailing), w("please"));
    }

    #[test]
    fn numbers_in_digits_and_words() {
        assert_eq!(number(&w("50")), Some(50));
        assert_eq!(number(&w("50 percent")), Some(50));
        assert_eq!(number(&w("twenty five")), Some(25));
        assert_eq!(number(&w("fifteen")), Some(15));
        assert_eq!(number(&w("a hundred")), Some(100));
        assert_eq!(number(&w("seven")), Some(7));
        assert_eq!(number(&w("150")), None);
        assert_eq!(number(&w("twenty ten")), None);
        assert_eq!(number(&w("loud")), None);
    }

    #[test]
    fn web_addresses_typed_or_spoken() {
        assert_eq!(
            url(&w("youtube.com")).as_deref(),
            Some("https://youtube.com")
        );
        assert_eq!(
            url(&w("youtube dot com")).as_deref(),
            Some("https://youtube.com")
        );
        assert_eq!(
            url(&w("bbc.co.uk/news")).as_deref(),
            Some("https://bbc.co.uk/news")
        );
        assert_eq!(url(&w("chrome")), None);
        assert_eq!(url(&w("version 2.5")), None);
    }
}
