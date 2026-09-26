//! Chatterbox Turbo on ONNX Runtime (VOICE §3, VOICE-11): the Expressive voice, MIT-licensed by
//! Resemble AI, with paralinguistic tags (`[laugh]`, `[chuckle]`, `[sigh]`). It runs the
//! publisher's own ONNX export (q4 weights):
//!
//! 1. the speech encoder turns a reference recording (24 kHz) into a conditioning prefix, the
//!    reference's own speech tokens and the speaker's embedding and features;
//! 2. the text is tokenized (GPT-2 byte-level BPE plus the tags) and embedded;
//! 3. a 24-layer GPT-2 language model generates speech tokens after the start token, greedily with
//!    a repetition penalty of 1.2, until the stop token;
//! 4. the conditional decoder turns the reference's tokens, the new ones and three silence tokens
//!    into 24 kHz audio in one step.
//!
//! Chatterbox copies the voice it's given. KIVO gives it a voice made on this PC: one of the
//! Windows voices reading a reference sentence (the caller supplies it), so nothing is downloaded
//! for the voice and no one's recording is used.
//!
//! Model files: `tokenizer.json` and `onnx/{speech_encoder,embed_tokens,language_model,
//! conditional_decoder}_q4.onnx` with their `.onnx_data`.

use crate::bpe::Bpe;
use crate::engine::{Accel, EngineInfo, EngineKind, EngineSlot, ResourceEstimate};
use crate::error::{VoiceError, VoiceResult};
use crate::traits::{AudioSink, TtsEngine, VoiceInfo};
use ort::session::Session;
use ort::value::Tensor;
use std::collections::HashMap;
use std::path::Path;
use tokio_util::sync::CancellationToken;

pub const ENGINE_ID: &str = "chatterbox-turbo";
pub const SAMPLE_RATE: u32 = 24_000;

const START_SPEECH: i64 = 6561;
const STOP_SPEECH: i64 = 6562;
const SILENCE: i64 = 4299;
const END_OF_TEXT: i64 = 50256;
const REPETITION_PENALTY: f32 = 1.2;
/// Speech tokens come at 25 a second; a sentence never needs more than this.
const MAX_TOKENS: usize = 1_000;

pub fn info() -> EngineInfo {
    EngineInfo {
        id: ENGINE_ID.into(),
        name: "Chatterbox Turbo".into(),
        slot: EngineSlot::Tts,
        kind: EngineKind::Local,
        license: "MIT".into(),
        languages: vec!["en".into()],
        streaming: true,
        accel: vec![Accel::Cpu],
        resources: ResourceEstimate {
            ram_mb: 1_500,
            vram_mb: 0,
            disk_mb: 720,
        },
        model: Some(ENGINE_ID.into()),
    }
}

/// Makes the reference recording for a voice: `(samples, rate)` for a voice id.
pub type Reference = Box<dyn FnMut(&str) -> VoiceResult<(Vec<f32>, u32)> + Send>;

/// What the speech encoder made of one reference voice.
#[derive(Clone)]
struct Conditioning {
    cond_emb: (Vec<usize>, Vec<f32>),
    prompt_tokens: Vec<i64>,
    speaker_embeddings: (Vec<usize>, Vec<f32>),
    speaker_features: (Vec<usize>, Vec<f32>),
}

pub struct Chatterbox {
    info: EngineInfo,
    tokenizer: Bpe,
    speech_encoder: Session,
    embed_tokens: Session,
    language_model: Session,
    decoder: Session,
    reference: Reference,
    voices: Vec<VoiceInfo>,
    /// Encoded voices, by voice id.
    conditioned: HashMap<String, Conditioning>,
}

fn dims(shape: &[i64]) -> Vec<usize> {
    shape
        .iter()
        .map(|&d| usize::try_from(d).unwrap_or(0))
        .collect()
}

