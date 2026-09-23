//! SentencePiece encoding, enough to turn a wake phrase into the keyword spotter's tokens
//! (VOICE §4). It reads the model's SentencePiece protobuf (`bpe.model`, whatever its name says)
//! for the pieces, their scores and the model type, and encodes as SentencePiece does: each word
//! gets a `▁` prefix; a **Unigram** model takes the segmentation with the best total score
//! (Viterbi), a **BPE** model merges the best-scoring adjacent pair until nothing merges.

use crate::error::{VoiceError, VoiceResult};
use std::collections::HashMap;
use std::path::Path;

/// SentencePiece's word-boundary mark.
pub const SPACE: char = '▁';

/// Piece types in the model file.
const NORMAL: u64 = 1;
const USER_DEFINED: u64 = 4;
/// `TrainerSpec.model_type`.
const UNIGRAM: u64 = 1;
const BPE: u64 = 2;
/// SentencePiece's penalty for a character no piece covers, below the worst piece.
const UNKNOWN_PENALTY: f32 = 10.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Unigram,
    Bpe,
}

pub struct SentencePiece {
    kind: Kind,
    /// Usable pieces and their scores.
    scores: HashMap<String, f32>,
    longest: usize,
    unknown_score: f32,
}

impl SentencePiece {
    pub fn load(path: &Path) -> VoiceResult<Self> {
        let data = std::fs::read(path)
            .map_err(|e| VoiceError::Engine(format!("{}: {e}", path.display())))?;
        Self::parse(&data)
    }

