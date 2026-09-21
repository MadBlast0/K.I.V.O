//! Moonshine (MIT, English) on ONNX Runtime: the default "Ultra Fast" speech-to-text engine
//! (VOICE §3). It transcribes a whole utterance at a time; while the user is still speaking it
//! re-transcribes the audio so far every half second, which gives live partial transcripts.
//!
//! Model files (downloaded by the model manager, DIST-12): `encoder_model.ort`,
//! `decoder_model_merged.ort` and `tokens.txt` (base64 byte pieces, SentencePiece `▁` for spaces).

use crate::engine::{Accel, EngineInfo, EngineKind, EngineSlot, ResourceEstimate};
use crate::error::{VoiceError, VoiceResult};
use crate::traits::{SAMPLE_RATE, SttEngine, SttEvent, SttOptions, SttStream};
use base64::Engine as _;
use ort::session::Session;
use ort::value::Tensor;
use std::path::Path;
use tokio_util::sync::CancellationToken;

pub const MODEL_ID: &str = "moonshine-base-en";

const LAYERS: usize = 8;
const HEADS: usize = 8;
const HEAD_DIM: usize = 52;
const HIDDEN: usize = 416;
const START: i64 = 1;
const END: i64 = 2;
/// Moonshine produces at most about 6.5 tokens per second of speech.
const TOKENS_PER_SECOND: f32 = 6.5;
/// Re-transcribe for a partial after this much new audio.
const PARTIAL_EVERY: usize = SAMPLE_RATE as usize / 2;
/// Too little audio to be a word.
const MIN_AUDIO: usize = SAMPLE_RATE as usize * 3 / 10;

pub fn info() -> EngineInfo {
    EngineInfo {
        id: MODEL_ID.into(),
        name: "Moonshine Base".into(),
        slot: EngineSlot::Stt,
        kind: EngineKind::Local,
        license: "MIT".into(),
        languages: vec!["en".into()],
        streaming: true,
        accel: vec![Accel::Cpu],
        resources: ResourceEstimate {
            ram_mb: 350,
            vram_mb: 0,
            disk_mb: 135,
        },
        model: Some(MODEL_ID.into()),
    }
}

pub struct Moonshine {
    info: EngineInfo,
    encoder: Session,
    decoder: Session,
    pieces: Vec<Vec<u8>>,
}

impl Moonshine {
    pub fn load(dir: &Path, threads: usize) -> VoiceResult<Self> {
        let missing = |name: &str| !dir.join(name).is_file();
        if [
            "encoder_model.ort",
            "decoder_model_merged.ort",
            "tokens.txt",
        ]
        .iter()
        .any(|f| missing(f))
        {
            return Err(VoiceError::ModelMissing("speech recognition".into()));
        }
        let session = |file: &str| -> VoiceResult<Session> {
            Ok(Session::builder()?
                .with_intra_threads(threads)?
                .with_inter_threads(1)?
                .commit_from_file(dir.join(file))?)
        };
        let tokens = std::fs::read_to_string(dir.join("tokens.txt"))
            .map_err(|e| VoiceError::Engine(format!("tokens.txt: {e}")))?;
        Ok(Self {
            info: info(),
            encoder: session("encoder_model.ort")?,
            decoder: session("decoder_model_merged.ort")?,
            pieces: parse_tokens(&tokens)?,
        })
    }

