//! Open-vocabulary keyword spotting (VOICE §4): "Hey Kivo", custom wake words and the command
//! spotter ("Kivo stop"). The model is sherpa-onnx's English Zipformer keyword spotter
//! (Apache-2.0, a download the user chooses); KIVO runs it on ONNX Runtime with a port of
//! sherpa-onnx's keyword decoder, because sherpa's own library bundles GPL code (DECISIONS
//! "No training"). A keyword is typed text: it is split into the model's SentencePiece pieces, and a
//! modified beam search over the streaming encoder's output, boosted towards the keywords' token
//! paths, reports a keyword when its tokens' average probability passes its threshold.

pub mod graph;
pub mod pieces;

use crate::engine::{Accel, EngineInfo, EngineKind, EngineSlot, ResourceEstimate};
use crate::error::{VoiceError, VoiceResult};
use crate::fbank::{BINS, OnlineFbank};
use graph::{ContextGraph, Entry, ROOT, StateId};
use ort::session::{Session, SessionInputValue};
use ort::value::Tensor;
use pieces::SentencePiece;
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;

pub const ENGINE_ID: &str = "kws-zipformer-gigaspeech-en";

/// sherpa-onnx's defaults.
const MAX_ACTIVE_PATHS: usize = 4;
const NUM_TRAILING_BLANKS: usize = 1;
pub const DEFAULT_BOOST: f32 = 1.0;
pub const DEFAULT_THRESHOLD: f32 = 0.25;
/// Encoder frames are 40 ms (10 ms features, subsampled by 4).
pub const ENCODER_FRAME_MS: usize = 40;
/// After this much trailing silence the decoder starts afresh (sherpa: 1.5 s).
const RESET_AFTER_BLANKS: usize = 1_500 / ENCODER_FRAME_MS;

pub fn info() -> EngineInfo {
    EngineInfo {
        id: ENGINE_ID.into(),
        name: "Keyword spotter (English)".into(),
        slot: EngineSlot::Wake,
        kind: EngineKind::Local,
        license: "Apache-2.0".into(),
        languages: vec!["en".into()],
        streaming: true,
        accel: vec![Accel::Cpu],
        resources: ResourceEstimate {
            ram_mb: 30,
            vram_mb: 0,
            disk_mb: 6,
        },
        model: Some(ENGINE_ID.into()),
    }
}

/// An encoder input carried from one chunk to the next.
struct StateSpec {
    name: String,
    shape: Vec<usize>,
}

/// The loaded model: encoder, decoder, joiner, tokens and vocabulary.
pub struct KwsModel {
    encoder: Session,
    decoder: Session,
    joiner: Session,
    states: Vec<StateSpec>,
    /// Feature frames per encoder call, and how far each call moves on.
    chunk: usize,
    shift: usize,
    context: usize,
    vocab: usize,
    ids: HashMap<String, i64>,
    unk: i64,
    pieces: SentencePiece,
}

fn session(path: &Path) -> VoiceResult<Session> {
    if !path.is_file() {
        return Err(VoiceError::ModelMissing("wake-word".into()));
    }
    // One thread: the spotter listens all the time and must stay cheap (VOICE §10).
    Ok(Session::builder()?
        .with_intra_threads(1)?
        .with_inter_threads(1)?
        .commit_from_file(path)?)
}

fn meta(session: &Session, key: &str) -> VoiceResult<usize> {
    session
        .metadata()?
        .custom(key)
        .and_then(|v| v.trim().parse().ok())
        .ok_or_else(|| VoiceError::Engine(format!("the keyword model has no {key}")))
}

