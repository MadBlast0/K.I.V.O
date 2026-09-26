//! Parakeet TDT 0.6B v3 on ONNX Runtime (VOICE §3, VOICE-10): the Balanced speech recognizer, 25
//! European languages in one model, CC-BY-4.0 by NVIDIA. It runs NeMo's transducer branch as the
//! sherpa-onnx export ships it (int8 encoder, prediction network and joiner) and decodes it
//! greedily with token-and-duration (TDT) steps. Like Moonshine it transcribes a whole utterance
//! at a time, with partials while the user speaks (`utterance`).
//!
//! Features follow NeMo's preprocessor, which the model was trained on: pre-emphasis 0.97, a
//! 25 ms Hann window every 10 ms in a centred 512-point STFT, power spectrum, 128 Slaney mel bins
//! from 0 to 8 kHz, `ln(x + 2^-24)`, then each bin normalized over the utterance.
//!
//! Model files (downloaded by the model manager): `encoder.int8.onnx`, `decoder.int8.onnx`,
//! `joiner.int8.onnx`, `tokens.txt`.

use crate::engine::{Accel, EngineInfo, EngineKind, EngineSlot, ResourceEstimate};
use crate::error::{VoiceError, VoiceResult};
use crate::traits::{SAMPLE_RATE, SttEngine, SttOptions, SttStream};
use crate::utterance::{Transcriber, Utterance};
use ort::session::Session;
use ort::value::Tensor;
use realfft::RealFftPlanner;
use std::path::Path;
use tokio_util::sync::CancellationToken;

pub const MODEL_ID: &str = "parakeet-tdt-v3";

/// The languages Parakeet TDT v3 was trained on.
pub const LANGUAGES: [&str; 25] = [
    "bg", "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "de", "el", "hu", "it", "lv", "lt", "mt",
    "pl", "pt", "ro", "sk", "sl", "es", "sv", "ru", "uk",
];

const MEL_BINS: usize = 128;
const FFT_LEN: usize = 512;
const WIN_LEN: usize = 400;
const HOP: usize = 160;
const PREEMPHASIS: f32 = 0.97;
/// The steps a TDT joiner can take after a symbol.
const DURATIONS: [usize; 5] = [0, 1, 2, 3, 4];
/// At most this many symbols from one encoder frame.
const MAX_SYMBOLS_PER_FRAME: usize = 10;
/// Too little audio to be a word.
const MIN_AUDIO: usize = SAMPLE_RATE as usize * 3 / 10;

pub fn info() -> EngineInfo {
    EngineInfo {
        id: MODEL_ID.into(),
        name: "Parakeet TDT v3".into(),
        slot: EngineSlot::Stt,
        kind: EngineKind::Local,
        license: "CC-BY-4.0".into(),
        languages: LANGUAGES.iter().map(|l| (*l).to_owned()).collect(),
        streaming: true,
        accel: vec![Accel::Cpu],
        resources: ResourceEstimate {
            ram_mb: 1_100,
            vram_mb: 0,
            disk_mb: 640,
        },
        model: Some(MODEL_ID.into()),
    }
}

/// NeMo-style log-mel features, `[bins][frames]`, normalized per bin.
pub fn features(samples: &[f32]) -> (Vec<f32>, usize) {
    // Pre-emphasis.
    let mut x = Vec::with_capacity(samples.len());
    let mut prev = 0.0;
    for &s in samples {
        x.push(s - PREEMPHASIS * prev);
        prev = s;
    }
    // A centred STFT: FFT_LEN / 2 zeros on each side.
    let pad = FFT_LEN / 2;
    let mut padded = vec![0.0; pad];
    padded.extend_from_slice(&x);
    padded.extend(std::iter::repeat_n(0.0, pad));
    let frames = 1 + x.len() / HOP;
    // A symmetric Hann window of WIN_LEN, centred in FFT_LEN.
    let offset = (FFT_LEN - WIN_LEN) / 2;
    let window: Vec<f32> = (0..WIN_LEN)
        .map(|n| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * n as f32 / (WIN_LEN as f32 - 1.0)).cos())
        .collect();
    let filters = slaney_mel();
    let fft = RealFftPlanner::<f32>::new().plan_fft_forward(FFT_LEN);
    let mut frame = vec![0.0; FFT_LEN];
    let mut spectrum = fft.make_output_vec();
    let mut out = vec![0.0; MEL_BINS * frames];
    for t in 0..frames {
        let start = t * HOP;
        frame.fill(0.0);
        for n in 0..WIN_LEN {
            frame[offset + n] = padded.get(start + offset + n).copied().unwrap_or(0.0) * window[n];
        }
        if fft.process(&mut frame, &mut spectrum).is_err() {
            continue;
        }
        let power: Vec<f32> = spectrum.iter().map(|c| c.norm_sqr()).collect();
        for (b, filter) in filters.iter().enumerate() {
            let energy: f32 = filter.iter().map(|&(i, w)| power[i] * w).sum();
            out[b * frames + t] = (energy + 2f32.powi(-24)).ln();
        }
    }
    // Per-bin normalization over the utterance (unbiased std, + 1e-5).
    for b in 0..MEL_BINS {
        let row = &mut out[b * frames..(b + 1) * frames];
        let mean = row.iter().sum::<f32>() / frames as f32;
        let var = if frames > 1 {
            row.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / (frames as f32 - 1.0)
        } else {
            0.0
        };
        let std = var.sqrt() + 1e-5;
        for v in row.iter_mut() {
            *v = (*v - mean) / std;
        }
    }
    (out, frames)
}