    /// Transcribes a whole utterance.
    pub fn transcribe(&mut self, audio: &[f32], cancel: &CancellationToken) -> VoiceResult<String> {
        if audio.len() < MIN_AUDIO {
            return Ok(String::new());
        }
        let samples = audio.len();
        let outputs = self.encoder.run(ort::inputs![
            "input_values" => Tensor::from_array(([1usize, samples], audio.to_vec()))?,
            "attention_mask" => Tensor::from_array(([1usize, samples], vec![1_i64; samples]))?,
        ])?;
        let (shape, hidden) = outputs["last_hidden_state"].try_extract_tensor::<f32>()?;
        let frames = usize::try_from(shape[1]).unwrap_or(0);
        let hidden = hidden.to_vec();
        drop(outputs);

        #[allow(clippy::cast_precision_loss, reason = "audio length in seconds")]
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "small, positive"
        )]
        let max_tokens =
            ((samples as f32 / SAMPLE_RATE as f32) * TOKENS_PER_SECOND).ceil() as usize + 4;
        let empty = || vec![0.0_f32; 0];
        // Per layer: decoder key, decoder value (grow each step); encoder key, value (fixed).
        let mut self_kv: Vec<(Vec<f32>, Vec<f32>)> =
            (0..LAYERS).map(|_| (empty(), empty())).collect();
        let mut cross_kv: Vec<(Vec<f32>, Vec<f32>)> =
            (0..LAYERS).map(|_| (empty(), empty())).collect();
        let mut token = START;
        let mut text = Vec::new();

        // Each step adds one token to the decoder's own cache, so the cache holds `step` entries.
        for step in 0..max_tokens {
            let past_len = step;
            if cancel.is_cancelled() {
                return Err(VoiceError::Cancelled);
            }
            let first = step == 0;
            let mut inputs = ort::inputs![
                "input_ids" => Tensor::from_array(([1usize, 1], vec![token]))?,
                "encoder_hidden_states" => Tensor::from_array(([1usize, frames, HIDDEN], hidden.clone()))?,
                "encoder_attention_mask" => Tensor::from_array(([1usize, frames], vec![1_i64; frames]))?,
                "use_cache_branch" => Tensor::from_array(([1usize], vec![!first]))?,
            ];
            let cross_len = if first { 0 } else { frames };
            for (layer, ((dk, dv), (ek, ev))) in self_kv.iter().zip(&cross_kv).enumerate() {
                let kv = |data: &Vec<f32>, len: usize| {
                    Tensor::from_array(([1usize, HEADS, len, HEAD_DIM], data.clone()))
                };
                inputs.push((
                    format!("past_key_values.{layer}.decoder.key").into(),
                    kv(dk, past_len)?.into(),
                ));
                inputs.push((
                    format!("past_key_values.{layer}.decoder.value").into(),
                    kv(dv, past_len)?.into(),
                ));
                inputs.push((
                    format!("past_key_values.{layer}.encoder.key").into(),
                    kv(ek, cross_len)?.into(),
                ));
                inputs.push((
                    format!("past_key_values.{layer}.encoder.value").into(),
                    kv(ev, cross_len)?.into(),
                ));
            }
            let outputs = self.decoder.run(inputs)?;
            let (_, logits) = outputs["logits"].try_extract_tensor::<f32>()?;
            token = argmax(logits);
            for (layer, ((dk, dv), (ek, ev))) in
                self_kv.iter_mut().zip(cross_kv.iter_mut()).enumerate()
            {
                let take = |name: String| -> VoiceResult<Vec<f32>> {
                    Ok(outputs[name.as_str()]
                        .try_extract_tensor::<f32>()?
                        .1
                        .to_vec())
                };
                *dk = take(format!("present.{layer}.decoder.key"))?;
                *dv = take(format!("present.{layer}.decoder.value"))?;
                if first {
                    *ek = take(format!("present.{layer}.encoder.key"))?;
                    *ev = take(format!("present.{layer}.encoder.value"))?;
                }
            }
            if token == END {
                break;
            }
            text.push(token);
        }
        Ok(self.decode(&text))
    }

    fn decode(&self, ids: &[i64]) -> String {
        let mut bytes = Vec::new();
        for &id in ids {
            // 0–2 are <unk>, <s> and </s>.
            if let Some(piece) = usize::try_from(id)
                .ok()
                .filter(|&i| i > END as usize)
                .and_then(|i| self.pieces.get(i))
            {
                bytes.extend_from_slice(piece);
            }
        }
        String::from_utf8_lossy(&bytes)
            .replace('\u{2581}', " ")
            .trim()
            .to_owned()
    }
}

fn argmax(values: &[f32]) -> i64 {
    let (index, _) = values
        .iter()
        .enumerate()
        .fold((0, f32::NEG_INFINITY), |best, (i, &v)| {
            if v > best.1 { (i, v) } else { best }
        });
    i64::try_from(index).unwrap_or(END)
}

/// `tokens.txt`: one `<base64 bytes> <id>` per line.
fn parse_tokens(text: &str) -> VoiceResult<Vec<Vec<u8>>> {
    let mut pieces = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let (piece, id) = line
            .rsplit_once(' ')
            .ok_or_else(|| VoiceError::Engine(format!("bad token line: {line}")))?;
        let id: usize = id
            .trim()
            .parse()
            .map_err(|_| VoiceError::Engine(format!("bad token id: {line}")))?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(piece.trim())
            .map_err(|_| VoiceError::Engine(format!("bad token piece: {line}")))?;
        if pieces.len() <= id {
            pieces.resize(id + 1, Vec::new());
        }
        pieces[id] = bytes;
    }
    Ok(pieces)
}

