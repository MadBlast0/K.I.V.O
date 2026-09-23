//! English number words for the phonemizer: cardinals, ordinals, years and digit strings
//! ("twenty twenty-six", "one hundred and five" is spoken without "and", as in US English).

const ONES: [&str; 20] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
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
const TENS: [&str; 10] = [
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];
const SCALES: [(u64, &str); 4] = [
    (1_000_000_000_000, "trillion"),
    (1_000_000_000, "billion"),
    (1_000_000, "million"),
    (1_000, "thousand"),
];

/// 0–999.
fn hundreds(n: u64, out: &mut Vec<String>) {
    let (h, rest) = (n / 100, n % 100);
    if h > 0 {
        out.push(ONES[h as usize].into());
        out.push("hundred".into());
    }
    if rest > 0 || (h == 0 && out.is_empty()) {
        tens(rest, out);
    }
}

/// 0–99.
fn tens(n: u64, out: &mut Vec<String>) {
    if n < 20 {
        out.push(ONES[n as usize].into());
    } else if n.is_multiple_of(10) {
        out.push(TENS[(n / 10) as usize].into());
    } else {
        out.push(format!(
            "{}-{}",
            TENS[(n / 10) as usize],
            ONES[(n % 10) as usize]
        ));
    }
}

/// "one thousand two hundred thirty-four".
pub fn cardinal(n: u64) -> Vec<String> {
    let mut out = Vec::new();
    if n == 0 {
        out.push("zero".into());
        return out;
    }
    let mut rest = n;
    for (scale, name) in SCALES {
        if rest >= scale {
            hundreds(rest / scale, &mut out);
            out.push(name.into());
            rest %= scale;
        }
    }
    if rest > 0 {
        hundreds(rest, &mut out);
    }
    out
}

/// "first", "twenty-third", "one hundredth".
pub fn ordinal(n: u64) -> Vec<String> {
    let mut words = cardinal(n);
    if let Some(last) = words.pop() {
        let (head, tail) = match last.rsplit_once('-') {
            Some((h, t)) => (format!("{h}-"), t.to_owned()),
            None => (String::new(), last),
        };
        let tail = match tail.as_str() {
            "one" => "first".into(),
            "two" => "second".into(),
            "three" => "third".into(),
            "five" => "fifth".into(),
            "eight" => "eighth".into(),
            "nine" => "ninth".into(),
            "twelve" => "twelfth".into(),
            t if t.ends_with('y') => format!("{}ieth", &t[..t.len() - 1]),
            t => format!("{t}th"),
        };
        words.push(format!("{head}{tail}"));
    }
    words
}

/// Four-digit years as people say them: "nineteen eighty-four", "two thousand five",
/// "twenty twenty-six".
pub fn year(n: u64) -> Vec<String> {
    let (hi, lo) = (n / 100, n % 100);
    if !(1000..10_000).contains(&n) || (hi % 10 == 0 && lo < 10) {
        return cardinal(n);
    }
    let mut out = Vec::new();
    tens(hi, &mut out);
    match lo {
        0 => out.push("hundred".into()),
        1..=9 => {
            out.push("oh".into());
            out.push(ONES[lo as usize].into());
        }
        _ => tens(lo, &mut out),
    }
    out
}

/// Digits one by one ("zero seven").
pub fn digits(text: &str) -> Vec<String> {
    text.chars()
        .filter_map(|c| c.to_digit(10))
        .map(|d| ONES[d as usize].to_owned())
        .collect()
}

/// A number as written ("1,234", "3.5", "2026", "21st"), as words. `None` if it isn't one.
pub fn words(token: &str) -> Option<Vec<String>> {
    let lower = token.to_ascii_lowercase();
    let (number, suffix) = split_suffix(&lower);
    let plain = number.replace(',', "");
    if plain.is_empty() || !plain.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    if let Some(kind) = suffix {
        let n: u64 = plain.parse().ok()?;
        return Some(match kind {
            "st" | "nd" | "rd" | "th" => ordinal(n),
            _ => cardinal(n),
        });
    }
    if let Some((whole, fraction)) = plain.split_once('.') {
        let mut out = if whole.is_empty() {
            Vec::new()
        } else {
            cardinal(whole.parse().ok()?)
        };
        out.push("point".into());
        out.extend(digits(fraction));
        return Some(out);
    }
    // A long run of digits (a code, a phone number) is read digit by digit.
    if plain.len() > 12 || (plain.len() > 1 && plain.starts_with('0')) {
        return Some(digits(&plain));
    }
    let n: u64 = plain.parse().ok()?;
    if plain.len() == 4 && !number.contains(',') {
        return Some(year(n));
    }
    Some(cardinal(n))
}

fn split_suffix(token: &str) -> (&str, Option<&str>) {
    for suffix in ["st", "nd", "rd", "th", "s"] {
        if let Some(stem) = token.strip_suffix(suffix)
            && stem.chars().last().is_some_and(|c| c.is_ascii_digit())
        {
            return (stem, Some(suffix));
        }
    }
    (token, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(token: &str) -> String {
        words(token).unwrap().join(" ")
    }

    #[test]
    fn numbers_are_spoken_the_way_people_say_them() {
        assert_eq!(said("0"), "zero");
        assert_eq!(said("7"), "seven");
        assert_eq!(said("42"), "forty-two");
        assert_eq!(said("105"), "one hundred five");
        assert_eq!(said("1,234"), "one thousand two hundred thirty-four");
        assert_eq!(said("2026"), "twenty twenty-six");
        assert_eq!(said("1984"), "nineteen eighty-four");
        assert_eq!(said("2005"), "two thousand five");
        assert_eq!(said("1900"), "nineteen hundred");
        assert_eq!(said("3.14"), "three point one four");
        assert_eq!(said("21st"), "twenty-first");
        assert_eq!(said("12th"), "twelfth");
        assert_eq!(said("40th"), "fortieth");
        assert_eq!(said("007"), "zero zero seven");
        assert_eq!(said("1000000"), "one million");
        assert!(words("abc").is_none());
    }
}
