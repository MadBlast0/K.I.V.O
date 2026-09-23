//! Letter-to-sound rules for words no lexicon knows (names, new words): the NRL rules (Elovitz et
//! al., 1976, a US Naval Research Laboratory report in the public domain), written here with
//! misaki's phoneme symbols so the result feeds Kokoro directly. A rule reads
//! `left [match] right = phonemes`; the first rule for a letter whose contexts fit wins.
//!
//! Context symbols: `#` one or more vowels, `:` zero or more consonants, `^` one consonant, `.` a
//! voiced consonant, `+` a front vowel (E, I, Y), `%` a suffix (ER, E, ES, ED, ING, ELY), `&` a
//! sibilant, `@` a consonant that makes U say /u/, and a space for the word's edge.

/// (left, match, right, phonemes), grouped by the match's first letter.
type Rule = (&'static str, &'static str, &'static str, &'static str);

#[rustfmt::skip]
const RULES: &[Rule] = &[
    // A
    (" ", "A", " ", "ə"), (" ", "ARE", " ", "ɑɹ"), (" ", "AR", "O", "əɹ"), ("", "AR", "#", "ɛɹ"),
    (" ^", "AS", "#", "As"), ("", "A", "WA", "ə"), ("", "AW", "", "ɔ"), (" :", "ANY", "", "ɛni"),
    ("", "A", "^+#", "A"), ("#:", "ALLY", "", "əli"), (" ", "AL", "#", "əl"),
    ("", "AGAIN", "", "əɡɛn"), ("#:", "AG", "E", "ɪʤ"), ("", "A", "^+:#", "æ"),
    (" :", "A", "^+ ", "A"), ("", "A", "^%", "A"), (" ", "ARR", "", "əɹ"), ("", "ARR", "", "æɹ"),
    (" :", "AR", " ", "ɑɹ"), ("", "AR", " ", "ɜɹ"), ("", "AR", "", "ɑɹ"), ("", "AIR", "", "ɛɹ"),
    ("", "AI", "", "A"), ("", "AY", "", "A"), ("", "AU", "", "ɔ"), ("#:", "AL", " ", "əl"),
    ("#:", "ALS", " ", "əlz"), ("", "ALK", "", "ɔk"), ("", "AL", "^", "ɔl"),
    (" :", "ABLE", "", "Abəl"), ("", "ABLE", "", "əbəl"), ("", "ANG", "+", "Anʤ"), ("", "A", "", "æ"),
    // B
    (" ", "BE", "^#", "bɪ"), ("", "BEING", "", "biɪŋ"), (" ", "BOTH", " ", "bOθ"),
    (" ", "BUS", "#", "bɪz"), ("", "BUIL", "", "bɪl"), ("", "B", "", "b"),
    // C
    (" ", "CH", "^", "k"), ("^E", "CH", "", "k"), ("", "CH", "", "ʧ"), (" S", "CI", "#", "sI"),
    ("", "CI", "A", "ʃ"), ("", "CI", "O", "ʃ"), ("", "CI", "EN", "ʃ"), ("", "C", "+", "s"),
    ("", "CK", "", "k"), ("", "COM", "%", "kʌm"), ("", "C", "", "k"),
    // D
    ("#:", "DED", " ", "dɪd"), (".E", "D", " ", "d"), ("#^:E", "D", " ", "t"), (" ", "DE", "^#", "dɪ"),
    (" ", "DO", " ", "du"), (" ", "DOES", "", "dʌz"), (" ", "DOING", "", "duɪŋ"), (" ", "DOW", "", "dW"),
    ("", "DU", "A", "ʤu"), ("", "D", "", "d"),
    // E
    ("#:", "E", " ", ""), ("'^:", "E", " ", ""), (" :", "E", " ", "i"), ("#", "ED", " ", "d"),
    ("#:", "E", "D ", ""), ("", "EV", "ER", "ɛv"), ("", "E", "^%", "i"), ("", "ERI", "#", "iɹi"),
    ("", "ERI", "", "ɛɹɪ"), ("#:", "ER", "#", "ɜɹ"), ("", "ER", "#", "ɛɹ"), ("", "ER", "", "ɜɹ"),
    (" ", "EVEN", "", "ivɛn"), ("#:", "E", "W", ""), ("@", "EW", "", "u"), ("", "EW", "", "ju"),
    ("", "E", "O", "i"), ("#:&", "ES", " ", "ɪz"), ("#:", "E", "S ", ""), ("#:", "ELY", " ", "li"),
    ("#:", "EMENT", "", "mɛnt"), ("", "EFUL", "", "fʊl"), ("", "EE", "", "i"), ("", "EARN", "", "ɜɹn"),
    (" ", "EAR", "^", "ɜɹ"), ("", "EAD", "", "ɛd"), ("#:", "EA", " ", "iə"), ("", "EA", "SU", "ɛ"),
    ("", "EA", "", "i"), ("", "EIGH", "", "A"), ("", "EI", "", "i"), (" ", "EYE", "", "I"),
    ("", "EY", "", "i"), ("", "EU", "", "ju"), ("", "E", "", "ɛ"),
    // F
    ("", "FUL", "", "fʊl"), ("", "F", "", "f"),
    // G
    ("", "GIV", "", "ɡɪv"), (" ", "G", "I^", "ɡ"), ("", "GE", "T", "ɡɛ"), ("SU", "GGES", "", "ɡʤɛs"),
    ("", "GG", "", "ɡ"), (" B#", "G", "", "ɡ"), ("", "G", "+", "ʤ"), ("", "GREAT", "", "ɡɹAt"),
    ("#", "GH", "", ""), ("", "G", "", "ɡ"),
    // H
    (" ", "HAV", "", "hæv"), (" ", "HERE", "", "hiɹ"), (" ", "HOUR", "", "Wɜɹ"), ("", "HOW", "", "hW"),
    ("", "H", "#", "h"), ("", "H", "", ""),
    // I
    (" ", "IN", "", "ɪn"), (" ", "I", " ", "I"), ("", "IN", "D", "In"), ("", "IER", "", "iɜɹ"),
    ("#:R", "IED", " ", "id"), ("", "IED", " ", "Id"), ("", "IEN", "", "iɛn"), ("", "IE", "T", "Iɛ"),
    (" :", "I", "%", "I"), ("", "I", "%", "i"), ("", "IE", "", "i"), ("", "I", "^+:#", "ɪ"),
    ("", "IR", "#", "Iɹ"), ("", "IZ", "%", "Iz"), ("", "IS", "%", "Iz"), ("", "I", "D%", "I"),
    ("+^", "I", "^+", "ɪ"), ("", "I", "T%", "I"), ("#:^", "I", "^+", "ɪ"), ("", "I", "^+", "I"),
    ("", "IR", "", "ɜɹ"), ("", "IGH", "", "I"), ("", "ILD", "", "Ild"), ("", "IGN", " ", "In"),
    ("", "IGN", "^", "In"), ("", "IGN", "%", "In"), ("", "IQUE", "", "ik"), ("", "I", "", "ɪ"),
    // J
    ("", "J", "", "ʤ"),
    // K
    (" ", "K", "N", ""), ("", "K", "", "k"),
    // L
    ("", "LO", "C#", "lO"), ("L", "L", "", ""), ("#^:", "L", "%", "əl"), ("", "LEAD", "", "lid"),
    ("", "L", "", "l"),
    // M
    ("", "MOV", "", "muv"), ("", "M", "", "m"),
    // N
    ("E", "NG", "+", "nʤ"), ("", "NG", "R", "ŋɡ"), ("", "NG", "#", "ŋɡ"), ("", "NGL", "%", "ŋɡəl"),
    ("", "NG", "", "ŋ"), ("", "NK", "", "ŋk"), (" ", "NOW", " ", "nW"), ("", "N", "", "n"),
    // O
    ("", "OF", " ", "əv"), ("", "OROUGH", "", "ɜɹO"), ("#:", "OR", " ", "ɜɹ"), ("#:", "ORS", " ", "ɜɹz"),
    ("", "OR", "", "ɔɹ"), (" ", "ONE", "", "wʌn"), ("", "OW", "", "O"), (" ", "OVER", "", "Ovɜɹ"),
    ("", "OV", "", "ʌv"), ("", "O", "^%", "O"), ("", "O", "^EN", "O"), ("", "O", "^I#", "O"),
    ("", "OL", "D", "Ol"), ("", "OUGHT", "", "ɔt"), ("", "OUGH", "", "ʌf"), (" ", "OU", "", "W"),
    ("H", "OU", "S#", "W"), ("", "OUS", "", "əs"), ("", "OUR", "", "ɔɹ"), ("", "OULD", "", "ʊd"),
    ("^", "OU", "^L", "ʌ"), ("", "OUP", "", "up"), ("", "OU", "", "W"), ("", "OY", "", "Y"),
    ("", "OING", "", "Oɪŋ"), ("", "OI", "", "Y"), ("", "OOR", "", "ɔɹ"), ("", "OOK", "", "ʊk"),
    ("", "OOD", "", "ʊd"), ("", "OO", "", "u"), ("", "O", "E", "O"), ("", "O", " ", "O"),
    ("", "OA", "", "O"), (" ", "ONLY", "", "Onli"), (" ", "ONCE", "", "wʌns"), ("", "ON'T", "", "Ont"),
    ("C", "O", "N", "ɑ"), ("", "O", "NG", "ɔ"), (" :^", "O", "N", "ʌ"), ("I", "ON", "", "ən"),
    ("#:", "ON", " ", "ən"), ("#^", "ON", "", "ən"), ("", "O", "ST ", "O"), ("", "OF", "^", "ɔf"),
    ("", "OTHER", "", "ʌðɜɹ"), ("", "OSS", " ", "ɔs"), ("#:^", "OM", "", "ʌm"), ("", "O", "", "ɑ"),
    // P
    ("", "PH", "", "f"), ("", "PEOP", "", "pip"), ("", "POW", "", "pW"), ("", "PUT", " ", "pʊt"),
    ("", "P", "", "p"),
    // Q
    ("", "QUAR", "", "kwɔɹ"), ("", "QU", "", "kw"), ("", "Q", "", "k"),
    // R
    (" ", "RE", "^#", "ɹi"), ("", "R", "", "ɹ"),
    // S
    ("", "SH", "", "ʃ"), ("#", "SION", "", "ʒən"), ("", "SOME", "", "sʌm"), ("#", "SUR", "#", "ʒɜɹ"),
    ("", "SUR", "#", "ʃɜɹ"), ("#", "SU", "#", "ʒu"), ("#", "SSU", "#", "ʃu"), ("#", "SED", " ", "zd"),
    ("#", "S", "#", "z"), ("", "SAID", "", "sɛd"), ("^", "SION", "", "ʃən"), ("", "S", "S", ""),
    (".", "S", " ", "z"), ("#:.E", "S", " ", "z"), ("#^:##", "S", " ", "z"), ("#^:#", "S", " ", "s"),
    ("U", "S", " ", "s"), (" :#", "S", " ", "z"), (" ", "SCH", "", "sk"), ("", "S", "C+", ""),
    ("#", "SM", "", "zm"), ("#", "SN", "'", "zən"), ("", "S", "", "s"),
    // T
    (" ", "THE", " ", "ðə"), ("", "TO", " ", "tu"), ("", "THAT", " ", "ðæt"), (" ", "THIS", " ", "ðɪs"),
    (" ", "THEY", "", "ðA"), (" ", "THERE", "", "ðɛɹ"), ("", "THER", "", "ðɜɹ"), ("", "THEIR", "", "ðɛɹ"),
    (" ", "THAN", " ", "ðæn"), (" ", "THEM", " ", "ðɛm"), ("", "THESE", " ", "ðiz"), (" ", "THEN", "", "ðɛn"),
    ("", "THROUGH", "", "θɹu"), ("", "THOSE", "", "ðOz"), ("", "THOUGH", " ", "ðO"), (" ", "THUS", "", "ðʌs"),
    ("", "TH", "", "θ"), ("#:", "TED", " ", "tɪd"), ("S", "TI", "#N", "ʧ"), ("", "TI", "O", "ʃ"),
    ("", "TI", "A", "ʃ"), ("", "TIEN", "", "ʃən"), ("", "TUR", "#", "ʧɜɹ"), ("", "TU", "A", "ʧu"),
    (" ", "TWO", "", "tu"), ("", "T", "", "t"),
    // U
    (" ", "UN", "I", "jun"), (" ", "UN", "", "ʌn"), (" ", "UPON", "", "əpɔn"), ("@", "UR", "#", "ʊɹ"),
    ("", "UR", "#", "jʊɹ"), ("", "UR", "", "ɜɹ"), ("", "U", "^ ", "ʌ"), ("", "U", "^^", "ʌ"),
    ("", "UY", "", "I"), (" G", "U", "#", ""), ("G", "U", "%", ""), ("G", "U", "#", "w"),
    ("#N", "U", "", "ju"), ("@", "U", "", "u"), ("", "U", "", "ju"),
    // V
    ("", "VIEW", "", "vju"), ("", "V", "", "v"),
    // W
    (" ", "WERE", "", "wɜɹ"), ("", "WA", "S", "wɑ"), ("", "WA", "T", "wɑ"), ("", "WHERE", "", "wɛɹ"),
    ("", "WHAT", "", "wɑt"), ("", "WHOL", "", "hOl"), ("", "WHO", "", "hu"), ("", "WH", "", "w"),
    ("", "WAR", "", "wɔɹ"), ("", "WOR", "^", "wɜɹ"), ("", "WR", "", "ɹ"), ("", "W", "", "w"),
    // X
    ("", "X", "", "ks"),
    // Y
    ("", "YOUNG", "", "jʌŋ"), (" ", "YOU", "", "ju"), (" ", "YES", "", "jɛs"), (" ", "Y", "", "j"),
    ("#:^", "Y", " ", "i"), ("#:^", "Y", "I", "i"), (" :", "Y", " ", "I"), (" :", "Y", "#", "I"),
    (" :", "Y", "^+:#", "ɪ"), (" :", "Y", "^#", "I"), ("", "Y", "", "ɪ"),
    // Z
    ("", "Z", "", "z"),
];

