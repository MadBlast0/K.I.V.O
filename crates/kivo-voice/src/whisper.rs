//! Whisper large-v3-turbo on ONNX Runtime (VOICE §3, VOICE-10): the Accurate speech recognizer,
//! about a hundred languages, MIT-licensed by OpenAI. It runs the int8 sherpa-onnx export: an
//! encoder that returns every decoder layer's cross-attention keys and values, and a decoder with
//! a self-attention cache, decoded greedily after the start-of-transcript sequence for the chosen
//! language. Like Moonshine and Parakeet it transcribes a whole utterance at a time, with partials
//! while the user speaks.
//!
//! Features are Whisper's own: the audio padded to 30 s, a 400-point Hann STFT every 10 ms, 128
//! Slaney mel bins, `log10`, clamped to 8 below the maximum and scaled to `(x + 4) / 4`.
//!
//! Model files: `turbo-encoder.int8.onnx`, `turbo-decoder.int8.onnx`, `turbo-tokens.txt`.

use crate::engine::{Accel, EngineInfo, EngineKind, EngineSlot, ResourceEstimate};
use crate::error::{VoiceError, VoiceResult};
use crate::traits::{SAMPLE_RATE, SttEngine, SttEvent, SttOptions, SttStream};
use crate::utterance::{Transcriber, common_words};
use base64::Engine as _;
use ort::session::Session;
use ort::value::Tensor;
use realfft::RealFftPlanner;
use std::path::Path;
use tokio_util::sync::CancellationToken;

pub const MODEL_ID: &str = "whisper-large-v3-turbo";

const N_FFT: usize = 400;
const HOP: usize = 160;
const CHUNK_SAMPLES: usize = SAMPLE_RATE as usize * 30;
const FRAMES: usize = CHUNK_SAMPLES / HOP;
/// The longest transcript for one 30 s window.
const MAX_TOKENS: usize = 224;
const MIN_AUDIO: usize = SAMPLE_RATE as usize * 3 / 10;

/// The languages KIVO offers Whisper for (it has more; these are KIVO's language list).
pub const LANGUAGES: [&str; 33] = [
    "en", "ar", "bg", "cs", "da", "de", "el", "es", "et", "fi", "fr", "hi", "hr", "hu", "id", "it",
    "ja", "ko", "lt", "lv", "nl", "pa", "pl", "pt", "ro", "ru", "sk", "sl", "sv", "tr", "uk", "vi",
    "zh",
];

pub fn info() -> EngineInfo {
    EngineInfo {
        id: MODEL_ID.into(),
        name: "Whisper large-v3-turbo".into(),
        slot: EngineSlot::Stt,
        kind: EngineKind::Local,
        license: "MIT".into(),
        languages: LANGUAGES.iter().map(|l| (*l).to_owned()).collect(),
        streaming: true,
        accel: vec![Accel::Cpu, Accel::DirectMl],
        resources: ResourceEstimate {
            ram_mb: 1_800,
            vram_mb: 0,
            disk_mb: 1_000,
        },
        model: Some(MODEL_ID.into()),
    }
}

fn hz_to_slaney(hz: f32) -> f32 {
    let (f_sp, min_log_hz) = (200.0 / 3.0, 1000.0);
    let logstep = 6.4f32.ln() / 27.0;
    if hz >= min_log_hz {
        min_log_hz / f_sp + (hz / min_log_hz).ln() / logstep
    } else {
        hz / f_sp
    }
}

fn slaney_to_hz(mel: f32) -> f32 {
    let (f_sp, min_log_hz) = (200.0 / 3.0, 1000.0);
    let min_log_mel = min_log_hz / f_sp;
    let logstep = 6.4f32.ln() / 27.0;
    if mel >= min_log_mel {
        min_log_hz * (logstep * (mel - min_log_mel)).exp()
    } else {
        f_sp * mel
    }
}

