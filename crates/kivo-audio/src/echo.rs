//! Echo cancellation (VOICE §1, VOICE-30): KIVO removes its own voice and sounds from the
//! microphone, so it can hear the user over itself (barge-in, the stop words) without mistaking
//! itself for them. The reference is KIVO's output mix, tapped where the mixer fills the speaker's
//! buffer; the canceller is WebRTC's AEC3 (the pure-Rust `sonora` port, BSD-3-Clause), which
//! estimates the delay between what was played and what the microphone heard by itself.
//!
//! Only KIVO's own output is in the reference: other apps' audio is not removed (the OS AEC,
//! where a device has one, covers everything).
//!
//! With headphones there is no echo to remove, and AEC3 then gates the first half-second of the
//! user's speech while KIVO talks (measured: 1% of its energy kept, against 40–60% with a
//! speaker's echo), which would swallow a short barge-in like "stop" or "open Chrome". So an
//! `EchoPath` detector compares what KIVO played with what the microphone heard while KIVO spoke;
//! once it finds no echo path, the canceller steps aside until the output changes.

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

/// Frames of history the echo-path detector compares (1.5 s).
const PATH_FRAMES: usize = 150;
/// Largest render-to-capture delay it looks for (250 ms).
const PATH_LAGS: usize = 25;
/// Frames of KIVO's sound it needs before deciding (0.5 s).
const PATH_ACTIVE: usize = 50;
/// How often it looks again until it has decided (every 100 ms).
const PATH_EVERY: usize = 10;
/// A frame counts as KIVO playing above this mean power (about −50 dBFS).
const PATH_ACTIVE_POWER: f32 = 1e-5;

/// Does KIVO's own sound reach the microphone? It compares the loudness envelopes of what was
/// played and what the microphone heard, 10 ms at a time, at every delay up to 250 ms: speakers
/// make them rise and fall together; headphones don't, and the microphone is then far quieter
/// than what was played.
#[derive(Default)]
pub struct EchoPath {
    far: VecDeque<f32>,
    near: VecDeque<f32>,
    since_check: usize,
    verdict: Option<bool>,
}

impl EchoPath {
    /// One 10 ms frame of what was played.
    pub fn far(&mut self, frame: &[f32]) {
        Self::push(&mut self.far, frame);
        self.since_check += 1;
        if self.verdict.is_none() && self.since_check >= PATH_EVERY {
            self.since_check = 0;
            self.decide();
        }
    }

    /// One 10 ms frame of what the microphone heard.
    pub fn near(&mut self, frame: &[f32]) {
        Self::push(&mut self.near, frame);
    }

    fn push(frames: &mut VecDeque<f32>, frame: &[f32]) {
        #[allow(clippy::cast_precision_loss, reason = "a frame's mean power")]
        let power = frame.iter().map(|s| s * s).sum::<f32>() / frame.len().max(1) as f32;
        frames.push_back(power);
        if frames.len() > PATH_FRAMES {
            frames.pop_front();
        }
    }

    /// `Some(true)`: there is an echo to cancel; `Some(false)`: there isn't (headphones);
    /// `None`: not sure yet.
    pub fn verdict(&self) -> Option<bool> {
        self.verdict
    }