const VOWELS: &[u8] = b"AEIOUY";
const VOICED: &[u8] = b"BDVGJLMNRWZ";
const FRONT: &[u8] = b"EIY";
const SIBILANT: &[u8] = b"SCGZXJ";
const LONG_U: &[u8] = b"TSRDLZNJ";
/// The suffixes `%` stands for, longest first.
const SUFFIXES: [&[u8]; 6] = [b"ING", b"ELY", b"ER", b"ES", b"ED", b"E"];

fn is_vowel(c: u8) -> bool {
    VOWELS.contains(&c)
}
fn is_consonant(c: u8) -> bool {
    c.is_ascii_alphabetic() && !is_vowel(c)
}

/// Does `pattern` match `word` going right from `at`? Returns where the match ended.
fn right(pattern: &[u8], word: &[u8], mut at: usize) -> bool {
    let mut p = 0;
    while p < pattern.len() {
        let c = word.get(at).copied().unwrap_or(b' ');
        match pattern[p] {
            b'#' => {
                if !is_vowel(c) {
                    return false;
                }
                while word.get(at).copied().is_some_and(is_vowel) {
                    at += 1;
                }
            }
            b':' => {
                while word.get(at).copied().is_some_and(is_consonant) {
                    at += 1;
                }
            }
            b'^' => {
                if !is_consonant(c) {
                    return false;
                }
                at += 1;
            }
            b'.' => {
                if !VOICED.contains(&c) {
                    return false;
                }
                at += 1;
            }
            b'+' => {
                if !FRONT.contains(&c) {
                    return false;
                }
                at += 1;
            }
            b'%' => {
                // A suffix: ER, E, ES, ED, ING or ELY, then the word's end.
                let rest = &word[at.min(word.len())..];
                let end = |n: usize| rest.get(n).is_none_or(|&c| c == b' ');
                let found = SUFFIXES
                    .iter()
                    .find(|s| rest.starts_with(s) && end(s.len()));
                match found {
                    Some(s) => at += s.len(),
                    None => return false,
                }
            }
            literal => {
                if c != literal {
                    return false;
                }
                at += 1;
            }
        }
        p += 1;
    }
    true
}

