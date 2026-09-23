//! Kaldi-compatible log-mel filterbank features, computed as audio streams in (VOICE §2). The
//! keyword spotter and the speaker model (CAM++) were trained on these features, so they follow
//! `kaldi-native-fbank` exactly with the options sherpa-onnx uses: 25 ms Povey windows every
//! 10 ms, DC removal, pre-emphasis 0.97, no dither, a 512-point FFT, 80 mel bins from 20 Hz to
//! 7.6 kHz, log power, and `snip_edges = false` (the first frame is centred on the first 10 ms,
//! with the start of the audio reflected).

use realfft::{RealFftPlanner, RealToComplex};
use std::sync::Arc;

pub const SAMPLE_RATE: usize = 16_000;
const FRAME_LEN: usize = 400;
const FRAME_SHIFT: usize = 160;
const FFT_LEN: usize = 512;
const PREEMPHASIS: f32 = 0.97;
const LOW_HZ: f32 = 20.0;
const HIGH_HZ: f32 = SAMPLE_RATE as f32 / 2.0 - 400.0;

/// Mel bins per frame.
pub const BINS: usize = 80;

/// One mel filter: the first FFT bin it covers and its weights from there.
struct Filter {
    first: usize,
    weights: Vec<f32>,
}

fn mel(hz: f32) -> f32 {
    1127.0 * (1.0 + hz / 700.0).ln()
}

fn filters() -> Vec<Filter> {
    let bin_width = SAMPLE_RATE as f32 / FFT_LEN as f32;
    let (low, high) = (mel(LOW_HZ), mel(HIGH_HZ));
    let delta = (high - low) / (BINS as f32 + 1.0);
    (0..BINS)
        .map(|b| {
            let left = low + b as f32 * delta;
            let centre = left + delta;
            let right = centre + delta;
            let mut first = None;
            let mut weights = Vec::new();
            // Kaldi uses the first FFT_LEN / 2 bins (the Nyquist bin is left out).
            for i in 0..FFT_LEN / 2 {
                let m = mel(bin_width * i as f32);
                if m > left && m < right {
                    first.get_or_insert(i);
                    weights.push(if m <= centre {
                        (m - left) / (centre - left)
                    } else {
                        (right - m) / (right - centre)
                    });
                }
            }
            Filter {
                first: first.unwrap_or(0),
                weights,
            }
        })
        .collect()
}

/// Streaming feature extraction: feed audio with `accept`, take frames with `frames`.
pub struct OnlineFbank {
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    filters: Vec<Filter>,
    /// Audio not yet fully used, starting at absolute sample `start`.
    audio: Vec<f32>,
    start: usize,
    /// The first samples ever received, for reflecting the start of the audio.
    head: Vec<f32>,
    received: usize,
    next_frame: usize,
    ready: Vec<[f32; BINS]>,
    // Scratch buffers.
    frame: Vec<f32>,
    spectrum: Vec<realfft::num_complex::Complex<f32>>,
    power: Vec<f32>,
}

impl Default for OnlineFbank {
    fn default() -> Self {
        Self::new()
    }
}

impl OnlineFbank {
    pub fn new() -> Self {
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(FFT_LEN);
        let spectrum = fft.make_output_vec();
        let window = (0..FRAME_LEN)
            .map(|i| {
                let a = 2.0 * std::f32::consts::PI * i as f32 / (FRAME_LEN - 1) as f32;
                (0.5 - 0.5 * a.cos()).powf(0.85)
            })
            .collect();
        Self {
            fft,
            window,
            filters: filters(),
            audio: Vec::new(),
            start: 0,
            head: Vec::with_capacity(FRAME_LEN),
            received: 0,
            next_frame: 0,
            ready: Vec::new(),
            frame: vec![0.0; FFT_LEN],
            spectrum,
            power: vec![0.0; FFT_LEN / 2 + 1],
        }
    }

    /// Starts over, as for a new recording.
    pub fn reset(&mut self) {
        self.audio.clear();
        self.start = 0;
        self.head.clear();
        self.received = 0;
        self.next_frame = 0;
        self.ready.clear();
    }

    /// Frames computed so far and not yet taken.
    pub fn frames(&mut self) -> Vec<[f32; BINS]> {
        std::mem::take(&mut self.ready)
    }

    /// Adds 16 kHz mono samples in [-1, 1] and computes every frame they complete.
    pub fn accept(&mut self, samples: &[f32]) {
        if self.head.len() < FRAME_LEN {
            let take = (FRAME_LEN - self.head.len()).min(samples.len());
            self.head.extend_from_slice(&samples[..take]);
        }
        self.audio.extend_from_slice(samples);
        self.received += samples.len();
        // Frame i covers [i·shift + shift/2 − len/2, … + len), i.e. it starts 120 samples early.
        while self.next_frame * FRAME_SHIFT + FRAME_SHIFT / 2 + FRAME_LEN / 2 <= self.received {
            self.compute_frame();
            self.next_frame += 1;
        }
        // Keep only what the next frame still needs.
        let next_start =
            (self.next_frame * FRAME_SHIFT + FRAME_SHIFT / 2).saturating_sub(FRAME_LEN / 2);
        if next_start > self.start {
            let drop = (next_start - self.start).min(self.audio.len());
            self.audio.drain(..drop);
            self.start += drop;
        }
    }

