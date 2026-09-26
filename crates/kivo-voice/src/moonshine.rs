//! Moonshine on ONNX Runtime: the default speech-to-text engine (VOICE §3), in several sizes and
//! languages (VOICE §11): Base and Tiny for English (MIT), and Base or Tiny models for other
//! languages (the non-commercial Moonshine Community License, shown before download). Their
//! dimensions are read from the model, so every variant runs on the same code. It transcribes a whole utterance at a time; while the user is still speaking it
//! re-transcribes the audio so far every half second, which gives live partial transcripts.
//!
//! Model files (downloaded by the model manager, DIST-12): `encoder_model.ort`,
//! `decoder_model_merged.ort` and `tokens.txt` (base64 byte pieces, SentencePiece `▁` for spaces).

use crate::engine::{Accel, EngineInfo, EngineKind, EngineSlot, ResourceEstimate};
use crate::error::{VoiceError, VoiceResult};
use crate::traits::{SAMPLE_RATE, SttEngine, SttOptions, SttStream};
use crate::utterance::{Transcriber, Utterance};
use base64::Engine as _;
use ort::session::Session;
use ort::value::Tensor;
use std::path::Path;
use tokio_util::sync::CancellationToken;

pub const MODEL_ID: &str = "moonshine-base-en";

/// A Moonshine model KIVO can use.
#[derive(Clone, Copy, Debug)]
pub struct Variant {
    pub id: &'static str,
    pub name: &'static str,
    pub language: &'static str,
    pub tiny: bool,
    /// English models are MIT; the others use the non-commercial Moonshine Community License.
    pub commercial: bool,
    pub disk_mb: u32,
}

/// Every Moonshine model in the catalog.
pub const VARIANTS: [Variant; 10] = [
    Variant {
        id: MODEL_ID,
        name: "Moonshine Base",
        language: "en",
        tiny: false,
        commercial: true,
        disk_mb: 135,
    },
    Variant {
        id: "moonshine-tiny-en",
        name: "Moonshine Tiny",
        language: "en",
        tiny: true,
        commercial: true,
        disk_mb: 42,
    },
    Variant {
        id: "moonshine-base-es",
        name: "Moonshine Base (Spanish)",
        language: "es",
        tiny: false,
        commercial: false,
        disk_mb: 62,
    },
    Variant {
        id: "moonshine-base-ja",
        name: "Moonshine Base (Japanese)",
        language: "ja",
        tiny: false,
        commercial: false,
        disk_mb: 135,
    },
    Variant {
        id: "moonshine-base-zh",
        name: "Moonshine Base (Chinese)",
        language: "zh",
        tiny: false,
        commercial: false,
        disk_mb: 135,
    },
    Variant {
        id: "moonshine-base-ar",
        name: "Moonshine Base (Arabic)",
        language: "ar",
        tiny: false,
        commercial: false,
        disk_mb: 135,
    },
    Variant {
        id: "moonshine-base-uk",
        name: "Moonshine Base (Ukrainian)",
        language: "uk",
        tiny: false,
        commercial: false,
        disk_mb: 135,
    },
    Variant {
        id: "moonshine-base-vi",
        name: "Moonshine Base (Vietnamese)",
        language: "vi",
        tiny: false,
        commercial: false,
        disk_mb: 135,
    },
    Variant {
        id: "moonshine-tiny-ko",
        name: "Moonshine Tiny (Korean)",
        language: "ko",
        tiny: true,
        commercial: false,
        disk_mb: 68,
    },
    Variant {
        id: "moonshine-tiny-ja",
        name: "Moonshine Tiny (Japanese)",
        language: "ja",
        tiny: true,
        commercial: false,
        disk_mb: 68,
    },
];

pub fn variant(id: &str) -> Option<Variant> {
    VARIANTS.iter().copied().find(|v| v.id == id)
}

const START: i64 = 1;
const END: i64 = 2;
/// Moonshine produces at most about 6.5 tokens per second of speech.
const TOKENS_PER_SECOND: f32 = 6.5;
/// How often a partial transcript comes, in milliseconds of new audio.
pub use crate::utterance::PARTIAL_EVERY_MS;
/// Too little audio to be a word.
const MIN_AUDIO: usize = SAMPLE_RATE as usize * 3 / 10;

/// Moonshine Base (English), the default.
pub fn info() -> EngineInfo {
    info_for(&VARIANTS[0])
}

pub fn info_for(v: &Variant) -> EngineInfo {
    EngineInfo {
        id: v.id.into(),
        name: v.name.into(),
        slot: EngineSlot::Stt,
        kind: EngineKind::Local,
        license: if v.commercial {
            "MIT"
        } else {
            "Moonshine Community License (non-commercial)"
        }
        .into(),
        languages: vec![v.language.into()],
        streaming: true,
        accel: vec![Accel::Cpu],
        resources: ResourceEstimate {
            ram_mb: if v.tiny { 150 } else { 350 },
            vram_mb: 0,
            disk_mb: v.disk_mb,
        },
        model: Some(v.id.into()),
    }
}