/// Does `pattern` match `word` going left from just before `end`?
fn left(pattern: &[u8], word: &[u8], end: usize) -> bool {
    let mut at = end as isize - 1;
    let get = |i: isize| {
        if i < 0 {
            b' '
        } else {
            word.get(i as usize).copied().unwrap_or(b' ')
        }
    };
    for &p in pattern.iter().rev() {
        let c = get(at);
        match p {
            b'#' => {
                if !is_vowel(c) {
                    return false;
                }
                while at >= 0 && is_vowel(get(at)) {
                    at -= 1;
                }
            }
            b':' => {
                while at >= 0 && is_consonant(get(at)) {
                    at -= 1;
                }
            }
            b'^' => {
                if !is_consonant(c) {
                    return false;
                }
                at -= 1;
            }
            b'.' => {
                if !VOICED.contains(&c) {
                    return false;
                }
                at -= 1;
            }
            b'+' => {
                if !FRONT.contains(&c) {
                    return false;
                }
                at -= 1;
            }
            b'&' => {
                // A sibilant, including CH and SH.
                if c == b'H' && matches!(get(at - 1), b'C' | b'S') {
                    at -= 2;
                } else if SIBILANT.contains(&c) {
                    at -= 1;
                } else {
                    return false;
                }
            }
            b'@' => {
                if c == b'H' && matches!(get(at - 1), b'T' | b'C' | b'S') {
                    at -= 2;
                } else if LONG_U.contains(&c) {
                    at -= 1;
                } else {
                    return false;
                }
            }
            literal => {
                if c != literal {
                    return false;
                }
                at -= 1;
            }
        }
    }
    true
}