/// librosa's Slaney mel filters for a 400-point FFT, 0–8 kHz.
fn filters(bins: usize) -> Vec<Vec<(usize, f32)>> {
    let fft_bins = N_FFT / 2 + 1;
    let fft_hz: Vec<f32> = (0..fft_bins)
        .map(|i| i as f32 * SAMPLE_RATE as f32 / N_FFT as f32)
        .collect();
    let (lo, hi) = (hz_to_slaney(0.0), hz_to_slaney(SAMPLE_RATE as f32 / 2.0));
    let points: Vec<f32> = (0..bins + 2)
        .map(|i| slaney_to_hz(lo + (hi - lo) * i as f32 / (bins as f32 + 1.0)))
        .collect();
    (0..bins)
        .map(|m| {
            let (left, centre, right) = (points[m], points[m + 1], points[m + 2]);
            let norm = 2.0 / (right - left);
            fft_hz
                .iter()
                .enumerate()
                .filter_map(|(i, &f)| {
                    let w = ((f - left) / (centre - left)).min((right - f) / (right - centre));
                    (w > 0.0).then_some((i, w * norm))
                })
                .collect()
        })
        .collect()
}

/// Whisper's log-mel spectrogram of the first 30 s of `audio`, `[bins][3000]`.
pub fn log_mel(audio: &[f32], bins: usize) -> Vec<f32> {
    let mut samples = audio[..audio.len().min(CHUNK_SAMPLES)].to_vec();
    samples.resize(CHUNK_SAMPLES, 0.0);
    // Centred STFT with reflection padding.
    let pad = N_FFT / 2;
    let mut padded = Vec::with_capacity(samples.len() + 2 * pad);
    padded.extend((1..=pad).rev().map(|i| samples[i]));
    padded.extend_from_slice(&samples);
    let n = samples.len();
    padded.extend((0..pad).map(|i| samples[n - 2 - i]));
    // A periodic Hann window.
    let window: Vec<f32> = (0..N_FFT)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / N_FFT as f32).cos())
        .collect();
    let bank = filters(bins);
    let fft = RealFftPlanner::<f32>::new().plan_fft_forward(N_FFT);
    let mut frame = vec![0.0; N_FFT];
    let mut spectrum = fft.make_output_vec();
    let mut out = vec![0.0; bins * FRAMES];
    for t in 0..FRAMES {
        for (i, w) in window.iter().enumerate() {
            frame[i] = padded[t * HOP + i] * w;
        }
        if fft.process(&mut frame, &mut spectrum).is_err() {
            continue;
        }
        for (b, filter) in bank.iter().enumerate() {
            let energy: f32 = filter
                .iter()
                .map(|&(i, w)| spectrum[i].norm_sqr() * w)
                .sum();
            out[b * FRAMES + t] = energy.max(1e-10).log10();
        }
    }
    let max = out.iter().copied().fold(f32::MIN, f32::max);
    for v in &mut out {
        *v = (v.max(max - 8.0) + 4.0) / 4.0;
    }
    out
}

pub struct Whisper {
    info: EngineInfo,
    encoder: Session,
    decoder: Session,
    /// Token id → its bytes.
    tokens: Vec<Vec<u8>>,
    mels: usize,
    layers: usize,
    context: usize,
    state: usize,
    sot: i64,
    eot: i64,
    transcribe: i64,
    no_timestamps: i64,
    /// Language code → its token.
    language_tokens: Vec<(String, i64)>,
}

impl Whisper {
    pub fn load(dir: &Path, threads: usize) -> VoiceResult<Self> {
        Self::load_on(dir, threads, crate::accel::Device::Cpu)
    }