fn hz_to_slaney(hz: f32) -> f32 {
    let (f_sp, min_log_hz) = (200.0 / 3.0, 1000.0);
    let min_log_mel = min_log_hz / f_sp;
    let logstep = 6.4f32.ln() / 27.0;
    if hz >= min_log_hz {
        min_log_mel + (hz / min_log_hz).ln() / logstep
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

/// librosa's mel filters (`htk=False`, `norm='slaney'`), 0–8 kHz, as (FFT bin, weight) lists.
fn slaney_mel() -> Vec<Vec<(usize, f32)>> {
    let bins = FFT_LEN / 2 + 1;
    let fft_hz: Vec<f32> = (0..bins)
        .map(|i| i as f32 * SAMPLE_RATE as f32 / FFT_LEN as f32)
        .collect();
    let (lo, hi) = (hz_to_slaney(0.0), hz_to_slaney(SAMPLE_RATE as f32 / 2.0));
    let points: Vec<f32> = (0..MEL_BINS + 2)
        .map(|i| slaney_to_hz(lo + (hi - lo) * i as f32 / (MEL_BINS as f32 + 1.0)))
        .collect();
    (0..MEL_BINS)
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

/// The prediction network's two LSTM states.
type DecoderState = (Vec<f32>, Vec<f32>);

pub struct Parakeet {
    info: EngineInfo,
    encoder: Session,
    decoder: Session,
    joiner: Session,
    /// Token id → text piece (`▁` marks a word start).
    tokens: Vec<String>,
    blank: usize,
    /// The prediction network's layers and width.
    layers: usize,
    hidden: usize,
}

impl Parakeet {
    /// Loads the model from `dir`, running on `threads` CPU threads.
    pub fn load(dir: &Path, threads: usize) -> VoiceResult<Self> {
        let files = [
            "encoder.int8.onnx",
            "decoder.int8.onnx",
            "joiner.int8.onnx",
            "tokens.txt",
        ];
        if files.iter().any(|f| !dir.join(f).is_file()) {
            return Err(VoiceError::ModelMissing("speech recognition".into()));
        }
        let session = |file: &str| crate::onnx::session(&dir.join(file), threads);
        let encoder = session("encoder.int8.onnx")?;
        let meta = |key: &str| -> Option<usize> {
            encoder.metadata().ok()?.custom(key)?.trim().parse().ok()
        };
        let vocab = meta("vocab_size").unwrap_or(8192);
        let layers = meta("pred_rnn_layers").unwrap_or(2);
        let hidden = meta("pred_hidden").unwrap_or(640);
        let text = std::fs::read_to_string(dir.join("tokens.txt"))
            .map_err(|e| VoiceError::Engine(format!("tokens.txt: {e}")))?;
        let mut tokens = vec![String::new(); vocab + 1];
        for line in text.lines() {
            let Some((piece, id)) = line.rsplit_once(' ') else {
                continue;
            };
            if let Ok(id) = id.trim().parse::<usize>()
                && id < tokens.len()
            {
                tokens[id] = piece.to_owned();
            }
        }
        Ok(Self {
            info: info(),
            decoder: session("decoder.int8.onnx")?,
            joiner: session("joiner.int8.onnx")?,
            encoder,
            tokens,
            blank: vocab,
            layers,
            hidden,
        })
    }

    /// One step of the prediction network: its output for `token` and the new LSTM state.
    fn predict(
        &mut self,
        token: usize,
        state: (Vec<f32>, Vec<f32>),
    ) -> VoiceResult<(Vec<f32>, DecoderState)> {
        let shape = [self.layers, 1, self.hidden];
        let token = i32::try_from(token).map_err(|_| VoiceError::Engine("token id".into()))?;
        let outputs = self.decoder.run(ort::inputs![
            "targets" => Tensor::from_array(([1usize, 1], vec![token]))?,
            "target_length" => Tensor::from_array(([1usize], vec![1i32]))?,
            "states.1" => Tensor::from_array((shape, state.0))?,
            "onnx::Slice_3" => Tensor::from_array((shape, state.1))?,
        ])?;
        let out = outputs[0].try_extract_tensor::<f32>()?.1.to_vec();
        let h = outputs[2].try_extract_tensor::<f32>()?.1.to_vec();
        let c = outputs[3].try_extract_tensor::<f32>()?.1.to_vec();
        Ok((out, (h, c)))
    }

    /// The whole utterance as text.
    pub fn transcribe(&mut self, audio: &[f32], cancel: &CancellationToken) -> VoiceResult<String> {
        if audio.len() < MIN_AUDIO {
            return Ok(String::new());
        }
        let (features, frames) = features(audio);
        let frames_i = i64::try_from(frames).map_err(|_| VoiceError::Engine("length".into()))?;
        let outputs = self.encoder.run(ort::inputs![
            "audio_signal" => Tensor::from_array(([1usize, MEL_BINS, frames], features))?,
            "length" => Tensor::from_array(([1usize], vec![frames_i]))?,
        ])?;
        let (shape, encoded) = outputs[0].try_extract_tensor::<f32>()?;
        let dim = usize::try_from(shape[1]).unwrap_or(1024);
        let steps = usize::try_from(shape[2]).unwrap_or(0);
        let lengths = outputs[1].try_extract_tensor::<i64>()?.1.to_vec();
        let steps = lengths
            .first()
            .and_then(|&l| usize::try_from(l).ok())
            .map_or(steps, |l| l.min(steps));
        // Frame t of the encoder output, [1, dim, 1] (the output is [1, dim, steps]).
        let frame = |t: usize| -> Vec<f32> { (0..dim).map(|d| encoded[d * steps + t]).collect() };
        let encoded_frames: Vec<Vec<f32>> = (0..steps).map(frame).collect();
        drop(outputs);

        let zeros = vec![0.0; self.layers * self.hidden];
        let (mut predicted, mut state) = self.predict(self.blank, (zeros.clone(), zeros))?;
        let mut ids = Vec::new();
        let (mut t, mut emitted) = (0, 0);
        while t < steps {
            if cancel.is_cancelled() {
                return Err(VoiceError::Cancelled);
            }
            let logits = {
                let out = self.joiner.run(ort::inputs![
                    "encoder_outputs" => Tensor::from_array(([1usize, dim, 1], encoded_frames[t].clone()))?,
                    "decoder_outputs" => Tensor::from_array(([1usize, self.hidden, 1], predicted.clone()))?,
                ])?;
                out[0].try_extract_tensor::<f32>()?.1.to_vec()
            };
            let (tokens, durations) = logits.split_at(self.blank + 1);
            let token = argmax(tokens);
            let mut skip = DURATIONS[argmax(durations).min(DURATIONS.len() - 1)];
            if token != self.blank {
                ids.push(token);
                emitted += 1;
                (predicted, state) = self.predict(token, state)?;
            } else if skip == 0 {
                skip = 1;
            }
            if emitted >= MAX_SYMBOLS_PER_FRAME {
                skip = skip.max(1);
            }
            if skip > 0 {
                t += skip;
                emitted = 0;
            }
        }
        Ok(self.text(&ids))
    }

    fn text(&self, ids: &[usize]) -> String {
        let joined: String = ids
            .iter()
            .filter_map(|&id| self.tokens.get(id))
            .filter(|p| !p.starts_with('<'))
            .map(String::as_str)
            .collect();
        joined.replace('▁', " ").trim().to_owned()
    }
}

fn argmax(values: &[f32]) -> usize {
    values
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map_or(0, |(i, _)| i)
}

impl Transcriber for Parakeet {
    fn transcribe(&mut self, audio: &[f32], cancel: &CancellationToken) -> VoiceResult<String> {
        Parakeet::transcribe(self, audio, cancel)
    }
}

impl SttEngine for Parakeet {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn features_have_128_normalized_bins_every_10_ms() {
        let tone: Vec<f32> = (0..16_000)
            .map(|i| (i as f32 * 2.0 * std::f32::consts::PI * 440.0 / 16_000.0).sin() * 0.3)
            .collect();
        let (f, frames) = features(&tone);
        assert_eq!(frames, 101);
        assert_eq!(f.len(), 128 * 101);
        // Each bin is normalized: mean ~0.
        let row = &f[10 * frames..11 * frames];
        assert!(row.iter().sum::<f32>().abs() / (frames as f32) < 1e-3);
    }

    #[test]
    fn slaney_filters_cover_the_band_once() {
        let filters = slaney_mel();
        assert_eq!(filters.len(), 128);
        assert!(filters.iter().all(|f| !f.is_empty()));
        // The first filter starts at the lowest bins, the last ends near 8 kHz.
        assert!(filters[0][0].0 <= 1);
        assert!(filters[127].last().unwrap().0 >= 250);
    }

    /// The sample recording, when the model is present (KIVO_PARAKEET_DIR).
    #[test]
    fn transcribes_the_sample_recording_when_the_model_is_there() {
        let Some(dir) = std::env::var_os("KIVO_PARAKEET_DIR").map(std::path::PathBuf::from) else {
            eprintln!("KIVO_PARAKEET_DIR isn't set; skipping");
            return;
        };
        let mut engine = Parakeet::load(&dir, 4).unwrap();
        let wav = std::fs::read(dir.join("test_wavs/en.wav")).unwrap();
        let audio = crate::utterance::wav_samples(&wav);
        let text = engine
            .transcribe(&audio, &CancellationToken::new())
            .unwrap();
        println!("{text}");
        assert_eq!(
            text,
            "Ask not what your country can do for you. Ask what you can do for your country."
        );
    }
}