    /// Starts over (the output device changed).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn decide(&mut self) {
        let n = self.far.len().min(self.near.len());
        if n < PATH_LAGS + PATH_ACTIVE {
            return;
        }
        let far: Vec<f32> = self.far.iter().skip(self.far.len() - n).copied().collect();
        let near: Vec<f32> = self
            .near
            .iter()
            .skip(self.near.len() - n)
            .copied()
            .collect();
        // Frames (after the largest lag) where KIVO was audible.
        let active: Vec<usize> = (PATH_LAGS..n)
            .filter(|&t| far[t] > PATH_ACTIVE_POWER)
            .collect();
        if active.len() < PATH_ACTIVE {
            return;
        }
        #[allow(clippy::cast_precision_loss, reason = "means over frames")]
        let mean = |v: &[f32], idx: &mut dyn Iterator<Item = usize>| {
            let (sum, count) = idx.fold((0.0_f32, 0_usize), |(s, c), i| (s + v[i], c + 1));
            sum / count.max(1) as f32
        };
        let far_power = mean(&far, &mut active.iter().copied());
        let near_power = mean(&near, &mut active.iter().copied());
        // The microphone hears almost nothing while KIVO plays: no path worth cancelling.
        if near_power < far_power * 1e-4 {
            self.verdict = Some(false);
            return;
        }
        let log = |p: f32| (p + 1e-10).log10();
        let best = (0..=PATH_LAGS)
            .map(|lag| {
                let pairs: Vec<(f32, f32)> = active
                    .iter()
                    .map(|&t| (log(far[t - lag]), log(near[t])))
                    .collect();
                correlation(&pairs)
            })
            .fold(f32::MIN, f32::max);
        if best >= 0.4 {
            self.verdict = Some(true);
        } else if best < 0.15 {
            self.verdict = Some(false);
        }
    }
}

/// Pearson correlation of the pairs (0 when either side doesn't vary).
fn correlation(pairs: &[(f32, f32)]) -> f32 {
    #[allow(clippy::cast_precision_loss, reason = "a mean")]
    let n = pairs.len().max(1) as f32;
    let (mx, my) = pairs
        .iter()
        .fold((0.0, 0.0), |(x, y), (a, b)| (x + a / n, y + b / n));
    let (mut sxy, mut sxx, mut syy) = (0.0_f32, 0.0_f32, 0.0_f32);
    for (a, b) in pairs {
        sxy += (a - mx) * (b - my);
        sxx += (a - mx) * (a - mx);
        syy += (b - my) * (b - my);
    }
    if sxx <= f32::EPSILON || syy <= f32::EPSILON {
        return 0.0;
    }
    sxy / (sxx * syy).sqrt()
}

/// Frames over which the echo reduction is judged (0.5 s of audible echo).
const SETTLE_FRAMES: usize = 50;
/// The canceller has learned the room once it takes this much off the echo (10 dB).
const SETTLED_REDUCTION: f32 = 10.0;