impl KwsModel {
    /// Loads `encoder.onnx`, `decoder.onnx`, `joiner.onnx`, `tokens.txt` and `bpe.model`.
    pub fn load(dir: &Path) -> VoiceResult<Self> {
        let encoder = session(&dir.join("encoder.onnx"))?;
        let decoder = session(&dir.join("decoder.onnx"))?;
        let joiner = session(&dir.join("joiner.onnx"))?;
        let chunk = meta(&encoder, "T")?;
        let shift = meta(&encoder, "decode_chunk_len")?;
        let context = meta(&decoder, "context_size")?;
        let vocab = meta(&decoder, "vocab_size")?;
        let states = encoder
            .inputs()
            .iter()
            .filter(|i| i.name() != "x" && i.name() != "processed_lens")
            .map(|i| StateSpec {
                name: i.name().to_owned(),
                // The batch dimension is dynamic (−1): one stream.
                shape: i
                    .dtype()
                    .tensor_shape()
                    .map(|s| s.iter().map(|&d| usize::try_from(d).unwrap_or(1)).collect())
                    .unwrap_or_default(),
            })
            .collect();
        let tokens = std::fs::read_to_string(dir.join("tokens.txt"))
            .map_err(|_| VoiceError::ModelMissing("wake-word".into()))?;
        let ids: HashMap<String, i64> = tokens
            .lines()
            .filter_map(|line| {
                let (piece, id) = line.rsplit_once(' ')?;
                Some((piece.to_owned(), id.parse().ok()?))
            })
            .collect();
        let unk = ids.get("<unk>").copied().unwrap_or(2);
        let pieces = SentencePiece::load(&dir.join("bpe.model"))?;
        Ok(Self {
            encoder,
            decoder,
            joiner,
            states,
            chunk,
            shift,
            context,
            vocab,
            ids,
            unk,
            pieces,
        })
    }

    /// The model's tokens for a phrase, or `None` if it has a sound the model can't spell.
    pub fn tokens(&self, phrase: &str) -> Option<Vec<i64>> {
        let text = phrase
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '\'' {
                    c
                } else {
                    ' '
                }
            })
            .collect::<String>()
            .to_uppercase();
        let pieces = self.pieces.encode(&text);
        if pieces.is_empty() {
            return None;
        }
        pieces.iter().map(|p| self.ids.get(p).copied()).collect()
    }

    /// The pieces a phrase becomes (for the wake-word check and the log).
    pub fn pieces(&self, phrase: &str) -> Vec<String> {
        self.pieces.encode(&phrase.to_uppercase())
    }
}

/// A keyword to listen for.
#[derive(Clone, Debug)]
pub struct Keyword {
    /// The caller's id, returned with a detection.
    pub id: String,
    pub phrase: String,
    pub boost: f32,
    pub threshold: f32,
}

impl Keyword {
    pub fn new(id: &str, phrase: &str) -> Self {
        Self {
            id: id.into(),
            phrase: phrase.into(),
            boost: DEFAULT_BOOST,
            threshold: DEFAULT_THRESHOLD,
        }
    }

    /// Sensitivity 0–1 (the Settings slider): higher hears the word more easily and risks more
    /// false alarms. The threshold runs from 0.45 down to 0.10 (0.275 at 0.5, near sherpa's
    /// 0.25); the boost from 1.0 up to 2.5. The boost keeps the keyword's path in the 4-path beam
    /// long enough to finish: measured on Windows' voice saying "Hey Kivo, mute.", sherpa's
    /// default boost of 1.0 lost the word, 1.5 found it at 0.36 and 2.0 at 0.40.
    pub fn with_sensitivity(mut self, sensitivity: f32) -> Self {
        let s = sensitivity.clamp(0.0, 1.0);
        self.threshold = 0.45 - 0.35 * s;
        self.boost = 1.0 + 1.5 * s;
        self
    }
}

/// A keyword heard.
#[derive(Clone, Debug, PartialEq)]
pub struct Detection {
    pub id: String,
    /// The average probability of its tokens.
    pub score: f32,
    /// Where it started and ended, in ms since the spotter was last reset.
    pub start_ms: usize,
    pub end_ms: usize,
}

#[derive(Clone)]
struct Hyp {
    ys: Vec<i64>,
    log_prob: f32,
    probs: Vec<f32>,
    times: Vec<usize>,
    trailing: usize,
    state: StateId,
}

/// A listening stream over a model and a set of keywords.
pub struct KeywordSpotter {
    model: KwsModel,
    graph: ContextGraph,
    keywords: Vec<Keyword>,
    fbank: OnlineFbank,
    feats: Vec<[f32; BINS]>,
    /// Feature frames already fed to the encoder (in total since the reset).
    processed: usize,
    /// Frames dropped from the front of `feats`.
    dropped: usize,
    states: Vec<Vec<f32>>,
    hyps: Vec<Hyp>,
    /// Encoder frames decoded since the reset.
    frame_offset: usize,
}