/// `audio` at `from` Hz, linearly resampled to `to` Hz.
pub fn resample(audio: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || audio.is_empty() {
        return audio.to_vec();
    }
    let ratio = f64::from(from) / f64::from(to);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "sample positions"
    )]
    let n = (audio.len() as f64 / ratio) as usize;
    (0..n)
        .map(|i| {
            #[allow(clippy::cast_precision_loss, reason = "sample positions")]
            let x = i as f64 * ratio;
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "floor of a position"
            )]
            let j = x as usize;
            #[allow(clippy::cast_possible_truncation, reason = "a fraction")]
            let t = (x - j as f64) as f32;
            let a = audio[j.min(audio.len() - 1)];
            let b = audio[(j + 1).min(audio.len() - 1)];
            a + (b - a) * t
        })
        .collect()
}

impl Chatterbox {
    /// Loads the model from `dir`. `voices` are the voices it can copy; `reference` makes the
    /// recording for one of them.
    pub fn load(
        dir: &Path,
        threads: usize,
        voices: Vec<VoiceInfo>,
        reference: Reference,
    ) -> VoiceResult<Self> {
        let onnx = dir.join("onnx");
        let names = [
            "speech_encoder",
            "embed_tokens",
            "language_model",
            "conditional_decoder",
        ];
        if !dir.join("tokenizer.json").is_file()
            || names
                .iter()
                .any(|n| !onnx.join(format!("{n}_q4.onnx")).is_file())
        {
            return Err(VoiceError::ModelMissing(ENGINE_ID.into()));
        }
        let session =
            |name: &str| crate::onnx::session(&onnx.join(format!("{name}_q4.onnx")), threads);
        let tokenizer = Bpe::from_json(
            &std::fs::read_to_string(dir.join("tokenizer.json"))
                .map_err(|e| VoiceError::Engine(format!("tokenizer.json: {e}")))?,
        )?;
        Ok(Self {
            info: info(),
            tokenizer,
            speech_encoder: session("speech_encoder")?,
            embed_tokens: session("embed_tokens")?,
            language_model: session("language_model")?,
            decoder: session("conditional_decoder")?,
            reference,
            voices,
            conditioned: HashMap::new(),
        })
    }

    /// Encodes `audio` (24 kHz) as the voice to copy.
    fn condition(&mut self, audio: Vec<f32>) -> VoiceResult<Conditioning> {
        let n = audio.len();
        let outputs = self
            .speech_encoder
            .run(ort::inputs!["audio_values" => Tensor::from_array(([1usize, n], audio))?])?;
        let take = |i: usize| -> VoiceResult<(Vec<usize>, Vec<f32>)> {
            let (shape, data) = outputs[i].try_extract_tensor::<f32>()?;
            Ok((dims(shape), data.to_vec()))
        };
        let cond_emb = take(0)?;
        let prompt_tokens = outputs[1].try_extract_tensor::<i64>()?.1.to_vec();
        let speaker_embeddings = take(2)?;
        let speaker_features = take(3)?;
        Ok(Conditioning {
            cond_emb,
            prompt_tokens,
            speaker_embeddings,
            speaker_features,
        })
    }

    fn conditioning(&mut self, voice: &str) -> VoiceResult<Conditioning> {
        if let Some(c) = self.conditioned.get(voice) {
            return Ok(c.clone());
        }
        let (audio, rate) = (self.reference)(voice)?;
        let c = self.condition(resample(&audio, rate, SAMPLE_RATE))?;
        self.conditioned.insert(voice.to_owned(), c.clone());
        Ok(c)
    }

    /// Embeddings for token ids, `[1, n, width]`.
    fn embed(&mut self, ids: Vec<i64>) -> VoiceResult<(Vec<usize>, Vec<f32>)> {
        let n = ids.len();
        let outputs = self
            .embed_tokens
            .run(ort::inputs!["input_ids" => Tensor::from_array(([1usize, n], ids))?])?;
        let (shape, data) = outputs[0].try_extract_tensor::<f32>()?;
        Ok((dims(shape), data.to_vec()))
    }

