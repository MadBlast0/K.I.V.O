//! End-of-turn detection with Smart Turn v3 (Daily/pipecat, BSD-2-Clause; VOICE §3, VOICE-33).
//! Silence alone ends an utterance too early when someone pauses mid-thought ("open… Chrome")
//! and too late when they finish crisply. After a short pause (250 ms) the last 8 seconds of the
//! user's audio go to this small model (a Whisper-tiny encoder with a classifier), which says
//! how likely it is they have finished.
//!
//! Its features are Whisper's, reproduced exactly: the audio is padded at the start to 8 s and
//! normalized, then an 80-bin log-mel spectrogram (400-point Hann STFT every 10 ms, Slaney mel
//! scale up to 8 kHz, log10 floored 8 below the peak, scaled by (x + 4) / 4).

use crate::engine::{Accel, EngineInfo, EngineKind, EngineSlot, ResourceEstimate};
use crate::error::{VoiceError, VoiceResult};
use crate::traits::TurnDetector;
use ort::session::Session;
use ort::value::Tensor;
use realfft::{RealFftPlanner, RealToComplex};
use std::path::Path;
use std::sync::Arc;

pub const ENGINE_ID: &str = "smart-turn-v3";
const RATE: usize = 16_000;
const SECONDS: usize = 8;
const SAMPLES: usize = RATE * SECONDS;
const N_FFT: usize = 400;
const HOP: usize = 160;
const MELS: usize = 80;
/// Frames the model takes (the STFT's last frame is dropped, as Whisper does).
pub const FRAMES: usize = SAMPLES / HOP;

pub fn info() -> EngineInfo {
    EngineInfo {
        id: ENGINE_ID.into(),
        name: "Smart Turn v3".into(),
        slot: EngineSlot::TurnDetector,
        kind: EngineKind::Local,
        license: "BSD-2-Clause".into(),
        languages: vec!["*".into()],
        streaming: false,
        accel: vec![Accel::Cpu],
        resources: ResourceEstimate {
            ram_mb: 40,
            vram_mb: 0,
            disk_mb: 9,
        },
        model: Some(ENGINE_ID.into()),
    }
}

fn hz_to_mel(hz: f64) -> f64 {
    // Slaney: linear below 1 kHz, logarithmic above.
    let (min_log_hz, min_log_mel, logstep) = (1_000.0, 15.0, (6.4_f64).ln() / 27.0);
    if hz >= min_log_hz {
        min_log_mel + (hz / min_log_hz).ln() / logstep
    } else {
        3.0 * hz / 200.0
    }
}

fn mel_to_hz(mel: f64) -> f64 {
    let (min_log_hz, min_log_mel, logstep) = (1_000.0, 15.0, (6.4_f64).ln() / 27.0);
    if mel >= min_log_mel {
        min_log_hz * (logstep * (mel - min_log_mel)).exp()
    } else {
        200.0 * mel / 3.0
    }
}

/// Whisper's mel filters (Slaney scale and normalization), `[MELS][N_FFT / 2 + 1]`.
fn mel_filters() -> Vec<Vec<f32>> {
    let bins = N_FFT / 2 + 1;
    let (lo, hi) = (hz_to_mel(0.0), hz_to_mel(8_000.0));
    #[allow(clippy::cast_precision_loss, reason = "small counts")]
    let points: Vec<f64> = (0..MELS + 2)
        .map(|i| mel_to_hz(lo + (hi - lo) * i as f64 / (MELS + 1) as f64))
        .collect();
    #[allow(clippy::cast_precision_loss, reason = "small counts")]
    let fft_freqs: Vec<f64> = (0..bins)
        .map(|i| i as f64 * (RATE as f64 / 2.0) / (bins - 1) as f64)
        .collect();
    (0..MELS)
        .map(|m| {
            let (left, centre, right) = (points[m], points[m + 1], points[m + 2]);
            let norm = 2.0 / (right - left);
            fft_freqs
                .iter()
                .map(|&f| {
                    let down = (f - left) / (centre - left);
                    let up = (right - f) / (right - centre);
                    #[allow(clippy::cast_possible_truncation, reason = "a filter weight")]
                    let w = (down.min(up).max(0.0) * norm) as f32;
                    w
                })
                .collect()
        })
        .collect()
}

/// Whisper's input features for the last 8 s of `audio`: `[MELS][FRAMES]`, row-major.
pub struct Features {
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    filters: Vec<Vec<f32>>,
}

impl Default for Features {
    fn default() -> Self {
        Self::new()
    }
}

impl Features {
    pub fn new() -> Self {
        #[allow(clippy::cast_precision_loss, reason = "window index")]
        let window = (0..N_FFT)
            .map(|n| 0.5 - 0.5 * (std::f32::consts::TAU * n as f32 / N_FFT as f32).cos())
            .collect();
        Self {
            fft: RealFftPlanner::<f32>::new().plan_fft_forward(N_FFT),
            window,
            filters: mel_filters(),
        }
    }