impl KeywordSpotter {
    /// Keywords the model can't spell are left out and returned.
    pub fn new(model: KwsModel, keywords: Vec<Keyword>) -> (Self, Vec<Keyword>) {
        let mut kept = Vec::new();
        let mut entries = Vec::new();
        let mut refused = Vec::new();
        for k in keywords {
            match model.tokens(&k.phrase) {
                Some(tokens) => {
                    entries.push(Entry {
                        tokens,
                        boost: k.boost,
                        threshold: k.threshold,
                    });
                    kept.push(k);
                }
                None => refused.push(k),
            }
        }
        let mut spotter = Self {
            graph: ContextGraph::new(&entries),
            keywords: kept,
            fbank: OnlineFbank::new(),
            feats: Vec::new(),
            processed: 0,
            dropped: 0,
            states: Vec::new(),
            hyps: Vec::new(),
            frame_offset: 0,
            model,
        };
        spotter.reset();
        (spotter, refused)
    }

    /// Changes the keywords, keeping the model.
    pub fn set_keywords(self, keywords: Vec<Keyword>) -> (Self, Vec<Keyword>) {
        Self::new(self.model, keywords)
    }

    pub fn keywords(&self) -> &[Keyword] {
        &self.keywords
    }

    fn blank_hyp(&self) -> Hyp {
        let mut ys = vec![-1; self.model.context];
        if let Some(last) = ys.last_mut() {
            *last = 0;
        }
        Hyp {
            ys,
            log_prob: 0.0,
            probs: Vec::new(),
            times: Vec::new(),
            trailing: 0,
            state: ROOT,
        }
    }

    /// Forgets all audio: the next sound starts a new stream.
    pub fn reset(&mut self) {
        self.fbank.reset();
        self.feats.clear();
        self.processed = 0;
        self.dropped = 0;
        self.frame_offset = 0;
        self.reset_decoder();
    }

    fn reset_decoder(&mut self) {
        self.states = self
            .model
            .states
            .iter()
            .map(|s| vec![0.0; s.shape.iter().product()])
            .collect();
        self.hyps = vec![self.blank_hyp()];
    }

    /// Adds 16 kHz mono audio; returns the keywords it completes.
    pub fn accept(&mut self, samples: &[f32]) -> VoiceResult<Vec<Detection>> {
        self.fbank.accept(samples);
        self.feats.extend(self.fbank.frames());
        let mut found = Vec::new();
        while self.processed + self.model.chunk <= self.dropped + self.feats.len() {
            let from = self.processed - self.dropped;
            let chunk: Vec<f32> = self.feats[from..from + self.model.chunk]
                .iter()
                .flatten()
                .copied()
                .collect();
            let encoded = self.encode(chunk)?;
            self.processed += self.model.shift;
            found.extend(self.decode(&encoded)?);
            // Long silence: start the decoder afresh, as sherpa does.
            let best = self
                .hyps
                .iter()
                .max_by(|a, b| a.log_prob.total_cmp(&b.log_prob));
            if best.is_some_and(|h| h.trailing > RESET_AFTER_BLANKS) {
                self.reset_decoder();
            }
        }
        let used = self.processed - self.dropped;
        if used > 0 {
            self.feats.drain(..used.min(self.feats.len()));
            self.dropped += used;
        }
        Ok(found)
    }

    /// One encoder call: 45 feature frames in, encoder frames (320 numbers each) out.
    fn encode(&mut self, chunk: Vec<f32>) -> VoiceResult<Vec<Vec<f32>>> {
        let mut inputs: Vec<(Cow<'_, str>, SessionInputValue<'_>)> = vec![
            (
                "x".into(),
                Tensor::from_array(([1usize, self.model.chunk, BINS], chunk))?.into(),
            ),
            (
                "processed_lens".into(),
                Tensor::from_array(([1usize], vec![self.processed as i64]))?.into(),
            ),
        ];
        for (spec, data) in self.model.states.iter().zip(&self.states) {
            inputs.push((
                spec.name.clone().into(),
                Tensor::from_array((spec.shape.clone(), data.clone()))?.into(),
            ));
        }
        let outputs = self.model.encoder.run(inputs)?;
        for (spec, data) in self.model.states.iter().zip(self.states.iter_mut()) {
            let (_, next) =
                outputs[format!("new_{}", spec.name).as_str()].try_extract_tensor::<f32>()?;
            data.clear();
            data.extend_from_slice(next);
        }
        let (shape, out) = outputs["encoder_out"].try_extract_tensor::<f32>()?;
        let dim = usize::try_from(*shape.last().unwrap_or(&1))
            .unwrap_or(1)
            .max(1);
        Ok(out.chunks(dim).map(<[f32]>::to_vec).collect())
    }