    /// Loads the model on `device`: the GPU (DirectML) where the runtime's policy allows, else
    /// the processor.
    pub fn load_on(dir: &Path, threads: usize, device: crate::accel::Device) -> VoiceResult<Self> {
        let files = [
            "turbo-encoder.int8.onnx",
            "turbo-decoder.int8.onnx",
            "turbo-tokens.txt",
        ];
        if files.iter().any(|f| !dir.join(f).is_file()) {
            return Err(VoiceError::ModelMissing("speech recognition".into()));
        }
        let mut gpu = false;
        let mut session = |file: &str| -> VoiceResult<Session> {
            let (session, on_gpu) = crate::accel::session(&dir.join(file), threads, device)?;
            gpu |= on_gpu;
            Ok(session)
        };
        let encoder = session("turbo-encoder.int8.onnx")?;
        let meta = |key: &str| -> Option<String> { encoder.metadata().ok()?.custom(key) };
        let number = |key: &str| -> VoiceResult<i64> {
            meta(key)
                .and_then(|v| v.trim().parse().ok())
                .ok_or_else(|| VoiceError::Engine(format!("the Whisper model has no {key}")))
        };
        let size = |key: &str| -> VoiceResult<usize> {
            usize::try_from(number(key)?).map_err(|_| VoiceError::Engine(key.into()))
        };
        let codes = meta("all_language_codes").unwrap_or_default();
        let ids = meta("all_language_tokens").unwrap_or_default();
        let language_tokens = codes
            .split(',')
            .zip(ids.split(','))
            .filter_map(|(c, i)| Some((c.trim().to_owned(), i.trim().parse().ok()?)))
            .collect();
        let text = std::fs::read_to_string(dir.join("turbo-tokens.txt"))
            .map_err(|e| VoiceError::Engine(format!("tokens: {e}")))?;
        let mut tokens: Vec<Vec<u8>> = Vec::new();
        for line in text.lines() {
            let Some((piece, id)) = line.rsplit_once(' ') else {
                continue;
            };
            let Ok(id) = id.trim().parse::<usize>() else {
                continue;
            };
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(piece.trim())
                .unwrap_or_default();
            if tokens.len() <= id {
                tokens.resize(id + 1, Vec::new());
            }
            tokens[id] = bytes;
        }
        let mut engine = Self {
            info: info(),
            mels: size("n_mels").unwrap_or(128),
            layers: size("n_text_layer")?,
            context: size("n_text_ctx")?,
            state: size("n_text_state")?,
            sot: number("sot")?,
            eot: number("eot")?,
            transcribe: number("transcribe")?,
            no_timestamps: number("no_timestamps")?,
            language_tokens,
            decoder: session("turbo-decoder.int8.onnx")?,
            encoder,
            tokens,
        };
        // Where it runs, for the Voice page's details (VOICE §8).
        if gpu {
            engine.info.accel = vec![Accel::DirectMl, Accel::Cpu];
        }
        Ok(engine)
    }

    fn language_token(&self, language: &str) -> Option<i64> {
        let primary = language.split('-').next().unwrap_or(language);
        self.language_tokens
            .iter()
            .find(|(code, _)| code.eq_ignore_ascii_case(primary))
            .map(|(_, id)| *id)
    }

    /// The first 30 s of `audio` as text, in `language`.
    pub fn transcribe_in(
        &mut self,
        audio: &[f32],
        language: &str,
        cancel: &CancellationToken,
    ) -> VoiceResult<String> {
        if audio.len() < MIN_AUDIO {
            return Ok(String::new());
        }
        let mel = log_mel(audio, self.mels);
        let outputs = self
            .encoder
            .run(ort::inputs!["mel" => Tensor::from_array(([1usize, self.mels, FRAMES], mel))?])?;
        let (k_shape, cross_k) = outputs[0].try_extract_tensor::<f32>()?;
        let k_shape: Vec<usize> = k_shape
            .iter()
            .map(|&d| usize::try_from(d).unwrap_or(0))
            .collect();
        let cross_k = cross_k.to_vec();
        let cross_v = outputs[1].try_extract_tensor::<f32>()?.1.to_vec();
        drop(outputs);

        let mut prompt = vec![self.sot];
        if let Some(lang) = self.language_token(language) {
            prompt.push(lang);
        }
        prompt.extend([self.transcribe, self.no_timestamps]);
        let cache_shape = [self.layers, 1, self.context, self.state];
        let mut self_k = vec![0.0f32; self.layers * self.context * self.state];
        let mut self_v = self_k.clone();
        let mut input = prompt.clone();
        let mut offset = 0i64;
        let mut out: Vec<i64> = Vec::new();
        while out.len() < MAX_TOKENS {
            if cancel.is_cancelled() {
                return Err(VoiceError::Cancelled);
            }
            let n = input.len();
            let outputs = self.decoder.run(ort::inputs![
                "tokens" => Tensor::from_array(([1usize, n], input.clone()))?,
                "in_n_layer_self_k_cache" => Tensor::from_array((cache_shape, self_k))?,
                "in_n_layer_self_v_cache" => Tensor::from_array((cache_shape, self_v))?,
                "n_layer_cross_k" => Tensor::from_array((k_shape.clone(), cross_k.clone()))?,
                "n_layer_cross_v" => Tensor::from_array((k_shape.clone(), cross_v.clone()))?,
                "offset" => Tensor::from_array(([1usize], vec![offset]))?,
            ])?;
            let (shape, logits) = outputs[0].try_extract_tensor::<f32>()?;
            let vocab = usize::try_from(shape[2]).unwrap_or(0);
            let last = &logits[(n - 1) * vocab..n * vocab];
            let next = last
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .map_or(self.eot, |(i, _)| i64::try_from(i).unwrap_or(self.eot));
            self_k = outputs[1].try_extract_tensor::<f32>()?.1.to_vec();
            self_v = outputs[2].try_extract_tensor::<f32>()?.1.to_vec();
            offset += i64::try_from(n).unwrap_or(0);
            if next == self.eot || usize::try_from(offset).unwrap_or(usize::MAX) >= self.context {
                break;
            }
            out.push(next);
            input = vec![next];
        }
        Ok(self.text(&out))
    }

