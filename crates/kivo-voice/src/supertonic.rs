//! Supertonic 3 (Supertone; code MIT, weights OpenRAIL-M; VOICE §11, VOICE-46): multilingual
//! text-to-speech in 31 languages, from characters (no phonemizer), on ONNX Runtime. A port of
//! Supertone's own Rust example: text → character ids → duration predictor → text encoder →
//! flow-matching denoising (8 steps) → vocoder, one sentence group at a time.
//!
//! OpenRAIL-M carries use restrictions; KIVO shows the licence before the download (DIST-13).

use crate::engine::{Accel, EngineInfo, EngineKind, EngineSlot, ResourceEstimate};
use crate::error::{VoiceError, VoiceResult};
use crate::traits::{AudioSink, TtsEngine, VoiceInfo};
use ort::session::Session;
use ort::value::Tensor;
use serde::Deserialize;
use std::path::Path;
use tokio_util::sync::CancellationToken;
use unicode_normalization::UnicodeNormalization;

pub const ENGINE_ID: &str = "supertonic-3";
pub const DEFAULT_VOICE: &str = "F1";
/// Supertone's defaults.
const STEPS: usize = 8;
const SPEED: f32 = 1.05;
const CHUNK_GAP_SECONDS: f32 = 0.3;

/// The languages Supertonic 3 speaks (its `AVAILABLE_LANGS`, less the "na" placeholder).
pub const LANGUAGES: [&str; 31] = [
    "en", "ko", "ja", "ar", "bg", "cs", "da", "de", "el", "es", "et", "fi", "fr", "hi", "hr", "hu",
    "id", "it", "lt", "lv", "nl", "pl", "pt", "ro", "ru", "sk", "sl", "sv", "tr", "uk", "vi",
];
pub const VOICES: [&str; 10] = ["F1", "F2", "F3", "F4", "F5", "M1", "M2", "M3", "M4", "M5"];

pub fn info() -> EngineInfo {
    EngineInfo {
        id: ENGINE_ID.into(),
        name: "Supertonic 3".into(),
        slot: EngineSlot::Tts,
        kind: EngineKind::Local,
        license: "OpenRAIL-M".into(),
        languages: LANGUAGES.iter().map(|l| (*l).to_owned()).collect(),
        streaming: true,
        accel: vec![Accel::Cpu],
        resources: ResourceEstimate {
            ram_mb: 900,
            vram_mb: 0,
            disk_mb: 400,
        },
        model: Some(ENGINE_ID.into()),
    }
}

#[derive(Deserialize)]
struct Config {
    ae: AeConfig,
    ttl: TtlConfig,
}

#[derive(Deserialize)]
struct AeConfig {
    sample_rate: u32,
    base_chunk_size: usize,
}

#[derive(Deserialize)]
struct TtlConfig {
    chunk_compress_factor: usize,
    latent_dim: usize,
}

#[derive(Deserialize)]
struct StyleComponent {
    data: Vec<Vec<Vec<f32>>>,
    dims: Vec<usize>,
}

#[derive(Deserialize)]
struct StyleFile {
    style_ttl: StyleComponent,
    style_dp: StyleComponent,
}

struct Style {
    ttl: (Vec<usize>, Vec<f32>),
    dp: (Vec<usize>, Vec<f32>),
}

fn flat(c: StyleComponent) -> (Vec<usize>, Vec<f32>) {
    (c.dims, c.data.into_iter().flatten().flatten().collect())
}

fn engine_err(e: impl std::fmt::Display) -> VoiceError {
    VoiceError::Engine(e.to_string())
}

pub struct Supertonic {
    info: EngineInfo,
    config: Config,
    indexer: Vec<i64>,
    duration: Session,
    encoder: Session,
    estimator: Session,
    vocoder: Session,
    styles: Vec<(String, Style)>,
    language: String,
    noise: Noise,
}