    fn sample(&self, index: isize) -> f32 {
        if index < 0 {
            // Kaldi reflects the start: sample −k is sample k − 1.
            let reflected = (-index - 1) as usize;
            return self.head.get(reflected).copied().unwrap_or(0.0);
        }
        let i = index as usize;
        if i >= self.start {
            self.audio.get(i - self.start).copied().unwrap_or(0.0)
        } else {
            self.head.get(i).copied().unwrap_or(0.0)
        }
    }

    fn compute_frame(&mut self) {
        let first =
            (self.next_frame * FRAME_SHIFT + FRAME_SHIFT / 2) as isize - (FRAME_LEN / 2) as isize;
        for j in 0..FRAME_LEN {
            self.frame[j] = self.sample(first + j as isize);
        }
        let frame = &mut self.frame[..FRAME_LEN];
        let mean = frame.iter().sum::<f32>() / FRAME_LEN as f32;
        frame.iter_mut().for_each(|s| *s -= mean);
        for j in (1..FRAME_LEN).rev() {
            frame[j] -= PREEMPHASIS * frame[j - 1];
        }
        frame[0] -= PREEMPHASIS * frame[0];
        for (s, w) in frame.iter_mut().zip(&self.window) {
            *s *= w;
        }
        self.frame[FRAME_LEN..].fill(0.0);
        if self
            .fft
            .process(&mut self.frame, &mut self.spectrum)
            .is_err()
        {
            return;
        }
        for (p, c) in self.power.iter_mut().zip(&self.spectrum) {
            *p = c.norm_sqr();
        }
        let mut out = [0.0_f32; BINS];
        for (o, f) in out.iter_mut().zip(&self.filters) {
            let energy: f32 = f
                .weights
                .iter()
                .zip(&self.power[f.first..])
                .map(|(w, p)| w * p)
                .sum();
            *o = energy.max(f32::EPSILON).ln();
        }
        self.ready.push(out);
    }
}

/// The deterministic test signal the feature tests share (two tones and a little noise),
/// computed in f64 as the Python references are.
#[cfg(test)]
pub(crate) fn tests_signal(len: usize) -> Vec<f32> {
    tests::signal(len)
}

/// Features for a whole clip at once.
pub fn compute(samples: &[f32]) -> Vec<[f32; BINS]> {
    let mut fbank = OnlineFbank::new();
    fbank.accept(samples);
    fbank.frames()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A deterministic test signal: two tones and a little pseudo-random noise.
    pub(crate) fn signal(len: usize) -> Vec<f32> {
        let mut seed = 12_345_u32;
        (0..len)
            .map(|i| {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                // In f64, as the Python reference computes it.
                let noise = (f64::from(seed >> 16) / 32_768.0 - 1.0) * 0.01;
                let t = i as f64 / SAMPLE_RATE as f64;
                (0.3 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()
                    + 0.2 * (2.0 * std::f64::consts::PI * 1_250.0 * t).sin()
                    + noise) as f32
            })
            .collect()
    }

    #[test]
    fn streaming_gives_the_same_frames_as_one_pass() {
        let audio = signal(16_000);
        let whole = compute(&audio);
        let mut online = OnlineFbank::new();
        let mut streamed = Vec::new();
        for chunk in audio.chunks(1_234) {
            online.accept(chunk);
            streamed.extend(online.frames());
        }
        assert_eq!(whole.len(), streamed.len());
        // snip_edges = false: (n + shift/2) / shift frames, less the ones still waiting for audio.
        assert_eq!(whole.len(), (16_000 - 280) / 160 + 1);
        for (a, b) in whole.iter().zip(&streamed) {
            for (x, y) in a.iter().zip(b) {
                assert!((x - y).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn matches_kaldi_native_fbank() {
        // Reference values from kaldi-native-fbank 1.21 with sherpa-onnx's options, for
        // `signal(16_000)` (see the module docs): frames 0, 1 and 50, bins 0, 10, 40 and 79.
        let feats = compute(&signal(16_000));
        let expected: [(usize, usize, f32); 12] = REFERENCE;
        for (frame, bin, value) in expected {
            let got = feats[frame][bin];
            assert!(
                (got - value).abs() < 2e-3,
                "frame {frame} bin {bin}: {got} vs {value}"
            );
        }
    }

    const REFERENCE: [(usize, usize, f32); 12] = [
        (0, 0, -5.26571),
        (0, 10, -0.73884),
        (0, 40, -0.98462),
        (0, 79, -1.77266),
        (1, 0, -11.57144),
        (1, 10, -5.98675),
        (1, 40, -6.65953),
        (1, 79, -1.75030),
        (50, 0, -11.0477),
        (50, 10, -6.02853),
        (50, 40, -5.68125),
        (50, 79, -1.52357),
    ];
}