    pub fn compute(&self, audio: &[f32]) -> Vec<f32> {
        // The last 8 seconds, padded with silence at the start.
        let tail = &audio[audio.len().saturating_sub(SAMPLES)..];
        let mut x = vec![0.0_f32; SAMPLES - tail.len()];
        x.extend_from_slice(tail);
        // Zero mean, unit variance (over the whole window).
        #[allow(clippy::cast_precision_loss, reason = "sample count")]
        let n = x.len() as f64;
        let mean = x.iter().map(|&v| f64::from(v)).sum::<f64>() / n;
        let var = x
            .iter()
            .map(|&v| (f64::from(v) - mean).powi(2))
            .sum::<f64>()
            / n;
        let scale = 1.0 / (var + 1e-7).sqrt();
        #[allow(clippy::cast_possible_truncation, reason = "normalized samples")]
        let x: Vec<f32> = x
            .iter()
            .map(|&v| ((f64::from(v) - mean) * scale) as f32)
            .collect();
        // Centred frames: reflect-pad n_fft/2 on both sides.
        let pad = N_FFT / 2;
        let reflect = |i: isize| -> f32 {
            let len = x.len() as isize;
            let j = if i < 0 {
                -i
            } else if i >= len {
                2 * (len - 1) - i
            } else {
                i
            };
            x[j.clamp(0, len - 1) as usize]
        };
        let bins = N_FFT / 2 + 1;
        let mut frame = vec![0.0_f32; N_FFT];
        let mut spectrum = self.fft.make_output_vec();
        let mut mel = vec![0.0_f32; MELS * FRAMES];
        for t in 0..FRAMES {
            let start = (t * HOP) as isize - pad as isize;
            for (k, v) in frame.iter_mut().enumerate() {
                *v = reflect(start + k as isize) * self.window[k];
            }
            if self.fft.process(&mut frame, &mut spectrum).is_err() {
                continue;
            }
            for (m, filter) in self.filters.iter().enumerate() {
                let energy: f32 = filter
                    .iter()
                    .zip(&spectrum[..bins])
                    .map(|(w, c)| w * c.norm_sqr())
                    .sum();
                mel[m * FRAMES + t] = energy.max(1e-10).log10();
            }
        }
        let peak = mel.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        for v in &mut mel {
            *v = (v.max(peak - 8.0) + 4.0) / 4.0;
        }
        mel
    }
}

pub struct SmartTurn {
    info: EngineInfo,
    session: Session,
    features: Features,
}

impl SmartTurn {
    /// Loads `smart-turn.onnx` from the model folder.
    pub fn load(dir: &Path) -> VoiceResult<Self> {
        let path = dir.join("smart-turn.onnx");
        if !path.is_file() {
            return Err(VoiceError::ModelMissing("end-of-turn".into()));
        }
        let session = Session::builder()?
            .with_intra_threads(1)?
            .with_inter_threads(1)?
            .commit_from_file(path)?;
        Ok(Self {
            info: info(),
            session,
            features: Features::new(),
        })
    }
}

impl TurnDetector for SmartTurn {
    fn info(&self) -> &EngineInfo {
        &self.info
    }

    /// The probability the speaker has finished, from the audio alone (the transcript isn't
    /// used by this model).
    fn end_probability(&mut self, audio_tail: &[f32], _transcript: &str) -> VoiceResult<f32> {
        let features = self.features.compute(audio_tail);
        let input = Tensor::from_array(([1usize, MELS, FRAMES], features))?;
        let outputs = self.session.run(ort::inputs!["input_features" => input])?;
        let (_, p) = outputs[0].try_extract_tensor::<f32>()?;
        Ok(p.first().copied().unwrap_or(0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mel_scale_round_trips() {
        for hz in [0.0, 500.0, 999.0, 1_000.0, 4_000.0, 8_000.0] {
            assert!((mel_to_hz(hz_to_mel(hz)) - hz).abs() < 1e-6, "{hz}");
        }
        assert!((hz_to_mel(1_000.0) - 15.0).abs() < 1e-9);
    }

    #[test]
    fn features_match_whispers_extractor() {
        // Reference values from transformers' WhisperFeatureExtractor(chunk_length=8) on the
        // same signal, padded at the start as Smart Turn's inference does.
        let audio = crate::fbank::tests_signal(16_000 * 3);
        let f = Features::new().compute(&audio);
        assert_eq!(f.len(), MELS * FRAMES);
        for (mel, frame, want) in REFERENCE {
            let got = f[mel * FRAMES + frame];
            assert!(
                (got - want).abs() < 2e-3,
                "mel {mel} frame {frame}: {got} vs {want}"
            );
        }
    }

    const REFERENCE: [(usize, usize, f32); 6] = [
        (0, 0, -0.26838),
        (40, 400, -0.26838),
        (79, 700, 0.41503),
        (10, 600, 1.64153),
        (20, 799, 0.56652),
        (60, 650, 0.39850),
    ];

    #[test]
    fn predicts_when_the_model_is_installed() {
        let Some(dir) = std::env::var_os("KIVO_SMART_TURN_DIR").map(std::path::PathBuf::from)
        else {
            eprintln!("KIVO_SMART_TURN_DIR not set; skipping");
            return;
        };
        let mut model = SmartTurn::load(&dir).unwrap();
        let p = model
            .end_probability(&crate::fbank::tests_signal(16_000 * 2), "")
            .unwrap();
        assert!((0.0..=1.0).contains(&p), "{p}");
    }
}