/// AEC3 on 16 kHz mono, fed any amount of audio at a time.
pub struct EchoCanceller {
    apm: AudioProcessing,
    resampler: Option<(u32, RateConverter)>,
    render: Vec<f32>,
    capture: Vec<f32>,
    cleaned: Vec<f32>,
    frame_out: Vec<f32>,
    delay_ms: i32,
    path: EchoPath,
    /// Microphone and cleaned power of recent frames with echo in them.
    settle: VecDeque<(f32, f32)>,
    /// It has removed at least `SETTLED_REDUCTION` of the echo (sticky until the output changes).
    settled: bool,
    /// Microphone and cleaned power of the last `clean` call.
    last: (f32, f32),
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
            path: EchoPath::default(),
            settle: VecDeque::new(),
            settled: false,
            last: (0.0, 0.0),
        }
    }

    /// Whether KIVO's sound reaches the microphone (`None` until it has played long enough).
    pub fn echo_path(&self) -> Option<bool> {
        self.path.verdict()
    }

    /// The output device changed: the echo path is found again.
    pub fn reset_echo_path(&mut self) {
        self.path.reset();
        self.settle.clear();
        self.settled = false;
    }

    /// How much quieter the last cleaned audio was than the microphone, in dB. Echo alone comes
    /// out 20 dB or more quieter once the canceller has settled; the user's voice mostly gets
    /// through, so a small reduction means someone is talking (double talk).
    pub fn last_reduction_db(&self) -> f32 {
        let (heard, cleaned) = self.last;
        if heard <= 1e-9 {
            return 0.0;
        }
        10.0 * (heard / cleaned.max(1e-12)).log10()
    }

    /// Whether what comes out can be trusted to be the user, not KIVO (barge-in, VOICE-31): with
    /// no echo path at once; with one, once the canceller has learned the room and removes at
    /// least 10 dB of KIVO's voice.
    pub fn settled(&self) -> bool {
        match self.path.verdict() {
            Some(false) => true,
            Some(true) => self.settled,
            None => false,
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
            self.path.far(frame);
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
        let power = |s: &[f32]| s.iter().map(|v| v * v).sum::<f32>();
        let heard = power(audio);
        self.capture.extend_from_slice(audio);
        audio.clear();
        let mut offset = 0;
        while self.capture.len() - offset >= FRAME {
            let frame = &self.capture[offset..offset + FRAME];
            self.path.near(frame);
            if self.path.verdict() == Some(false) {
                // No echo to remove (headphones): the microphone is passed on untouched.
                audio.extend_from_slice(frame);
                offset += FRAME;
                continue;
            }
            self.cleaned.resize(FRAME, 0.0);
            let _ = self.apm.set_stream_delay_ms(self.delay_ms);
            if self
                .apm
                .process_capture_f32(&[frame], &mut [&mut self.cleaned[..]])
                .is_ok()
            {
                if !self.settled {
                    let power = |s: &[f32]| s.iter().map(|v| v * v).sum::<f32>();
                    let heard = power(frame);
                    // Only frames with something in them say how much echo was removed.
                    if heard > 1e-4 {
                        self.settle.push_back((heard, power(&self.cleaned)));
                        if self.settle.len() > SETTLE_FRAMES {
                            self.settle.pop_front();
                        }
                        if self.settle.len() == SETTLE_FRAMES {
                            let (before, after) = self
                                .settle
                                .iter()
                                .fold((0.0, 0.0), |(b, a), (h, c)| (b + h, a + c));
                            let reduction = 10.0 * (before / after.max(1e-12)).log10();
                            self.settled = reduction >= SETTLED_REDUCTION;
                        }
                    }
                }
                audio.extend_from_slice(&self.cleaned);
            } else {
                audio.extend_from_slice(frame);
            }
            offset += FRAME;
        }
        self.capture.drain(..offset);
        self.last = (heard, power(audio));
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

    /// Drives the canceller with KIVO playing `played` and the microphone hearing `mic` (both
    /// 16 kHz), 10 ms at a time; returns what came out.
    fn run(aec: &mut EchoCanceller, played: &[f32], mic: &[f32]) -> Vec<f32> {
        let mut out = Vec::new();
        for (p, m) in played.chunks(160).zip(mic.chunks(160)) {
            aec.played(RATE, p);
            let mut chunk = m.to_vec();
            aec.clean(&mut chunk);
            out.extend(chunk);
        }
        out
    }

    #[test]
    fn with_headphones_the_user_s_first_words_are_not_swallowed() {
        let mut aec = EchoCanceller::new();
        let n = RATE as usize * 4;
        let played = voice(n, 1);
        // Headphones: the microphone never hears KIVO. The user starts talking after 2 s.
        let user = voice(n, 5);
        let start = RATE as usize * 2;
        let mic: Vec<f32> = (0..n)
            .map(|j| if j >= start { user[j] } else { 0.0 })
            .collect();
        let out = run(&mut aec, &played, &mic);
        assert_eq!(aec.echo_path(), Some(false), "no echo path");
        assert!(aec.settled(), "nothing to learn");
        let onset = RATE as usize / 2;
        let kept = energy(&out[start..start + onset]) / energy(&mic[start..start + onset]);
        eprintln!("the first half-second kept at {:.0}%", kept * 100.0);
        assert!(kept > 0.9, "{kept}");
    }

    #[test]
    fn a_speaker_s_echo_is_found_and_cancelled() {
        let mut aec = EchoCanceller::new();
        let n = RATE as usize * 4;
        let played = voice(n, 1);
        let delay = 640;
        let mic: Vec<f32> = (0..n)
            .map(|j| {
                if j >= delay {
                    played[j - delay] * 0.4
                } else {
                    0.0
                }
            })
            .collect();
        let out = run(&mut aec, &played, &mic);
        assert_eq!(aec.echo_path(), Some(true), "an echo path");
        assert!(aec.settled(), "it has learned the room");
        let tail = n - RATE as usize;
        assert!(
            energy(&out[tail..]) < energy(&mic[tail..]) * 0.1,
            "still cancelled"
        );
        aec.reset_echo_path();
        assert_eq!(aec.echo_path(), None);
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