/// Every Moonshine engine.
pub fn infos() -> Vec<EngineInfo> {
    VARIANTS.iter().map(info_for).collect()
}

pub struct Moonshine {
    info: EngineInfo,
    encoder: Session,
    decoder: Session,
    pieces: Vec<Vec<u8>>,
    /// The decoder's shape, read from the model: layers, heads per layer, width per head.
    layers: usize,
    heads: usize,
    head_dim: usize,
    inputs: Inputs,
}

/// Which optional inputs this export takes.
#[derive(Clone, Copy, Debug)]
struct Inputs {
    encoder_mask: bool,
    decoder_mask: bool,
    cache_branch: bool,
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
        let session = |file: &str| crate::onnx::session(&dir.join(file), threads);
        let tokens = std::fs::read_to_string(dir.join("tokens.txt"))
            .map_err(|e| VoiceError::Engine(format!("tokens.txt: {e}")))?;
        let decoder = session("decoder_model_merged.ort")?;
        let encoder = session("encoder_model.ort")?;
        let has = |session: &Session, name: &str| session.inputs().iter().any(|i| i.name() == name);
        // Exports differ: the English ones take attention masks and a cache switch, some others
        // don't.
        let inputs = Inputs {
            encoder_mask: has(&encoder, "attention_mask"),
            decoder_mask: has(&decoder, "encoder_attention_mask"),
            cache_branch: has(&decoder, "use_cache_branch"),
        };
        let layers = decoder
            .inputs()
            .iter()
            .filter(|i| i.name().ends_with(".decoder.key"))
            .count();
        let (heads, head_dim) = decoder
            .inputs()
            .iter()
            .find(|i| i.name() == "past_key_values.0.decoder.key")
            .and_then(|i| i.dtype().tensor_shape().map(|s| s.to_vec()))
            .and_then(|s| {
                Some((
                    usize::try_from(*s.get(1)?).ok()?,
                    usize::try_from(*s.get(3)?).ok()?,
                ))
            })
            .ok_or_else(|| VoiceError::Engine("the speech model's decoder is unfamiliar".into()))?;
        let id = dir
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(variant)
            .unwrap_or(VARIANTS[0]);
        Ok(Self {
            info: info_for(&id),
            encoder,
            decoder,
            pieces: parse_tokens(&tokens)?,
            layers,
            heads,
            head_dim,
            inputs,
        })
    }

    /// Transcribes a whole utterance.
    pub fn transcribe(&mut self, audio: &[f32], cancel: &CancellationToken) -> VoiceResult<String> {
        if audio.len() < MIN_AUDIO {
            return Ok(String::new());
        }
        let samples = audio.len();
        let mut encoder_inputs = ort::inputs![
            "input_values" => Tensor::from_array(([1usize, samples], audio.to_vec()))?,
        ];
        if self.inputs.encoder_mask {
            encoder_inputs.push((
                "attention_mask".into(),
                Tensor::from_array(([1usize, samples], vec![1_i64; samples]))?.into(),
            ));
        }
        let outputs = self.encoder.run(encoder_inputs)?;
        let (shape, hidden) = outputs["last_hidden_state"].try_extract_tensor::<f32>()?;
        let frames = usize::try_from(shape[1]).unwrap_or(0);
        let width = usize::try_from(shape[2]).unwrap_or(0);
        let (layers, heads, head_dim) = (self.layers, self.heads, self.head_dim);
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
            (0..layers).map(|_| (empty(), empty())).collect();
        let mut cross_kv: Vec<(Vec<f32>, Vec<f32>)> =
            (0..layers).map(|_| (empty(), empty())).collect();
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
                "encoder_hidden_states" => Tensor::from_array(([1usize, frames, width], hidden.clone()))?,
            ];
            if self.inputs.decoder_mask {
                inputs.push((
                    "encoder_attention_mask".into(),
                    Tensor::from_array(([1usize, frames], vec![1_i64; frames]))?.into(),
                ));
            }
            if self.inputs.cache_branch {
                inputs.push((
                    "use_cache_branch".into(),
                    Tensor::from_array(([1usize], vec![!first]))?.into(),
                ));
            }
            let cross_len = if first { 0 } else { frames };
            for (layer, ((dk, dv), (ek, ev))) in self_kv.iter().zip(&cross_kv).enumerate() {
                let kv = |data: &Vec<f32>, len: usize| {
                    Tensor::from_array(([1usize, heads, len, head_dim], data.clone()))
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
        Box::new(Utterance::new(self, cancel))
    }
}

/// The longest input the model takes: its decoder fails from about 9.5 s (384 encoder frames).
pub const MAX_SAMPLES: usize = SAMPLE_RATE as usize * 9;

impl Transcriber for Moonshine {
    fn transcribe(&mut self, audio: &[f32], cancel: &CancellationToken) -> VoiceResult<String> {
        Moonshine::transcribe(self, audio, cancel)
    }