    /// Reads a `ModelProto`: field 1 is each `SentencePiece` (piece = 1, score = 2, type = 3),
    /// field 2 the `TrainerSpec` (model_type = 3).
    pub fn parse(data: &[u8]) -> VoiceResult<Self> {
        let bad = || VoiceError::Engine("the keyword model's vocabulary is damaged".into());
        let mut scores = HashMap::new();
        let mut model_type = UNIGRAM;
        let mut top = Reader::new(data);
        while let Some((field, value)) = top.next().ok_or_else(bad)? {
            match (field, value) {
                (1, Value::Bytes(piece)) => {
                    let (mut text, mut score, mut kind) = (None, 0.0_f32, NORMAL);
                    let mut inner = Reader::new(piece);
                    while let Some((field, value)) = inner.next().ok_or_else(bad)? {
                        match (field, value) {
                            (1, Value::Bytes(b)) => {
                                text = Some(String::from_utf8_lossy(b).into_owned());
                            }
                            (2, Value::Fixed32(bits)) => score = f32::from_bits(bits),
                            (3, Value::Varint(v)) => kind = v,
                            _ => {}
                        }
                    }
                    if let Some(text) = text
                        && (kind == NORMAL || kind == USER_DEFINED)
                    {
                        scores.insert(text, score);
                    }
                }
                (2, Value::Bytes(spec)) => {
                    let mut inner = Reader::new(spec);
                    while let Some((field, value)) = inner.next().ok_or_else(bad)? {
                        if let (3, Value::Varint(v)) = (field, value) {
                            model_type = v;
                        }
                    }
                }
                _ => {}
            }
        }
        if scores.is_empty() {
            return Err(bad());
        }
        let kind = match model_type {
            BPE => Kind::Bpe,
            _ => Kind::Unigram,
        };
        let longest = scores.keys().map(|p| p.chars().count()).max().unwrap_or(1);
        let worst = scores.values().copied().fold(f32::INFINITY, f32::min);
        Ok(Self {
            kind,
            scores,
            longest,
            unknown_score: worst - UNKNOWN_PENALTY,
        })
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// The pieces for `text` (already in the model's case). A character no piece covers comes
    /// out on its own, and won't map to a token.
    pub fn encode(&self, text: &str) -> Vec<String> {
        let mut chars: Vec<char> = Vec::new();
        for word in text.split_whitespace() {
            chars.push(SPACE);
            chars.extend(word.chars());
        }
        match self.kind {
            Kind::Unigram => self.viterbi(&chars),
            Kind::Bpe => self.merge(&chars),
        }
    }

    fn viterbi(&self, chars: &[char]) -> Vec<String> {
        let n = chars.len();
        // best[i]: the best score of chars[..i], and where its last piece starts.
        let mut best: Vec<(f32, usize)> = vec![(f32::NEG_INFINITY, 0); n + 1];
        best[0].0 = 0.0;
        for end in 1..=n {
            for start in end.saturating_sub(self.longest)..end {
                if best[start].0 == f32::NEG_INFINITY {
                    continue;
                }
                let piece: String = chars[start..end].iter().collect();
                let score = match self.scores.get(&piece) {
                    Some(&s) => s,
                    None if end - start == 1 => self.unknown_score,
                    None => continue,
                };
                let total = best[start].0 + score;
                if total > best[end].0 {
                    best[end] = (total, start);
                }
            }
        }
        let mut pieces = Vec::new();
        let mut end = n;
        while end > 0 {
            let start = best[end].1;
            pieces.push(chars[start..end].iter().collect());
            end = start;
        }
        pieces.reverse();
        pieces
    }

    fn merge(&self, chars: &[char]) -> Vec<String> {
        let mut symbols: Vec<String> = chars.iter().map(|c| c.to_string()).collect();
        loop {
            let mut best: Option<(usize, f32)> = None;
            for i in 0..symbols.len().saturating_sub(1) {
                let merged = format!("{}{}", symbols[i], symbols[i + 1]);
                if let Some(&score) = self.scores.get(&merged)
                    && best.is_none_or(|(_, s)| score > s)
                {
                    best = Some((i, score));
                }
            }
            let Some((i, _)) = best else { break };
            let right = symbols.remove(i + 1);
            symbols[i].push_str(&right);
        }
        symbols
    }
}

enum Value<'a> {
    Varint(u64),
    Fixed32(u32),
    Bytes(&'a [u8]),
    Other,
}

/// A minimal protobuf reader: field number and value, skipping what it doesn't need.
struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, at: 0 }
    }

    fn varint(&mut self) -> Option<u64> {
        let mut value = 0_u64;
        for shift in (0..64).step_by(7) {
            let byte = *self.data.get(self.at)?;
            self.at += 1;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(n)?;
        let bytes = self.data.get(self.at..end)?;
        self.at = end;
        Some(bytes)
    }

    /// `Some(None)` at the end; `None` if the data is damaged.
    fn next(&mut self) -> Option<Option<(u64, Value<'a>)>> {
        if self.at >= self.data.len() {
            return Some(None);
        }
        let key = self.varint()?;
        let value = match key & 7 {
            0 => Value::Varint(self.varint()?),
            1 => {
                self.take(8)?;
                Value::Other
            }
            2 => {
                let len = usize::try_from(self.varint()?).ok()?;
                Value::Bytes(self.take(len)?)
            }
            5 => {
                let b = self.take(4)?;
                Value::Fixed32(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            }
            _ => return None,
        };
        Some(Some((key >> 3, value)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a tiny model file: pieces with scores, and the model type.
    fn model(pieces: &[(&str, f32)], model_type: u8) -> Vec<u8> {
        let mut out = Vec::new();
        for (text, score) in pieces {
            let mut piece = vec![0x0a, u8::try_from(text.len()).unwrap()];
            piece.extend_from_slice(text.as_bytes());
            piece.push(0x15);
            piece.extend_from_slice(&score.to_le_bytes());
            piece.extend_from_slice(&[0x18, 1]);
            out.push(0x0a);
            out.push(u8::try_from(piece.len()).unwrap());
            out.extend(piece);
        }
        // TrainerSpec { model_type }
        out.extend_from_slice(&[0x12, 2, 0x18, model_type]);
        out
    }

    const PIECES: [(&str, f32); 9] = [
        ("▁", -1.0),
        ("H", -2.0),
        ("E", -2.0),
        ("Y", -2.0),
        ("HE", -3.0),
        ("▁HE", -4.0),
        ("EY", -5.0),
        ("▁HEY", -5.5),
        ("▁HEYY", -30.0),
    ];

    #[test]
    fn unigram_takes_the_best_scoring_segmentation() {
        let sp = SentencePiece::parse(&model(&PIECES, 1)).unwrap();
        assert_eq!(sp.kind(), Kind::Unigram);
        // One piece ▁HEY (−5.5) beats ▁HE + Y (−6) and ▁ + H + EY (−8).
        assert_eq!(sp.encode("HEY"), ["▁HEY"]);
        // ▁HEY + Y (−7.5) beats the one rare piece ▁HEYY (−30).
        assert_eq!(sp.encode("HEYY"), ["▁HEY", "Y"]);
        // An unknown character stands alone.
        assert_eq!(sp.encode("HEZ"), ["▁HE", "Z"]);
    }

    #[test]
    fn bpe_merges_the_best_scoring_pairs_first() {
        let sp = SentencePiece::parse(&model(&PIECES, 2)).unwrap();
        assert_eq!(sp.kind(), Kind::Bpe);
        assert_eq!(sp.encode("HEY HE"), ["▁HEY", "▁HE"]);
        assert_eq!(sp.encode("YE"), ["▁", "Y", "E"]);
    }

    #[test]
    fn damaged_files_are_refused() {
        assert!(SentencePiece::parse(&[0x0a, 0x50, 1, 2]).is_err());
        assert!(SentencePiece::parse(&[]).is_err());
    }
}
