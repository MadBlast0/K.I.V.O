//! GPT-2's byte-level BPE, read from a Hugging Face `tokenizer.json` (Chatterbox's text side,
//! VOICE-11): added tokens (`[chuckle]`, `<|endoftext|>`) split out first, then GPT-2's
//! pre-tokenizer, the byte-to-character mapping, and merges by rank.

use crate::error::{VoiceError, VoiceResult};
use serde_json::Value;
use std::collections::HashMap;

pub struct Bpe {
    vocab: HashMap<String, i64>,
    ranks: HashMap<(String, String), usize>,
    /// Added tokens by their text, longest first so none hides a longer one.
    added: Vec<(String, i64)>,
    byte_char: [char; 256],
}

/// GPT-2's reversible byte → printable character map.
fn bytes_to_unicode() -> [char; 256] {
    let mut map = ['\0'; 256];
    let printable: Vec<u32> = (u32::from(b'!')..=u32::from(b'~'))
        .chain(0xA1..=0xAC)
        .chain(0xAE..=0xFF)
        .collect();
    let mut next = 256;
    for b in 0..256u32 {
        let c = if printable.contains(&b) {
            b
        } else {
            let c = next;
            next += 1;
            c
        };
        map[b as usize] = char::from_u32(c).unwrap_or('?');
    }
    map
}

impl Bpe {
    pub fn from_json(text: &str) -> VoiceResult<Self> {
        let value: Value = serde_json::from_str(text)
            .map_err(|e| VoiceError::Engine(format!("tokenizer.json: {e}")))?;
        let model = &value["model"];
        let vocab: HashMap<String, i64> = model["vocab"]
            .as_object()
            .ok_or_else(|| VoiceError::Engine("tokenizer.json has no vocabulary".into()))?
            .iter()
            .filter_map(|(k, v)| Some((k.clone(), v.as_i64()?)))
            .collect();
        let ranks = model["merges"]
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .enumerate()
            .filter_map(|(rank, m)| {
                let (a, b) = match m {
                    Value::String(s) => s
                        .split_once(' ')
                        .map(|(a, b)| (a.to_owned(), b.to_owned()))?,
                    Value::Array(p) => (
                        p.first()?.as_str()?.to_owned(),
                        p.get(1)?.as_str()?.to_owned(),
                    ),
                    _ => return None,
                };
                Some(((a, b), rank))
            })
            .collect();
        let mut added: Vec<(String, i64)> = value["added_tokens"]
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .filter_map(|t| Some((t["content"].as_str()?.to_owned(), t["id"].as_i64()?)))
            .collect();
        added.sort_by_key(|(t, _)| std::cmp::Reverse(t.len()));
        Ok(Self {
            vocab,
            ranks,
            added,
            byte_char: bytes_to_unicode(),
        })
    }

    /// Token ids for `text` (no end-of-text added).
    pub fn encode(&self, text: &str) -> Vec<i64> {
        let mut out = Vec::new();
        let mut rest = text;
        while !rest.is_empty() {
            // The earliest added token, if any.
            let found = self
                .added
                .iter()
                .filter_map(|(t, id)| rest.find(t.as_str()).map(|at| (at, t, *id)))
                .min_by_key(|(at, t, _)| (*at, std::cmp::Reverse(t.len())));
            match found {
                Some((at, token, id)) => {
                    self.encode_plain(&rest[..at], &mut out);
                    out.push(id);
                    rest = &rest[at + token.len()..];
                }
                None => {
                    self.encode_plain(rest, &mut out);
                    break;
                }
            }
        }
        out
    }

    fn encode_plain(&self, text: &str, out: &mut Vec<i64>) {
        for word in pretokenize(text) {
            let mapped: String = word.bytes().map(|b| self.byte_char[b as usize]).collect();
            for piece in self.merge(&mapped) {
                if let Some(&id) = self.vocab.get(&piece) {
                    out.push(id);
                }
            }
        }
    }