    fn max_samples(&self) -> Option<usize> {
        Some(MAX_SAMPLES)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::TtsEngine as _;

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
        assert_eq!(
            crate::utterance::common_words("open chr", "open chrome"),
            "open"
        );
        assert_eq!(
            crate::utterance::common_words("open chrome", "open chrome please"),
            "open"
        );
        assert_eq!(
            crate::utterance::common_words("open chrome and", "open chrome now"),
            "open chrome"
        );
        assert_eq!(crate::utterance::common_words("", "open"), "");
        assert_eq!(crate::utterance::common_words("mute", "unmute"), "");
    }

    #[test]
    fn argmax_picks_the_largest_logit() {
        assert_eq!(argmax(&[0.1, 3.0, -1.0, 2.9]), 1);
    }

    /// A round trip for any variant: Supertonic says a sentence in the model's language and the
    /// model must hear most of its words (`KIVO_MOONSHINE_DIR`, `KIVO_SUPERTONIC_DIR`).
    #[test]
    fn hears_speech_in_its_own_language_when_the_models_are_here() {
        let (Some(dir), Some(voice)) = (
            std::env::var_os("KIVO_MOONSHINE_DIR").map(std::path::PathBuf::from),
            std::env::var_os("KIVO_SUPERTONIC_DIR").map(std::path::PathBuf::from),
        ) else {
            eprintln!("KIVO_MOONSHINE_DIR or KIVO_SUPERTONIC_DIR not set; skipping");
            return;
        };
        let mut engine = Moonshine::load(&dir, 2).unwrap();
        let language = engine.info().languages[0].clone();
        let sentence = match language.as_str() {
            "es" => "Abre la calculadora y pon música tranquila.",
            "ja" => "電卓を開いて、静かな音楽をかけてください。",
            "uk" => "Відкрий калькулятор і увімкни спокійну музику.",
            "vi" => "Mở máy tính và bật nhạc nhẹ nhàng.",
            "ar" => "افتح الآلة الحاسبة وشغل موسيقى هادئة.",
            "ko" => "계산기를 열고 조용한 음악을 틀어 주세요.",
            _ => "Open the calculator and play some quiet music.",
        };
        let mut tts = crate::supertonic::Supertonic::load(&voice, 2, &language).unwrap();
        let mut spoken = Vec::new();
        let mut rate = 0;
        tts.speak(sentence, None, &CancellationToken::new(), &mut |pcm, r| {
            rate = r;
            spoken.extend_from_slice(pcm);
            Ok(())
        })
        .unwrap();
        // Linear resampling is plenty for a recognizer test.
        #[allow(clippy::cast_precision_loss, reason = "sample positions")]
        let step = rate as f32 / SAMPLE_RATE as f32;
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let audio: Vec<f32> = (0..(spoken.len() as f32 / step) as usize)
            .map(|i| {
                let at = i as f32 * step;
                let j = at as usize;
                let next = spoken.get(j + 1).copied().unwrap_or(0.0);
                spoken[j] + (next - spoken[j]) * (at - j as f32)
            })
            .collect();
        let text = engine
            .transcribe(&audio, &CancellationToken::new())
            .unwrap();
        eprintln!("{language}: said “{sentence}”, heard “{text}”");
        let chars = |s: &str| -> Vec<char> { s.chars().filter(|c| c.is_alphanumeric()).collect() };
        let (said, heard) = (chars(sentence), chars(&text));
        let shared = said.iter().filter(|c| heard.contains(c)).count();
        assert!(shared * 3 >= said.len() * 2, "heard too little of it");
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

    /// Speech past the model's 9.5 s limit is heard in segments (the model alone fails on it).
    #[test]
    fn hears_speech_longer_than_the_model_takes_when_it_is_installed() {
        let Some(dir) = std::env::var_os("KIVO_MOONSHINE_DIR").map(std::path::PathBuf::from) else {
            eprintln!("KIVO_MOONSHINE_DIR not set; skipping");
            return;
        };
        let Ok(wav) = std::fs::read(dir.join("test_wavs/0.wav")) else {
            eprintln!("no sample recording in KIVO_MOONSHINE_DIR; skipping");
            return;
        };
        let mut engine = Moonshine::load(&dir, 4).unwrap();
        let one = crate::utterance::wav_samples(&wav);
        // The sample three times with a short pause between: about 18 s.
        let mut audio = Vec::new();
        for _ in 0..3 {
            audio.extend_from_slice(&one);
            audio.extend(std::iter::repeat_n(0.0, SAMPLE_RATE as usize / 2));
        }
        assert!(
            engine
                .transcribe(&audio, &CancellationToken::new())
                .is_err(),
            "the model alone can't take it"
        );
        let once = engine.transcribe(&one, &CancellationToken::new()).unwrap();
        let text =
            crate::utterance::transcribe_long(&mut engine, &audio, &CancellationToken::new())
                .unwrap();
        eprintln!("{text}");
        let words = |t: &str| t.split_whitespace().count();
        assert!(words(&text) >= words(&once) * 2, "{once} / {text}");
    }
}