    /// One sentence as 24 kHz audio in the voice `c`.
    fn synthesize(
        &mut self,
        text: &str,
        c: &Conditioning,
        cancel: &CancellationToken,
    ) -> VoiceResult<Vec<f32>> {
        let mut ids = self.tokenizer.encode(text);
        ids.extend([END_OF_TEXT, END_OF_TEXT]);
        let (text_shape, text_emb) = self.embed(ids)?;
        // The conditioning prefix, then the text.
        let width = text_shape[2];
        let mut embeds = c.cond_emb.1.clone();
        embeds.extend_from_slice(&text_emb);
        // (The start token only seeds the repetition penalty, as in the publisher's reference.)
        let seq = c.cond_emb.0[1] + text_shape[1];

        let past_names: Vec<String> = self
            .language_model
            .inputs()
            .iter()
            .map(|i| i.name().to_owned())
            .filter(|n| n.starts_with("past_key_values"))
            .collect();
        let head_shape = self
            .language_model
            .inputs()
            .iter()
            .find(|i| i.name().starts_with("past_key_values"))
            .and_then(|i| i.dtype().tensor_shape().map(|s| s.to_vec()))
            .ok_or_else(|| VoiceError::Engine("the speech model has no cache inputs".into()))?;
        let heads = usize::try_from(head_shape[1]).unwrap_or(16);
        let head_dim = usize::try_from(head_shape[3]).unwrap_or(64);
        let mut past: Vec<(Vec<usize>, Vec<f32>)> = past_names
            .iter()
            .map(|_| (vec![1, heads, 0, head_dim], Vec::new()))
            .collect();
        let mut generated: Vec<i64> = vec![START_SPEECH];
        let mut position = 0usize;
        let mut input = embeds;
        let mut input_len = seq;
        for _ in 0..MAX_TOKENS {
            if cancel.is_cancelled() {
                return Err(VoiceError::Cancelled);
            }
            let total = position + input_len;
            let mask = vec![1i64; total];
            let positions: Vec<i64> = (position..total)
                .map(|p| i64::try_from(p).unwrap_or(0))
                .collect();
            let mut inputs = ort::inputs![
                "inputs_embeds" => Tensor::from_array(([1usize, input_len, width], input))?,
                "attention_mask" => Tensor::from_array(([1usize, total], mask))?,
                "position_ids" => Tensor::from_array(([1usize, input_len], positions))?,
            ];
            for (name, (shape, data)) in past_names.iter().zip(past.drain(..)) {
                inputs.push((
                    name.clone().into(),
                    Tensor::from_array((shape, data))?.into(),
                ));
            }
            let outputs = self.language_model.run(inputs)?;
            let (shape, logits) = outputs[0].try_extract_tensor::<f32>()?;
            let vocab = usize::try_from(shape[2]).unwrap_or(0);
            let steps = usize::try_from(shape[1]).unwrap_or(1);
            let mut last = logits[(steps - 1) * vocab..steps * vocab].to_vec();
            for &t in &generated {
                if let Some(score) = usize::try_from(t).ok().and_then(|t| last.get_mut(t)) {
                    *score = if *score < 0.0 {
                        *score * REPETITION_PENALTY
                    } else {
                        *score / REPETITION_PENALTY
                    };
                }
            }
            let next = last
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .map_or(STOP_SPEECH, |(i, _)| {
                    i64::try_from(i).unwrap_or(STOP_SPEECH)
                });
            for i in 1..outputs.len() {
                let (shape, data) = outputs[i].try_extract_tensor::<f32>()?;
                past.push((dims(shape), data.to_vec()));
            }
            drop(outputs);
            generated.push(next);
            if next == STOP_SPEECH {
                break;
            }
            position = total;
            let (_, emb) = self.embed(vec![next])?;
            input = emb;
            input_len = 1;
        }
        // The new speech tokens, after the reference's and before three silences.
        let new: Vec<i64> = generated
            .iter()
            .copied()
            .skip(1)
            .take_while(|&t| t != STOP_SPEECH)
            .collect();
        let mut tokens = c.prompt_tokens.clone();
        tokens.extend(new);
        tokens.extend([SILENCE; 3]);
        let n = tokens.len();
        let outputs = self.decoder.run(ort::inputs![
            "speech_tokens" => Tensor::from_array(([1usize, n], tokens))?,
            "speaker_embeddings" => Tensor::from_array((c.speaker_embeddings.0.clone(), c.speaker_embeddings.1.clone()))?,
            "speaker_features" => Tensor::from_array((c.speaker_features.0.clone(), c.speaker_features.1.clone()))?,
        ])?;
        Ok(outputs[0].try_extract_tensor::<f32>()?.1.to_vec())
    }
}