fn session(path: &Path, threads: usize) -> VoiceResult<Session> {
    if !path.is_file() {
        return Err(VoiceError::ModelMissing("Supertonic".into()));
    }
    Ok(Session::builder()?
        .with_intra_threads(threads.max(1))?
        .with_inter_threads(1)?
        .commit_from_file(path)?)
}

impl Supertonic {
    /// Loads the model folder (`onnx/…`, `voice_styles/…`) to speak `language`.
    pub fn load(dir: &Path, threads: usize, language: &str) -> VoiceResult<Self> {
        let onnx = dir.join("onnx");
        let read = |p: &Path| {
            std::fs::read_to_string(p).map_err(|_| VoiceError::ModelMissing("Supertonic".into()))
        };
        let config: Config =
            serde_json::from_str(&read(&onnx.join("tts.json"))?).map_err(engine_err)?;
        let indexer: Vec<i64> =
            serde_json::from_str(&read(&onnx.join("unicode_indexer.json"))?).map_err(engine_err)?;
        let mut styles = Vec::new();
        for id in VOICES {
            let file = dir.join("voice_styles").join(format!("{id}.json"));
            if let Ok(text) = std::fs::read_to_string(&file) {
                let style: StyleFile = serde_json::from_str(&text).map_err(engine_err)?;
                styles.push((
                    id.to_owned(),
                    Style {
                        ttl: flat(style.style_ttl),
                        dp: flat(style.style_dp),
                    },
                ));
            }
        }
        if styles.is_empty() {
            return Err(VoiceError::ModelMissing("Supertonic".into()));
        }
        let base = language.split('-').next().unwrap_or("en");
        Ok(Self {
            info: info(),
            config,
            indexer,
            duration: session(&onnx.join("duration_predictor.onnx"), threads)?,
            encoder: session(&onnx.join("text_encoder.onnx"), threads)?,
            estimator: session(&onnx.join("vector_estimator.onnx"), threads)?,
            vocoder: session(&onnx.join("vocoder.onnx"), threads)?,
            styles,
            language: if LANGUAGES.contains(&base) {
                base
            } else {
                "en"
            }
            .to_owned(),
            noise: Noise::new(),
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.config.ae.sample_rate
    }

    fn synthesize(&mut self, text: &str, voice: &str) -> VoiceResult<Vec<f32>> {
        let style = self
            .styles
            .iter()
            .find(|(id, _)| id == voice)
            .or_else(|| self.styles.first())
            .map(|(_, s)| s)
            .ok_or_else(|| VoiceError::ModelMissing("Supertonic".into()))?;
        let prepared = preprocess(text, &self.language);
        let ids: Vec<i64> = prepared
            .chars()
            .map(|c| self.indexer.get(c as usize).copied().unwrap_or(-1))
            .collect();
        let n = ids.len();
        let mask = vec![1.0_f32; n];
        let tensor = |shape: &[usize], data: Vec<f32>| Tensor::from_array((shape.to_vec(), data));
        let ids_t = Tensor::from_array(([1usize, n], ids))?;
        let mask_t = tensor(&[1, 1, n], mask.clone())?;
        let dp_t = tensor(&style.dp.0, style.dp.1.clone())?;
        let seconds = {
            let out = self.duration.run(ort::inputs![
                "text_ids" => ids_t.clone(),
                "style_dp" => dp_t,
                "text_mask" => mask_t.clone(),
            ])?;
            let (_, d) = out["duration"].try_extract_tensor::<f32>()?;
            d.first().copied().unwrap_or(0.0) / SPEED
        };
        let ttl_t = tensor(&style.ttl.0, style.ttl.1.clone())?;
        let (emb_shape, emb) = {
            let out = self.encoder.run(ort::inputs![
                "text_ids" => ids_t,
                "style_ttl" => ttl_t.clone(),
                "text_mask" => mask_t.clone(),
            ])?;
            let (shape, data) = out["text_emb"].try_extract_tensor::<f32>()?;
            (
                shape
                    .iter()
                    .map(|&d| usize::try_from(d).unwrap_or(0))
                    .collect::<Vec<_>>(),
                data.to_vec(),
            )
        };
        // The latent: noise, one frame per compressed chunk of audio.
        let rate = self.config.ae.sample_rate as usize;
        let chunk = self.config.ae.base_chunk_size * self.config.ttl.chunk_compress_factor;
        let dim = self.config.ttl.latent_dim * self.config.ttl.chunk_compress_factor;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let wav_len = (seconds.max(0.0) * rate as f32) as usize;
        let frames = wav_len.div_ceil(chunk).max(1);
        let mut latent: Vec<f32> = (0..dim * frames).map(|_| self.noise.next()).collect();
        let latent_mask = vec![1.0_f32; frames];
        #[allow(clippy::cast_precision_loss, reason = "a handful of steps")]
        for step in 0..STEPS {
            let out = self.estimator.run(ort::inputs![
                "noisy_latent" => tensor(&[1, dim, frames], latent.clone())?,
                "text_emb" => tensor(&emb_shape, emb.clone())?,
                "style_ttl" => ttl_t.clone(),
                "latent_mask" => tensor(&[1, 1, frames], latent_mask.clone())?,
                "text_mask" => mask_t.clone(),
                "current_step" => tensor(&[1], vec![step as f32])?,
                "total_step" => tensor(&[1], vec![STEPS as f32])?,
            ])?;
            let (_, next) = out["denoised_latent"].try_extract_tensor::<f32>()?;
            latent.clear();
            latent.extend_from_slice(next);
        }
        let out = self
            .vocoder
            .run(ort::inputs!["latent" => tensor(&[1, dim, frames], latent)?])?;
        let (_, wav) = out["wav_tts"].try_extract_tensor::<f32>()?;
        Ok(wav[..wav_len.min(wav.len())].to_vec())
    }
}

impl TtsEngine for Supertonic {
    fn info(&self) -> &EngineInfo {
        &self.info
    }

    fn voices(&self) -> Vec<VoiceInfo> {
        self.styles
            .iter()
            .map(|(id, _)| VoiceInfo {
                id: id.clone(),
                name: voice_name(id),
                language: self.language.clone(),
            })
            .collect()
    }

    fn speak(
        &mut self,
        text: &str,
        voice: Option<&str>,
        cancel: &CancellationToken,
        sink: AudioSink<'_>,
    ) -> VoiceResult<()> {
        let voice = voice.unwrap_or(DEFAULT_VOICE).to_owned();
        let max_len = if matches!(self.language.as_str(), "ko" | "ja") {
            120
        } else {
            300
        };
        let rate = self.sample_rate();
        for (i, chunk) in chunks(text, max_len).iter().enumerate() {
            if cancel.is_cancelled() {
                return Err(VoiceError::Cancelled);
            }
            let audio = self.synthesize(chunk, &voice)?;
            if cancel.is_cancelled() {
                return Err(VoiceError::Cancelled);
            }
            if i > 0 {
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    clippy::cast_precision_loss
                )]
                let gap = vec![0.0; (CHUNK_GAP_SECONDS * rate as f32) as usize];
                sink(&gap, rate)?;
            }
            sink(&audio, rate)?;
        }
        Ok(())
    }
}