/// A word's phonemes by rule (upper- or lower-case letters and apostrophes), with primary stress
/// on the first vowel. Letters no rule covers are skipped.
pub fn phonemes(word: &str) -> String {
    let upper: Vec<u8> = word
        .bytes()
        .filter(|b| b.is_ascii_alphabetic() || *b == b'\'')
        .map(|b| b.to_ascii_uppercase())
        .collect();
    let mut out = String::new();
    let mut at = 0;
    while at < upper.len() {
        let rule = RULES.iter().find(|(l, m, r, _)| {
            let m = m.as_bytes();
            upper[at..].starts_with(m)
                && left(l.as_bytes(), &upper, at)
                && right(r.as_bytes(), &upper, at + m.len())
        });
        match rule {
            Some((_, m, _, ps)) => {
                out.push_str(ps);
                at += m.len();
            }
            None => at += 1,
        }
    }
    stress_first_vowel(&out)
}

/// Puts the primary stress mark before the first vowel.
fn stress_first_vowel(ps: &str) -> String {
    match ps.char_indices().find(|(_, c)| super::g2p::is_vowel(*c)) {
        Some((i, _)) => format!("{}ˈ{}", &ps[..i], &ps[i..]),
        None => ps.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_words_come_out_close_to_the_dictionary() {
        assert_eq!(phonemes("cat"), "kˈæt");
        assert_eq!(phonemes("ship"), "ʃˈɪp");
        assert_eq!(phonemes("make"), "mˈAk");
        assert_eq!(phonemes("night"), "nˈIt");
        assert_eq!(phonemes("phone"), "fˈOn");
        assert_eq!(phonemes("thing"), "θˈɪŋ");
    }

    #[test]
    fn names_no_dictionary_has_are_still_pronounceable() {
        for name in ["Kivo", "Spotify", "Zorblax", "Wexford"] {
            let ps = phonemes(name);
            assert!(ps.contains('ˈ'), "{name}: {ps}");
            assert!(ps.chars().count() >= 3, "{name}: {ps}");
        }
        assert_eq!(phonemes("kivo"), "kˈɪvO");
    }
}