/// `text` split into sentences, so the first one plays while the next is made.
fn sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        current.push(c);
        if matches!(c, '.' | '!' | '?' | '\n') && current.trim().len() > 1 {
            out.push(current.trim().to_owned());
            current.clear();
        }
    }
    if !current.trim().is_empty() {
        out.push(current.trim().to_owned());
    }
    out
}

impl TtsEngine for Chatterbox {
    fn info(&self) -> &EngineInfo {
        &self.info
    }

    fn voices(&self) -> Vec<VoiceInfo> {
        self.voices.clone()
    }

    fn speak(
        &mut self,
        text: &str,
        voice: Option<&str>,
        cancel: &CancellationToken,
        sink: AudioSink<'_>,
    ) -> VoiceResult<()> {
        let voice = voice
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
            .or_else(|| self.voices.first().map(|v| v.id.clone()))
            .unwrap_or_default();
        let c = self.conditioning(&voice)?;
        for sentence in sentences(text) {
            let audio = self.synthesize(&sentence, &c, cancel)?;
            sink(&audio, SAMPLE_RATE)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampling_keeps_the_duration() {
        let one_second: Vec<f32> = (0..16_000).map(|i| (i as f32 / 100.0).sin()).collect();
        assert_eq!(resample(&one_second, 16_000, 24_000).len(), 24_000);
        assert_eq!(resample(&one_second, 16_000, 16_000).len(), 16_000);
    }

    #[test]
    fn text_is_spoken_a_sentence_at_a_time() {
        assert_eq!(
            sentences("Hello there. How are you? Fine [laugh]"),
            ["Hello there.", "How are you?", "Fine [laugh]"]
        );
    }

    /// Round trip, where the models are here: Chatterbox says a sentence in a voice copied from a
    /// sample recording, and Parakeet hears it back.
    #[test]
    fn says_a_sentence_that_a_recognizer_hears_back() {
        let (Some(dir), Some(asr), Some(whisper)) = (
            std::env::var_os("KIVO_CHATTERBOX_DIR").map(std::path::PathBuf::from),
            std::env::var_os("KIVO_PARAKEET_DIR").map(std::path::PathBuf::from),
            std::env::var_os("KIVO_WHISPER_DIR").map(std::path::PathBuf::from),
        ) else {
            eprintln!(
                "KIVO_CHATTERBOX_DIR, KIVO_PARAKEET_DIR or KIVO_WHISPER_DIR isn't set; skipping"
            );
            return;
        };
        let wav = std::fs::read(whisper.join("test_wavs/0.wav")).unwrap();
        let sample = crate::utterance::wav_samples(&wav);
        let voices = vec![VoiceInfo {
            id: "sample".into(),
            name: "Sample".into(),
            language: "en".into(),
        }];
        let mut engine = Chatterbox::load(
            &dir,
            4,
            voices,
            Box::new(move |_| Ok((sample.clone(), 16_000))),
        )
        .unwrap();
        let mut audio = Vec::new();
        engine
            .speak(
                "The weather is lovely today.",
                None,
                &CancellationToken::new(),
                &mut |pcm, rate| {
                    assert_eq!(rate, SAMPLE_RATE);
                    audio.extend_from_slice(pcm);
                    Ok(())
                },
            )
            .unwrap();
        assert!(
            audio.len() > SAMPLE_RATE as usize,
            "at least a second of speech"
        );
        let mut parakeet = crate::parakeet::Parakeet::load(&asr, 4).unwrap();
        let heard = parakeet
            .transcribe(
                &resample(&audio, SAMPLE_RATE, 16_000),
                &CancellationToken::new(),
            )
            .unwrap()
            .to_lowercase();
        println!("heard: {heard}");
        assert!(
            heard.contains("weather") && heard.contains("today"),
            "{heard}"
        );
    }
}