/// "F1" → "Voice 1 (female)".
pub fn voice_name(id: &str) -> String {
    let gender = if id.starts_with('M') {
        "male"
    } else {
        "female"
    };
    format!("Voice {} ({gender})", &id[1..])
}

/// Supertone's text preparation: NFKD, emoji and odd symbols removed, punctuation spacing
/// tidied, a final full stop, and the language tags the model expects.
pub fn preprocess(text: &str, language: &str) -> String {
    let mut t: String = text.nfkd().collect();
    t.retain(|c| !is_emoji(c));
    for (from, to) in [
        ('–', "-"),
        ('‑', "-"),
        ('—', "-"),
        ('_', " "),
        ('\u{201C}', "\""),
        ('\u{201D}', "\""),
        ('\u{2018}', "'"),
        ('\u{2019}', "'"),
        ('´', "'"),
        ('`', "'"),
        ('[', " "),
        (']', " "),
        ('|', " "),
        ('/', " "),
        ('#', " "),
        ('→', " "),
        ('←', " "),
    ] {
        t = t.replace(from, to);
    }
    for symbol in ['♥', '☆', '♡', '©', '\\'] {
        t = t.replace(symbol, "");
    }
    for (from, to) in [
        ("@", " at "),
        ("e.g.,", "for example, "),
        ("i.e.,", "that is, "),
    ] {
        t = t.replace(from, to);
    }
    for p in [",", ".", "!", "?", ";", ":", "'"] {
        t = t.replace(&format!(" {p}"), p);
    }
    while t.contains("\"\"") {
        t = t.replace("\"\"", "\"");
    }
    while t.contains("''") {
        t = t.replace("''", "'");
    }
    let mut t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    let ends_well = t
        .chars()
        .last()
        .is_some_and(|c| ".!?;:,'\"\u{201C}\u{201D}\u{2018}\u{2019})]}…。」』】〉》›»".contains(c));
    if !t.is_empty() && !ends_well {
        t.push('.');
    }
    format!("<{language}>{t}</{language}>")
}