    /// Applies merges to one pre-token, lowest rank first.
    fn merge(&self, word: &str) -> Vec<String> {
        let mut parts: Vec<String> = word.chars().map(String::from).collect();
        loop {
            let best = parts
                .windows(2)
                .enumerate()
                .filter_map(|(i, w)| {
                    self.ranks
                        .get(&(w[0].clone(), w[1].clone()))
                        .map(|&r| (r, i))
                })
                .min();
            let Some((_, i)) = best else { break };
            let merged = format!("{}{}", parts[i], parts[i + 1]);
            parts.splice(i..=i + 1, [merged]);
        }
        parts
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Letter,
    Number,
    Space,
    Other,
}

fn class(c: char) -> Class {
    if c.is_alphabetic() {
        Class::Letter
    } else if c.is_numeric() {
        Class::Number
    } else if c.is_whitespace() {
        Class::Space
    } else {
        Class::Other
    }
}

/// GPT-2's pre-tokenizer: `'s|'t|'re|'ve|'m|'ll|'d| ?\p{L}+| ?\p{N}+| ?[^\s\p{L}\p{N}]+|\s+(?!\S)|\s+`.
pub fn pretokenize(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        // Contractions.
        if chars[i] == '\'' {
            let rest: String = chars[i + 1..].iter().take(2).collect();
            let len = ["re", "ve", "ll"]
                .iter()
                .find(|s| rest.starts_with(**s))
                .map(|_| 3)
                .or_else(|| {
                    ["s", "t", "m", "d"]
                        .iter()
                        .find(|s| rest.starts_with(**s))
                        .map(|_| 2)
                });
            if let Some(len) = len {
                out.push(chars[i..i + len].iter().collect());
                i += len;
                continue;
            }
        }
        let c = chars[i];
        let start = i;
        // An optional leading space before a letter, number or other run.
        let (lead, body) = if c == ' ' && i + 1 < chars.len() && class(chars[i + 1]) != Class::Space
        {
            (1, class(chars[i + 1]))
        } else {
            (0, class(c))
        };
        if body == Class::Space {
            // Whitespace: all of it, except the last one when a non-space follows.
            let mut j = i;
            while j < chars.len() && class(chars[j]) == Class::Space {
                j += 1;
            }
            let end = if j < chars.len() && j - i > 1 {
                j - 1
            } else {
                j
            };
            out.push(chars[start..end].iter().collect());
            i = end;
            continue;
        }
        let mut j = i + lead;
        while j < chars.len() && class(chars[j]) == body {
            j += 1;
        }
        out.push(chars[start..j].iter().collect());
        i = j;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pretokenizes_like_gpt2() {
        assert_eq!(
            pretokenize("Hello world, it's 42!  Ok"),
            ["Hello", " world", ",", " it", "'s", " 42", "!", " ", " Ok"]
        );
        assert_eq!(pretokenize("a  b"), ["a", " ", " b"]);
        assert_eq!(pretokenize("end   "), ["end", "   "]);
    }

    #[test]
    fn merges_by_rank_and_splits_added_tokens() {
        let json = r#"{
            "model": {"type":"BPE","vocab":{"H":0,"i":1,"Hi":2,"Ġ":3,"t":4,"h":5,"e":6,"th":7,"the":8,"Ġthe":9},
                      "merges":["H i","t h","th e","Ġ the"]},
            "added_tokens":[{"id":50,"content":"[laugh]"},{"id":51,"content":"<|endoftext|>"}]
        }"#;
        let bpe = Bpe::from_json(json).unwrap();
        assert_eq!(bpe.encode("Hi the"), [2, 9]);
        assert_eq!(bpe.encode("Hi[laugh] the<|endoftext|>"), [2, 50, 9, 51]);
    }

    #[test]
    fn byte_map_is_gpt2s() {
        let map = bytes_to_unicode();
        assert_eq!(map[b' ' as usize], 'Ġ');
        assert_eq!(map[b'A' as usize], 'A');
        assert_eq!(map[b'\n' as usize], 'Ċ');
    }
}