    fn decode(&mut self, frames: &[Vec<f32>]) -> VoiceResult<Vec<Detection>> {
        let vocab = self.model.vocab;
        let context = self.model.context;
        let mut found = Vec::new();
        for (t, frame) in frames.iter().enumerate() {
            let n = self.hyps.len();
            let ys: Vec<i64> = self
                .hyps
                .iter()
                .flat_map(|h| h.ys[h.ys.len() - context..].to_vec())
                .collect();
            let decoder_out = {
                let outputs = self
                    .model
                    .decoder
                    .run(ort::inputs!["y" => Tensor::from_array(([n, context], ys))?])?;
                outputs["decoder_out"]
                    .try_extract_tensor::<f32>()?
                    .1
                    .to_vec()
            };
            let dim = decoder_out.len() / n;
            let encoder_rep: Vec<f32> = (0..n).flat_map(|_| frame.iter().copied()).collect();
            let mut logits = {
                let outputs = self.model.joiner.run(ort::inputs![
                    "encoder_out" => Tensor::from_array(([n, frame.len()], encoder_rep))?,
                    "decoder_out" => Tensor::from_array(([n, dim], decoder_out))?,
                ])?;
                outputs["logit"].try_extract_tensor::<f32>()?.1.to_vec()
            };
            for row in logits.chunks_mut(vocab) {
                log_softmax(row);
            }
            let acoustic = logits.clone();
            for (row, hyp) in logits.chunks_mut(vocab).zip(&self.hyps) {
                row.iter_mut().for_each(|v| *v += hyp.log_prob);
            }
            let mut order: Vec<usize> = (0..logits.len()).collect();
            let k = MAX_ACTIVE_PATHS.min(order.len());
            order.select_nth_unstable_by(k.saturating_sub(1), |&a, &b| {
                logits[b].total_cmp(&logits[a])
            });
            order.truncate(k);

            let mut next: Vec<Hyp> = Vec::with_capacity(k);
            for index in order {
                let (from, token) = (index / vocab, index % vocab);
                let token = token as i64;
                let mut hyp = self.hyps[from].clone();
                let mut context_score = 0.0;
                if token != 0 && token != self.model.unk {
                    hyp.ys.push(token);
                    hyp.times.push(t + self.frame_offset);
                    hyp.probs.push(acoustic[index].exp());
                    hyp.trailing = 0;
                    let (score, state) = self.graph.step(hyp.state, token);
                    context_score = score;
                    hyp.state = state;
                    if state == ROOT {
                        // Off every keyword: forget the history, match from the start.
                        hyp.ys = self.blank_hyp().ys;
                        hyp.times.clear();
                        hyp.probs.clear();
                    }
                } else {
                    hyp.trailing += 1;
                }
                hyp.log_prob = logits[index] + context_score;
                add(&mut next, hyp);
            }
            let best = next
                .iter()
                .max_by(|a, b| a.log_prob.total_cmp(&b.log_prob))
                .cloned();
            self.hyps = next;
            let Some(best) = best else { continue };
            let Some(end) = self.graph.matched(best.state) else {
                continue;
            };
            let state = &self.graph.states[end];
            let level = state.level.min(best.probs.len());
            if level == 0 {
                continue;
            }
            let score = best.probs[..level].iter().sum::<f32>() / level as f32;
            if best.trailing > NUM_TRAILING_BLANKS
                && score >= state.threshold
                && let Some(keyword) = state.keyword.and_then(|i| self.keywords.get(i))
            {
                let times = &best.times[best.times.len() - level..];
                found.push(Detection {
                    id: keyword.id.clone(),
                    score,
                    start_ms: times.first().copied().unwrap_or(0) * ENCODER_FRAME_MS,
                    end_ms: (times.last().copied().unwrap_or(0) + 1) * ENCODER_FRAME_MS,
                });
                self.hyps = vec![self.blank_hyp()];
            }
        }
        self.frame_offset += frames.len();
        Ok(found)
    }
}

