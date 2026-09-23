//! Echo cancellation (VOICE §1, VOICE-30): KIVO removes its own voice and sounds from the
//! microphone, so it can hear the user over itself (barge-in, the stop words) without mistaking
//! itself for them. The reference is KIVO's output mix, tapped where the mixer fills the speaker's
//! buffer; the canceller is WebRTC's AEC3 (the pure-Rust `sonora` port, BSD-3-Clause), which
//! estimates the delay between what was played and what the microphone heard by itself.
//!
//! Only KIVO's own output is in the reference: other apps' audio is not removed (the OS AEC,
//! where a device has one, covers everything).

use crate::resample::{RateConverter, downmix};
use sonora::config::EchoCanceller as Aec3;
use sonora::{AudioProcessing, Config, StreamConfig};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// The canceller works on 10 ms frames at 16 kHz.
pub const RATE: u32 = 16_000;
const FRAME: usize = 160;
/// The reference keeps at most this much unread audio (a stalled reader loses the oldest).
const REFERENCE_SECONDS: usize = 2;
/// The render-to-capture delay hint (AEC3 refines it itself): the reference is taken as the
/// mixer fills the speaker's buffer, so the hint is the output plus input latency of a typical
/// shared-mode WASAPI device.
pub const DEFAULT_DELAY_MS: i32 = 40;

struct Tapped {
    rate: u32,
    mono: VecDeque<f32>,
    scratch: Vec<f32>,
}

/// What KIVO is playing, as the mixer writes it to the speaker (mono, at the device's rate).
#[derive(Clone)]
pub struct Reference(Arc<Mutex<Tapped>>);

impl Default for Reference {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(Tapped {
            rate: 48_000,
            mono: VecDeque::new(),
            scratch: Vec::new(),
        })))
    }
}

impl Reference {
    /// Called from the audio thread with the buffer just mixed. Never blocks: if the reader holds
    /// the lock, this buffer is skipped (the canceller tolerates a gap).
    pub fn push(&self, interleaved: &[f32], channels: u16, rate: u32) {
        let Ok(mut tapped) = self.0.try_lock() else {
            return;
        };
        let Tapped {
            rate: r,
            mono,
            scratch,
        } = &mut *tapped;
        if *r != rate {
            *r = rate;
            mono.clear();
        }
        scratch.clear();
        downmix(interleaved, channels, scratch);
        mono.extend(scratch.iter().copied());
        let cap = rate as usize * REFERENCE_SECONDS;
        if mono.len() > cap {
            let extra = mono.len() - cap;
            mono.drain(..extra);
        }
    }

    /// Everything played since the last call, and its rate.
    pub fn take(&self) -> (u32, Vec<f32>) {
        let mut tapped = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let rate = tapped.rate;
        (rate, tapped.mono.drain(..).collect())
    }
}

/// AEC3 on 16 kHz mono, fed any amount of audio at a time.
pub struct EchoCanceller {
    apm: AudioProcessing,
    resampler: Option<(u32, RateConverter)>,
    render: Vec<f32>,
    capture: Vec<f32>,
    cleaned: Vec<f32>,
    frame_out: Vec<f32>,
    delay_ms: i32,
}

impl Default for EchoCanceller {
    fn default() -> Self {
        Self::new()
    }
}

impl EchoCanceller {
    pub fn new() -> Self {
        let stream = StreamConfig::new(RATE, 1);
        let config = Config {
            echo_canceller: Some(Aec3::default()),
            ..Default::default()
        };
        let apm = AudioProcessing::builder()
            .config(config)
            .capture_config(stream)
            .render_config(stream)
            .build();
        Self {
            apm,
            resampler: None,
            render: Vec::new(),
            capture: Vec::new(),
            cleaned: Vec::new(),
            frame_out: vec![0.0; FRAME],
            delay_ms: DEFAULT_DELAY_MS,
        }
    }

    /// The delay hint in ms (see `DEFAULT_DELAY_MS`).
    pub fn set_delay_hint(&mut self, delay_ms: i32) {
        self.delay_ms = delay_ms;
    }

    /// What was just played (at `rate`, mono): the canceller learns what to remove.
    pub fn played(&mut self, rate: u32, mono: &[f32]) {
        if mono.is_empty() {
            return;
        }
        if self.resampler.as_ref().is_none_or(|(r, _)| *r != rate) {
            self.resampler = Some((rate, RateConverter::new(rate, RATE)));
        }
        if let Some((_, converter)) = self.resampler.as_mut() {
            let render = &mut self.render;
            converter.push(mono, &mut |out| render.extend_from_slice(out));
        }
        let mut offset = 0;
        while self.render.len() - offset >= FRAME {
            let frame = &self.render[offset..offset + FRAME];
            let _ = self
                .apm
                .process_render_f32(&[frame], &mut [&mut self.frame_out[..]]);
            offset += FRAME;
        }
        self.render.drain(..offset);
    }

    /// Cleans microphone audio (16 kHz mono) in place. Output lags input by up to 10 ms: the
    /// last partial frame waits for the next call.
    pub fn clean(&mut self, audio: &mut Vec<f32>) {
        self.capture.extend_from_slice(audio);
        audio.clear();
        let mut offset = 0;
        while self.capture.len() - offset >= FRAME {
            let frame = &self.capture[offset..offset + FRAME];
            self.cleaned.resize(FRAME, 0.0);
            let _ = self.apm.set_stream_delay_ms(self.delay_ms);
            if self
                .apm
                .process_capture_f32(&[frame], &mut [&mut self.cleaned[..]])
                .is_ok()
            {
                audio.extend_from_slice(&self.cleaned);
            } else {
                audio.extend_from_slice(frame);
            }
            offset += FRAME;
        }
        self.capture.drain(..offset);
    }