impl SttEngine for Moonshine {
    fn info(&self) -> &EngineInfo {
        &self.info
    }

    fn start(
        &mut self,
        _options: &SttOptions,
        cancel: CancellationToken,
    ) -> Box<dyn SttStream + '_> {
        Box::new(Utterance {
            engine: self,
            cancel,
            audio: Vec::with_capacity(SAMPLE_RATE as usize * 10),
            transcribed_at: 0,
            stable: String::new(),
            last: String::new(),
        })
    }
}

/// One utterance: audio so far and what has been shown.
struct Utterance<'a> {
    engine: &'a mut Moonshine,
    cancel: CancellationToken,
    audio: Vec<f32>,
    transcribed_at: usize,
    stable: String,
    last: String,
}

impl SttStream for Utterance<'_> {
    fn accept(&mut self, audio: &[f32]) -> VoiceResult<Vec<SttEvent>> {
        self.audio.extend_from_slice(audio);
        if self.audio.len() < MIN_AUDIO || self.audio.len() - self.transcribed_at < PARTIAL_EVERY {
            return Ok(Vec::new());
        }
        self.transcribed_at = self.audio.len();
        let text = self.engine.transcribe(&self.audio, &self.cancel)?;
        let mut events = Vec::new();
        // Words two partials agree on (all but the last) won't change any more.
        let stable = common_words(&self.last, &text);
        if stable.len() > self.stable.len() {
            self.stable.clone_from(&stable);
            events.push(SttEvent::Stable(stable));
        }
        if !text.is_empty() && text != self.last {
            events.push(SttEvent::Partial(text.clone()));
        }
        self.last = text;
        Ok(events)
    }

    fn finish(&mut self) -> VoiceResult<String> {
        self.engine.transcribe(&self.audio, &self.cancel)
    }
}

/// The leading words `a` and `b` share, leaving out the last word of the shorter one (it may still
/// be growing).
fn common_words(a: &str, b: &str) -> String {
    let shared: Vec<&str> = a
        .split_whitespace()
        .zip(b.split_whitespace())
        .take_while(|(x, y)| x == y)
        .map(|(x, _)| x)
        .collect();
    let shorter = a
        .split_whitespace()
        .count()
        .min(b.split_whitespace().count());
    let keep = if shared.len() == shorter {
        shared.len().saturating_sub(1)
    } else {
        shared.len()
    };
    shared[..keep].join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_base64_pieces_with_ids() {
        let pieces =
            parse_tokens("PHVuaz4= 0\nPHM+ 1\nPC9zPg== 2\n4paBb3Blbg== 3\n4paBQ2hyb21l 4\n")
                .unwrap();
        assert_eq!(pieces[0], b"<unk>");
        assert_eq!(pieces[3], "\u{2581}open".as_bytes());
    }

    #[test]
    fn stable_words_leave_the_growing_last_word_out() {
        assert_eq!(common_words("open chr", "open chrome"), "open");
        assert_eq!(common_words("open chrome", "open chrome please"), "open");
        assert_eq!(
            common_words("open chrome and", "open chrome now"),
            "open chrome"
        );
        assert_eq!(common_words("", "open"), "");
        assert_eq!(common_words("mute", "unmute"), "");
    }

    #[test]
    fn argmax_picks_the_largest_logit() {
        assert_eq!(argmax(&[0.1, 3.0, -1.0, 2.9]), 1);
    }

    /// Runs the real model when it is present (`KIVO_MODELS` or the bench data folder).
    #[test]
    fn transcribes_the_sample_recording_when_the_model_is_installed() {
        let Some(dir) = std::env::var_os("KIVO_MOONSHINE_DIR").map(std::path::PathBuf::from) else {
            eprintln!("KIVO_MOONSHINE_DIR not set; skipping");
            return;
        };
        let mut engine = Moonshine::load(&dir, 2).unwrap();
        let wav = std::fs::read(dir.join("test_wavs/0.wav")).unwrap();
        // 16-bit PCM mono 16 kHz with a 44-byte header.
        let audio: Vec<f32> = wav[44..]
            .chunks_exact(2)
            .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0)
            .collect();
        let text = engine
            .transcribe(&audio, &CancellationToken::new())
            .unwrap();
        eprintln!("{text}");
        assert!(!text.is_empty());
    }
}
