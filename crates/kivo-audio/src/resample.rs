//! Sample-rate conversion (VOICE §1): the microphone's native format to 16 kHz mono for the
//! engines, and speech or earcons to the speaker's format. FFT resampling (`rubato`) on 10 ms
//! chunks: fixed ratio, high quality, no allocation per call.

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

/// Converts mono audio from one rate to another in 10 ms chunks.
pub struct RateConverter {
    fft: Option<Fft<f32>>,
    chunk: usize,
    pending: Vec<f32>,
    out: Vec<f32>,
}

impl RateConverter {
    /// A converter from `from` Hz to `to` Hz (mono). Equal rates pass audio straight through.
    pub fn new(from: u32, to: u32) -> Self {
        let chunk = (from as usize / 100).max(1);
        let fft = (from != to).then(|| {
            Fft::<f32>::new(from as usize, to as usize, chunk, 1, FixedSync::Input)
                .expect("sample rates are positive")
        });
        let out_max = fft.as_ref().map_or(chunk, Resampler::output_frames_max);
        Self {
            fft,
            chunk,
            pending: Vec::with_capacity(chunk * 2),
            out: vec![0.0; out_max],
        }
    }

    /// Converts `input`, calling `emit` with each converted piece. Audio that doesn't fill a whole
    /// 10 ms chunk waits for the next call.
    pub fn push(&mut self, input: &[f32], emit: &mut dyn FnMut(&[f32])) {
        let Some(fft) = &mut self.fft else {
            emit(input);
            return;
        };
        let mut rest = input;
        while !rest.is_empty() {
            let need = self.chunk - self.pending.len();
            let take = need.min(rest.len());
            self.pending.extend_from_slice(&rest[..take]);
            rest = &rest[take..];
            if self.pending.len() < self.chunk {
                break;
            }
            let capacity = self.out.len();
            let written = {
                let input = InterleavedSlice::new(&self.pending, 1, self.chunk).expect("sized");
                let mut output =
                    InterleavedSlice::new_mut(&mut self.out, 1, capacity).expect("sized");
                fft.process_into_buffer(&input, &mut output, None)
                    .map_or(0, |(_, w)| w)
            };
            self.pending.clear();
            if written > 0 {
                emit(&self.out[..written]);
            }
        }
    }

    /// Converts a whole clip at once (earcons, a TTS sentence), flushing the converter's delay.
    pub fn convert_all(from: u32, to: u32, input: &[f32]) -> Vec<f32> {
        if from == to {
            return input.to_vec();
        }
        let mut converter = Self::new(from, to);
        let mut out = Vec::with_capacity(input.len() * to as usize / from as usize + 1024);
        converter.push(input, &mut |s| out.extend_from_slice(s));
        // Push silence to flush the tail, then drop the resampler's delay from the front.
        let delay = converter.fft.as_ref().map_or(0, Resampler::output_delay);
        let flush = vec![0.0; converter.chunk * 2];
        converter.push(&flush, &mut |s| out.extend_from_slice(s));
        let expected = input.len() * to as usize / from as usize;
        out.drain(..delay.min(out.len()));
        out.truncate(expected);
        out
    }
}

/// Averages interleaved channels into mono.
pub fn downmix(interleaved: &[f32], channels: u16, out: &mut Vec<f32>) {
    out.clear();
    let channels = usize::from(channels.max(1));
    if channels == 1 {
        out.extend_from_slice(interleaved);
        return;
    }
    #[allow(clippy::cast_precision_loss, reason = "a channel count")]
    let scale = 1.0 / channels as f32;
    out.extend(
        interleaved
            .chunks_exact(channels)
            .map(|f| f.iter().sum::<f32>() * scale),
    );
}

/// Copies mono audio into every channel of an interleaved buffer.
pub fn upmix(mono: &[f32], channels: u16, out: &mut Vec<f32>) {
    let channels = usize::from(channels.max(1));
    out.reserve(mono.len() * channels);
    for &s in mono {
        out.extend(std::iter::repeat_n(s, channels));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, hz: f32, seconds: f32) -> Vec<f32> {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let n = (rate as f32 * seconds) as usize;
        #[allow(clippy::cast_precision_loss)]
        (0..n)
            .map(|i| (i as f32 * hz * std::f32::consts::TAU / rate as f32).sin() * 0.5)
            .collect()
    }

    fn rms(x: &[f32]) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn a_48k_stream_becomes_16k_in_10ms_steps() {
        let mut c = RateConverter::new(48_000, 16_000);
        let input = tone(48_000, 440.0, 1.0);
        let mut out = Vec::new();
        // Feed it in odd-sized pieces, like a device does.
        for piece in input.chunks(333) {
            c.push(piece, &mut |s| out.extend_from_slice(s));
        }
        assert!((15_800..=16_000).contains(&out.len()), "{}", out.len());
        // A 440 Hz tone keeps its level (well inside the pass band).
        assert!((rms(&out[4000..]) - rms(&input)).abs() < 0.02);
    }

    #[test]
    fn content_above_the_new_nyquist_is_removed() {
        let out = RateConverter::convert_all(48_000, 16_000, &tone(48_000, 12_000.0, 0.5));
        assert!(rms(&out[1000..]) < 0.01, "aliasing: {}", rms(&out));
    }

    #[test]
    fn whole_clips_keep_their_length_and_start() {
        let clip = tone(24_000, 300.0, 0.25);
        let out = RateConverter::convert_all(24_000, 48_000, &clip);
        assert_eq!(out.len(), clip.len() * 2);
        assert!(rms(&out[..2400]) > 0.2, "the start of the clip is kept");
    }

    #[test]
    fn equal_rates_pass_through() {
        let mut c = RateConverter::new(16_000, 16_000);
        let mut out = Vec::new();
        c.push(&[0.1, 0.2, 0.3], &mut |s| out.extend_from_slice(s));
        assert_eq!(out, [0.1, 0.2, 0.3]);
    }

    #[test]
    fn channels_are_averaged_and_copied() {
        let mut mono = Vec::new();
        downmix(&[1.0, 0.0, 0.5, 0.5], 2, &mut mono);
        assert_eq!(mono, [0.5, 0.5]);
        let mut stereo = Vec::new();
        upmix(&[0.25], 2, &mut stereo);
        assert_eq!(stereo, [0.25, 0.25]);
    }
}