fn is_emoji(c: char) -> bool {
    matches!(u32::from(c),
        0x1F600..=0x1F64F | 0x1F300..=0x1F5FF | 0x1F680..=0x1F6FF | 0x1F700..=0x1F77F
        | 0x1F780..=0x1F7FF | 0x1F800..=0x1F8FF | 0x1F900..=0x1F9FF | 0x1FA00..=0x1FA6F
        | 0x1FA70..=0x1FAFF | 0x2600..=0x26FF | 0x2700..=0x27BF | 0x1F1E6..=0x1F1FF)
}

/// Sentence groups of at most `max_len` bytes (paragraphs, then sentences, then commas, then
/// words), as Supertone's example splits long text.
pub fn chunks(text: &str, max_len: usize) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
        if para.len() <= max_len {
            out.push(para.to_owned());
            continue;
        }
        let mut current = String::new();
        for sentence in split_sentences(para) {
            let pieces: Vec<String> = if sentence.len() > max_len {
                split_long(&sentence, max_len)
            } else {
                vec![sentence]
            };
            for piece in pieces {
                if !current.is_empty() && current.len() + piece.len() + 1 > max_len {
                    out.push(std::mem::take(&mut current));
                }
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(&piece);
            }
        }
        if !current.is_empty() {
            out.push(current);
        }
    }
    out
}

const ABBREVIATIONS: [&str; 21] = [
    "Dr.", "Mr.", "Mrs.", "Ms.", "Prof.", "Sr.", "Jr.", "St.", "Ave.", "Rd.", "Blvd.", "Dept.",
    "Inc.", "Ltd.", "Co.", "Corp.", "etc.", "vs.", "i.e.", "e.g.", "Ph.D.",
];

fn split_sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = text.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        current.push(c);
        let boundary = matches!(c, '.' | '!' | '?')
            && chars.get(i + 1).is_some_and(|n| n.is_whitespace())
            && !ABBREVIATIONS
                .iter()
                .any(|a| current.trim_end().ends_with(a));
        if boundary {
            out.push(current.trim().to_owned());
            current.clear();
        }
    }
    if !current.trim().is_empty() {
        out.push(current.trim().to_owned());
    }
    out
}

fn split_long(sentence: &str, max_len: usize) -> Vec<String> {
    let mut out = Vec::new();
    for part in sentence.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        if part.len() <= max_len {
            out.push(part.to_owned());
            continue;
        }
        let mut words = String::new();
        for word in part.split_whitespace() {
            if !words.is_empty() && words.len() + word.len() + 1 > max_len {
                out.push(std::mem::take(&mut words));
            }
            if !words.is_empty() {
                words.push(' ');
            }
            words.push_str(word);
        }
        if !words.is_empty() {
            out.push(words);
        }
    }
    out
}

