//! The small local embedding model (BRAINS §2 stage 2, BRAIN-03): all-MiniLM-L6-v2
//! (sentence-transformers, Apache-2.0), quantized ONNX, 23 MB. A sentence becomes a 384-number
//! vector; sentences that mean the same land close together, so "kill the sound" finds the
//! exemplar "mute". KIVO's own BERT WordPiece tokenizer (lower case, accents stripped, punctuation
//! split, greedy longest-match pieces) feeds it, and the output is mean-pooled over the tokens
//! and L2-normalized, as the model was trained.

use crate::error::{VoiceError, VoiceResult};
use ort::session::Session;
use ort::value::Tensor;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

pub const MODEL_ID: &str = "minilm-l6-v2";
pub const DIMENSIONS: usize = 384;
/// Commands are short; longer text is cut.
const MAX_TOKENS: usize = 64;

/// BERT's WordPiece tokenizer for an uncased vocabulary.
pub struct WordPiece {
    vocab: HashMap<String, i64>,
    unk: i64,
    cls: i64,
    sep: i64,
}

impl WordPiece {
    pub fn from_vocab(text: &str) -> VoiceResult<Self> {
        let vocab: HashMap<String, i64> = text
            .lines()
            .enumerate()
            .map(|(i, t)| (t.to_owned(), i64::try_from(i).unwrap_or(0)))
            .collect();
        let id = |t: &str| {
            vocab
                .get(t)
                .copied()
                .ok_or_else(|| VoiceError::Engine(format!("the vocabulary has no {t}")))
        };
        Ok(Self {
            unk: id("[UNK]")?,
            cls: id("[CLS]")?,
            sep: id("[SEP]")?,
            vocab,
        })
    }

    /// Lower case, accents removed, punctuation as separate words.
    fn words(text: &str) -> Vec<String> {
        let mut words = Vec::new();
        let mut current = String::new();
        let flush = |current: &mut String, words: &mut Vec<String>| {
            if !current.is_empty() {
                words.push(std::mem::take(current));
            }
        };
        for c in text.chars().flat_map(char::to_lowercase) {
            let c = strip_accent(c);
            if c.is_whitespace() || c.is_control() {
                flush(&mut current, &mut words);
            } else if c.is_ascii_punctuation() || (!c.is_alphanumeric() && !c.is_whitespace()) {
                flush(&mut current, &mut words);
                words.push(c.to_string());
            } else {
                current.push(c);
            }
        }
        flush(&mut current, &mut words);
        words
    }

    /// Token ids with `[CLS]` and `[SEP]`, at most `MAX_TOKENS`.
    pub fn encode(&self, text: &str) -> Vec<i64> {
        let mut ids = vec![self.cls];
        'words: for word in Self::words(text) {
            let chars: Vec<char> = word.chars().collect();
            if chars.len() > 100 {
                ids.push(self.unk);
                continue;
            }
            let mut pieces = Vec::new();
            let mut start = 0;
            while start < chars.len() {
                let mut end = chars.len();
                let mut found = None;
                while start < end {
                    let piece: String = chars[start..end].iter().collect();
                    let piece = if start > 0 {
                        format!("##{piece}")
                    } else {
                        piece
                    };
                    if let Some(id) = self.vocab.get(&piece) {
                        found = Some(*id);
                        break;
                    }
                    end -= 1;
                }
                match found {
                    Some(id) => {
                        pieces.push(id);
                        start = end;
                    }
                    None => {
                        ids.push(self.unk);
                        continue 'words;
                    }
                }
            }
            ids.extend(pieces);
        }
        ids.truncate(MAX_TOKENS - 1);
        ids.push(self.sep);
        ids
    }
}

/// Common Latin accents folded to their base letter (BERT's `strip_accents`).
fn strip_accent(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        other => other,
    }
}

/// The sentence-embedding model.
pub struct MiniLm {
    tokenizer: WordPiece,
    session: Mutex<Session>,
}