/// Adds a hypothesis, merging it with one that has the same tokens (their probabilities add).
fn add(hyps: &mut Vec<Hyp>, hyp: Hyp) {
    if let Some(same) = hyps.iter_mut().find(|h| h.ys == hyp.ys) {
        let (a, b) = (same.log_prob, hyp.log_prob);
        let high = a.max(b);
        same.log_prob = high + ((a - high).exp() + (b - high).exp()).ln();
    } else {
        hyps.push(hyp);
    }
}

fn log_softmax(row: &mut [f32]) {
    let max = row.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let sum: f32 = row.iter().map(|v| (v - max).exp()).sum();
    let log_sum = max + sum.ln();
    row.iter_mut().for_each(|v| *v -= log_sum);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The installed model, or the unpacked release archive (`KIVO_KWS_DIR`), which also has the
    /// test recordings and sherpa's own keyword encodings.
    fn model_dir() -> Option<std::path::PathBuf> {
        [
            std::env::var_os("KIVO_KWS_DIR").map(std::path::PathBuf::from),
            std::env::var_os("LOCALAPPDATA").map(|d| {
                std::path::PathBuf::from(d)
                    .join("KIVO")
                    .join("models")
                    .join(ENGINE_ID)
            }),
        ]
        .into_iter()
        .flatten()
        .find(|d| d.join("encoder.onnx").is_file() || d.join("bpe.model").is_file())
    }

    fn wav(path: &Path) -> Vec<f32> {
        let data = std::fs::read(path).unwrap();
        data[44..]
            .chunks_exact(2)
            .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32_768.0)
            .collect()
    }

    #[test]
    fn phrases_become_the_same_pieces_as_sherpas_tool() {
        let Some(dir) = model_dir().filter(|d| d.join("keywords_raw.txt").is_file()) else {
            eprintln!("the keyword model archive isn't unpacked (KIVO_KWS_DIR); skipping");
            return;
        };
        let sp = SentencePiece::load(&dir.join("bpe.model")).unwrap();
        let raw = std::fs::read_to_string(dir.join("keywords_raw.txt")).unwrap();
        let encoded = std::fs::read_to_string(dir.join("keywords.txt")).unwrap();
        for (phrase, pieces) in raw.lines().zip(encoded.lines()) {
            assert_eq!(sp.encode(phrase).join(" "), pieces, "{phrase}");
        }
    }

    #[test]
    fn spots_keywords_in_the_test_recordings() {
        let Some(dir) = model_dir().filter(|d| d.join("test_wavs").is_dir()) else {
            eprintln!("the keyword model archive isn't unpacked (KIVO_KWS_DIR); skipping");
            return;
        };
        let model = KwsModel::load(&dir).unwrap();
        let keywords = vec![
            Keyword::new("light", "light up"),
            Keyword::new("child", "lovely child"),
            Keyword::new("forever", "forever"),
        ];
        let (mut spotter, refused) = KeywordSpotter::new(model, keywords);
        assert!(refused.is_empty());
        let mut heard = Vec::new();
        for name in ["0.wav", "1.wav"] {
            spotter.reset();
            let audio = wav(&dir.join("test_wavs").join(name));
            for chunk in audio.chunks(1_280) {
                heard.extend(spotter.accept(chunk).unwrap());
            }
            // A little silence lets a keyword at the very end finish.
            heard.extend(spotter.accept(&[0.0; 16_000]).unwrap());
        }
        let ids: Vec<&str> = heard.iter().map(|d| d.id.as_str()).collect();
        eprintln!("{heard:?}");
        // sherpa-onnx reports LIGHT UP in 0.wav and LOVELY CHILD and FOREVER in 1.wav.
        assert_eq!(ids, ["light", "child", "forever"]);
    }
}