/// Gaussian noise for the latent (xorshift and Box–Muller: no extra dependency).
struct Noise {
    state: u64,
    spare: Option<f32>,
}

impl Noise {
    fn new() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0x9E37_79B9_7F4A_7C15, |d| {
                u64::try_from(d.as_nanos() & u128::from(u64::MAX)).unwrap_or(1)
            });
        Self {
            state: seed | 1,
            spare: None,
        }
    }

    fn uniform(&mut self) -> f32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        #[allow(clippy::cast_precision_loss, reason = "a random fraction")]
        let u = (self.state >> 40) as f32 / (1u64 << 24) as f32;
        u.max(f32::MIN_POSITIVE)
    }

    fn next(&mut self) -> f32 {
        if let Some(s) = self.spare.take() {
            return s;
        }
        let (u1, u2) = (self.uniform(), self.uniform());
        let r = (-2.0 * u1.ln()).sqrt();
        let theta = std::f32::consts::TAU * u2;
        self.spare = Some(r * theta.sin());
        r * theta.cos()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_prepared_as_supertone_does() {
        assert_eq!(preprocess("Hello  world", "en"), "<en>Hello world.</en>");
        assert_eq!(
            preprocess("It\u{2019}s \u{201C}fine\u{201D} , really 😀", "en"),
            "<en>It's \"fine\", really.</en>"
        );
        assert_eq!(preprocess("Wait!", "de"), "<de>Wait!</de>");
    }

    #[test]
    fn long_text_is_split_into_sentence_groups() {
        let text = "Dr. Smith arrived. ".repeat(30);
        let parts = chunks(&text, 120);
        assert!(parts.iter().all(|p| p.len() <= 120), "{parts:?}");
        assert!(
            parts[0].starts_with("Dr. Smith arrived."),
            "abbreviations stay together"
        );
        assert_eq!(chunks("Short.", 300), ["Short."]);
    }

    #[test]
    fn the_noise_is_standard_normal() {
        let mut n = Noise::new();
        let samples: Vec<f32> = (0..20_000).map(|_| n.next()).collect();
        #[allow(clippy::cast_precision_loss)]
        let len = samples.len() as f32;
        let mean = samples.iter().sum::<f32>() / len;
        let var = samples.iter().map(|s| (s - mean).powi(2)).sum::<f32>() / len;
        assert!(
            mean.abs() < 0.05 && (var - 1.0).abs() < 0.05,
            "{mean} {var}"
        );
    }

    #[test]
    fn speaks_several_languages_when_the_model_is_here() {
        let Some(dir) = std::env::var_os("KIVO_SUPERTONIC_DIR").map(std::path::PathBuf::from)
        else {
            eprintln!("KIVO_SUPERTONIC_DIR not set; skipping");
            return;
        };
        for (lang, text) in [
            ("en", "Hi, I'm KIVO. How can I help?"),
            ("hi", "नमस्ते, मैं कीवो हूँ।"),
        ] {
            let started = std::time::Instant::now();
            let mut tts = Supertonic::load(&dir, 4, lang).unwrap();
            let loaded = started.elapsed();
            let started = std::time::Instant::now();
            let mut audio = Vec::new();
            tts.speak(text, None, &CancellationToken::new(), &mut |pcm, rate| {
                assert_eq!(rate, tts_rate());
                audio.extend_from_slice(pcm);
                Ok(())
            })
            .unwrap();
            let took = started.elapsed();
            #[allow(clippy::cast_precision_loss)]
            let seconds = audio.len() as f32 / tts_rate() as f32;
            eprintln!(
                "{lang}: loaded in {loaded:?}, {seconds:.2} s of speech in {took:?} (RTF {:.2})",
                took.as_secs_f32() / seconds
            );
            assert!((0.8..8.0).contains(&seconds), "{seconds}");
            assert!(audio.iter().any(|s| s.abs() > 0.05), "audible");
        }
    }

    fn tts_rate() -> u32 {
        44_100
    }
}