impl MiniLm {
    /// Loads `model.onnx` and `vocab.txt` from the model's folder.
    pub fn load(dir: &Path) -> VoiceResult<Self> {
        let model = dir.join("model.onnx");
        let vocab = dir.join("vocab.txt");
        if !model.is_file() || !vocab.is_file() {
            return Err(VoiceError::ModelMissing("sentence embeddings".into()));
        }
        let text = std::fs::read_to_string(vocab).map_err(|e| VoiceError::Engine(e.to_string()))?;
        let session = Session::builder()?
            .with_intra_threads(1)?
            .with_inter_threads(1)?
            .commit_from_file(model)?;
        Ok(Self {
            tokenizer: WordPiece::from_vocab(&text)?,
            session: Mutex::new(session),
        })
    }

    /// A unit-length vector for `text`.
    pub fn embed(&self, text: &str) -> VoiceResult<Vec<f32>> {
        let ids = self.tokenizer.encode(text);
        let n = ids.len();
        let mask = vec![1i64; n];
        let types = vec![0i64; n];
        let mut session = self
            .session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let outputs = session.run(ort::inputs![
            "input_ids" => Tensor::from_array(([1usize, n], ids))?,
            "attention_mask" => Tensor::from_array(([1usize, n], mask))?,
            "token_type_ids" => Tensor::from_array(([1usize, n], types))?,
        ])?;
        let (_, hidden) = outputs[0].try_extract_tensor::<f32>()?;
        if hidden.len() < n * DIMENSIONS {
            return Err(VoiceError::Engine("unexpected embedding shape".into()));
        }
        // Mean over the tokens (all of them count: no padding), then unit length.
        let mut out = vec![0.0f32; DIMENSIONS];
        for t in 0..n {
            for (o, v) in out
                .iter_mut()
                .zip(&hidden[t * DIMENSIONS..(t + 1) * DIMENSIONS])
            {
                *o += v;
            }
        }
        #[allow(clippy::cast_precision_loss, reason = "a token count")]
        let count = n as f32;
        for o in &mut out {
            *o /= count;
        }
        let norm = out.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-12);
        for o in &mut out {
            *o /= norm;
        }
        Ok(out)
    }
}

/// Cosine similarity of two unit vectors.
pub fn similarity(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    const VOCAB: &str =
        "[PAD]\n[UNK]\n[CLS]\n[SEP]\nturn\nthe\nsound\noff\n##s\nplay\n,\nre\n##play\ncafe\n";

    #[test]
    fn wordpiece_splits_words_and_punctuation() {
        let t = WordPiece::from_vocab(VOCAB).unwrap();
        // [CLS] turn the sound off , [SEP]
        assert_eq!(t.encode("Turn the SOUND off,"), [2, 4, 5, 6, 7, 10, 3]);
        // "sounds" = sound + ##s; "replay" = re + ##play; unknown words are [UNK].
        assert_eq!(t.encode("sounds replay zebra"), [2, 6, 8, 11, 12, 1, 3]);
        assert_eq!(t.encode("Café"), [2, 13, 3], "accents stripped");
    }

    /// The real model, where a test machine has it (`KIVO_MINILM_DIR`).
    #[test]
    fn paraphrases_are_close_and_other_requests_are_not() {
        let Some(dir) = std::env::var_os("KIVO_MINILM_DIR") else {
            eprintln!("KIVO_MINILM_DIR isn't set; skipping");
            return;
        };
        let m = MiniLm::load(Path::new(&dir)).unwrap();
        // The same ids as BERT's reference tokenizer (bert-base-uncased vocabulary).
        assert_eq!(
            m.tokenizer.encode("Hello, world! Unaffable"),
            [101, 7592, 1010, 2088, 999, 14477, 20961, 3468, 102]
        );
        let mute = m.embed("mute").unwrap();
        assert!((mute.iter().map(|v| v * v).sum::<f32>() - 1.0).abs() < 1e-4);
        let paraphrase = m.embed("turn the sound off").unwrap();
        let other = m.embed("what's the weather in paris").unwrap();
        let close = similarity(&m.embed("mute the sound").unwrap(), &paraphrase);
        let far = similarity(&mute, &other);
        eprintln!("close {close}, far {far}");
        assert!(close > 0.7, "{close}");
        assert!(far < 0.3, "{far}");
    }
}