    fn text(&self, ids: &[i64]) -> String {
        let bytes: Vec<u8> = ids
            .iter()
            .filter(|&&id| id < self.sot)
            .filter_map(|&id| self.tokens.get(usize::try_from(id).ok()?))
            .flatten()
            .copied()
            .collect();
        String::from_utf8_lossy(&bytes).trim().to_owned()
    }
}

/// A Whisper session for one language (the utterance's).
struct InLanguage<'a> {
    engine: &'a mut Whisper,
    language: String,
}

impl Transcriber for InLanguage<'_> {
    fn transcribe(&mut self, audio: &[f32], cancel: &CancellationToken) -> VoiceResult<String> {
        self.engine.transcribe_in(audio, &self.language, cancel)
    }
}

impl SttEngine for Whisper {
    fn info(&self) -> &EngineInfo {
        &self.info
    }

    fn start(
        &mut self,
        options: &SttOptions,
        cancel: CancellationToken,
    ) -> Box<dyn SttStream + '_> {
        let language = options.language.clone();
        Box::new(Owned {
            inner: InLanguage {
                engine: self,
                language,
            },
            cancel,
            audio: Vec::new(),
            last_partial: 0,
            last: String::new(),
            stable: String::new(),
        })
    }
}

/// Whisper's utterance: partials once a second (it's heavier than Moonshine), words two partials
/// agree on marked stable, the whole utterance transcribed at the end.
struct Owned<'a> {
    inner: InLanguage<'a>,
    cancel: CancellationToken,
    audio: Vec<f32>,
    last_partial: usize,
    last: String,
    stable: String,
}

impl SttStream for Owned<'_> {
    fn accept(&mut self, audio: &[f32]) -> VoiceResult<Vec<SttEvent>> {
        self.audio.extend_from_slice(audio);
        if self.audio.len() < MIN_AUDIO
            || self.audio.len() - self.last_partial < SAMPLE_RATE as usize
        {
            return Ok(Vec::new());
        }
        self.last_partial = self.audio.len();
        let text = self.inner.transcribe(&self.audio, &self.cancel)?;
        let mut events = Vec::new();
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
        self.inner.transcribe(&self.audio, &self.cancel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn features_are_whispers_3000_frames() {
        let tone: Vec<f32> = (0..16_000)
            .map(|i| (i as f32 * 2.0 * std::f32::consts::PI * 440.0 / 16_000.0).sin() * 0.3)
            .collect();
        let mel = log_mel(&tone, 128);
        assert_eq!(mel.len(), 128 * 3000);
        let max = mel.iter().copied().fold(f32::MIN, f32::max);
        let min = mel.iter().copied().fold(f32::MAX, f32::min);
        assert!(
            max - min <= 2.0 + 1e-4,
            "clamped to 8 below the peak, scaled by 1/4"
        );
    }

    #[test]
    fn transcribes_the_sample_recording_when_the_model_is_there() {
        let Some(dir) = std::env::var_os("KIVO_WHISPER_DIR").map(std::path::PathBuf::from) else {
            eprintln!("KIVO_WHISPER_DIR isn't set; skipping");
            return;
        };
        let mut engine = Whisper::load(&dir, 4).unwrap();
        let wav = std::fs::read(dir.join("test_wavs/0.wav")).unwrap();
        let audio = crate::utterance::wav_samples(&wav);
        let text = engine
            .transcribe_in(&audio, "en", &CancellationToken::new())
            .unwrap();
        assert_eq!(
            text,
            "After early nightfall the yellow lamps would light up here and there the squalid quarter of the brothels."
        );
    }
}