    /// Anything still waiting (when the canceller is switched off).
    pub fn flush(&mut self, audio: &mut Vec<f32>) {
        audio.append(&mut self.capture);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn energy(s: &[f32]) -> f32 {
        s.iter().map(|v| v * v).sum::<f32>()
    }

    /// A speech-like test signal: broadband (pseudo-random, lightly low-passed like a voice's
    /// spectrum) under a syllable-rate envelope, plus a gliding voiced part.
    fn voice(len: usize, seed: u32) -> Vec<f32> {
        let mut state = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
        let mut smooth = 0.0_f32;
        (0..len)
            .map(|i| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                #[allow(clippy::cast_precision_loss)]
                let noise = (state >> 8) as f32 / (1 << 24) as f32 * 2.0 - 1.0;
                smooth = 0.6 * smooth + 0.4 * noise;
                #[allow(clippy::cast_precision_loss)]
                let t = i as f32 / RATE as f32;
                #[allow(clippy::cast_precision_loss)]
                let offset = seed as f32;
                let env = 0.5 + 0.5 * (t * 4.0 * std::f32::consts::TAU + offset).sin();
                let f0 = 140.0 + 40.0 * (t * 1.3 + offset).sin();
                let voiced = (t * f0 * std::f32::consts::TAU).sin();
                env * 0.25 * (smooth + 0.5 * voiced)
            })
            .collect()
    }

    #[test]
    fn kivo_s_own_voice_is_removed_from_the_microphone() {
        let mut aec = EchoCanceller::new();
        let seconds = 6;
        let played = voice(RATE as usize * seconds, 1);
        // The room: the microphone hears KIVO 40 ms late, quieter.
        let delay = 640;
        let mut mic: Vec<f32> = vec![0.0; delay];
        mic.extend(played.iter().map(|s| s * 0.5));
        mic.truncate(played.len());
        let mut out = Vec::new();
        for (p, m) in played.chunks(320).zip(mic.chunks(320)) {
            aec.played(RATE, p);
            let mut chunk = m.to_vec();
            aec.clean(&mut chunk);
            out.extend(chunk);
        }
        // After adapting (the last two seconds), the echo is much quieter.
        let tail = out.len() - RATE as usize * 2;
        let before = energy(&mic[tail..]);
        let after = energy(&out[tail..]);
        let reduction_db = 10.0 * (before / after.max(1e-12)).log10();
        eprintln!("echo reduced by {reduction_db:.1} dB");
        assert!(reduction_db > 15.0, "{reduction_db} dB");
    }

    #[test]
    fn the_user_is_still_heard_over_kivo() {
        let mut aec = EchoCanceller::new();
        let n = RATE as usize * 6;
        let played = voice(n, 1);
        let user = voice(n, 7).iter().map(|s| s * 0.8).collect::<Vec<_>>();
        let delay = 480;
        let mut out = Vec::new();
        for (i, p) in played.chunks(160).enumerate() {
            aec.played(RATE, p);
            let start = i * 160;
            let mut chunk: Vec<f32> = (start..start + 160)
                .map(|j| {
                    let echo = if j >= delay {
                        played[j - delay] * 0.5
                    } else {
                        0.0
                    };
                    // The user talks during the last two seconds.
                    let talking = if j > n - RATE as usize * 2 {
                        user[j]
                    } else {
                        0.0
                    };
                    echo + talking
                })
                .collect();
            aec.clean(&mut chunk);
            out.extend(chunk);
        }
        let last = n - RATE as usize * 2;
        let kept = energy(&out[last..]) / energy(&user[last..]);
        eprintln!("the user's speech kept at {:.0}% energy", kept * 100.0);
        assert!(kept > 0.2, "{kept}");
    }

    #[test]
    fn speech_after_a_short_cue_is_left_alone() {
        let mut aec = EchoCanceller::new();
        let n = RATE as usize * 3;
        // KIVO plays a 250 ms cue, then nothing; the user talks from the start.
        let cue: Vec<f32> = voice(RATE as usize / 4, 3);
        let user = voice(n, 9);
        let mut out = Vec::new();
        for (i, u) in user.chunks(160).enumerate() {
            let start = i * 160;
            let played: Vec<f32> = (start..start + 160)
                .map(|j| cue.get(j).copied().unwrap_or(0.0))
                .collect();
            aec.played(RATE, &played);
            let mut chunk = u.to_vec();
            aec.clean(&mut chunk);
            out.extend(chunk);
        }
        let from = RATE as usize / 2;
        let kept = energy(&out[from..]) / energy(&user[from..out.len()]);
        eprintln!("speech after the cue kept at {:.0}% energy", kept * 100.0);
        assert!(kept > 0.7, "{kept}");
    }

    #[test]
    fn the_reference_downmixes_and_keeps_only_recent_audio() {
        let reference = Reference::default();
        reference.push(&[0.5, 0.1, 0.5, 0.1], 2, 48_000);
        let (rate, mono) = reference.take();
        assert_eq!(rate, 48_000);
        assert_eq!(mono.len(), 2);
        assert!((mono[0] - 0.3).abs() < 1e-6);
        assert!(reference.take().1.is_empty());
        reference.push(&vec![0.1; 48_000 * 3], 1, 48_000);
        assert_eq!(reference.take().1.len(), 48_000 * REFERENCE_SECONDS);
    }
}
