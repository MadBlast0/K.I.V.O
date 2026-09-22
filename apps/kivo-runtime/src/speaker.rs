//! Playing sound (VOICE §1, §6): KIVO's earcons and its spoken answers go through one mixer, so a
//! cue never cuts a sentence and "stop" silences both. The speaker is opened only when there is
//! something to play and released a few seconds later, so idle costs nothing (plan §128).
//!
//! While a cue plays, voice detection ignores the microphone for the cue's length plus 50 ms
//! (VOICE-25), so KIVO never mistakes its own chime for the user; recognition still hears
//! everything (DECISIONS "Earcon gating"). Echo cancellation arrives with M2.

use kivo_audio::mixer::{DeviceFormat, Mixer};
use kivo_platform::{AudioIo, AudioStream, DeviceId, StreamFormat};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long the speaker stays open after the last sound.
const KEEP_OPEN: Duration = Duration::from_secs(3);
/// Ignore the microphone for this long after a cue ends (VOICE §6).
const GATE_TAIL: Duration = Duration::from_millis(50);
/// The rate cues are generated at.
const CUE_RATE: u32 = 24_000;

/// The sounds KIVO makes (VOICE §6). The set is KIVO's own: soft, rounded tones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cue {
    ListenStart,
    ListenStop,
    Done,
    Error,
    Thinking,
    Hangup,
}

struct Open {
    stream: Box<dyn AudioStream>,
    mixer: Mixer,
    last_used: Instant,
}

pub struct Speaker {
    audio: Arc<dyn AudioIo>,
    device: Mutex<Option<DeviceId>>,
    open: Mutex<Option<Open>>,
    /// The microphone is ignored until this moment (KIVO's own sound is playing).
    gate_until: Mutex<Instant>,
    /// 0–1, how loud KIVO's own voice is right now (the Island's speaking pulse).
    speaking_level: Mutex<f32>,
    enabled: Mutex<bool>,
    volume: Mutex<f32>,
}

impl Speaker {
    pub fn new(audio: Arc<dyn AudioIo>, device: Option<DeviceId>) -> Self {
        Self {
            audio,
            device: Mutex::new(device),
            open: Mutex::new(None),
            gate_until: Mutex::new(Instant::now()),
            speaking_level: Mutex::new(0.0),
            enabled: Mutex::new(true),
            volume: Mutex::new(0.7),
        }
    }

    /// Settings → Sounds: cues on/off and their volume relative to the system (VOICE §6).
    pub fn set_sounds(&self, enabled: bool, volume_percent: u8) {
        *lock(&self.enabled) = enabled;
        *lock(&self.volume) = f32::from(volume_percent.min(100)) / 100.0;
    }

    pub fn set_output_device(&self, device: Option<DeviceId>) {
        *lock(&self.device) = device;
        *lock(&self.open) = None; // reopened on the new device when next needed
    }

    /// The mixer, opening the speaker if needed.
    fn mixer(&self) -> Option<Mixer> {
        let mut open = lock(&self.open);
        if let Some(open) = open.as_mut() {
            open.last_used = Instant::now();
            return Some(open.mixer.clone());
        }
        let device = lock(&self.device).clone();
        // The format isn't known until the stream exists, so the mixer is built inside the
        // callback's closure and shared out through this cell.
        let shared: Arc<Mutex<Option<Mixer>>> = Arc::default();
        let sink = Arc::clone(&shared);
        let stream = self
            .audio
            .open_playback(
                device.as_ref(),
                Box::new(move |out: &mut [f32], format: StreamFormat| {
                    let mut cell = lock(&sink);
                    let mixer = cell.get_or_insert_with(|| {
                        Mixer::new(DeviceFormat {
                            rate: format.sample_rate,
                            channels: format.channels,
                        })
                    });
                    mixer.fill(out);
                }),
            )
            .map_err(|e| tracing::warn!(%e, "couldn't open the speaker"))
            .ok()?;
        // The callback runs as soon as the stream starts, so the mixer appears within a buffer.
        let deadline = Instant::now() + Duration::from_millis(500);
        let mixer = loop {
            if let Some(mixer) = lock(&shared).clone() {
                break mixer;
            }
            if Instant::now() > deadline {
                tracing::warn!("the speaker never asked for audio");
                return None;
            }
            std::thread::sleep(Duration::from_millis(2));
        };
        *open = Some(Open {
            stream,
            mixer: mixer.clone(),
            last_used: Instant::now(),
        });
        Some(mixer)
    }

    /// Plays a cue. Returns when it has been queued, not when it has been heard.
    pub fn cue(&self, cue: Cue) {
        if !*lock(&self.enabled) {
            return;
        }
        let volume = *lock(&self.volume);
        let Some(mixer) = self.mixer() else { return };
        let samples = earcon(cue, volume);
        #[allow(clippy::cast_precision_loss, reason = "a short cue")]
        let length = Duration::from_secs_f32(samples.len() as f32 / CUE_RATE as f32);
        mixer.play_cue(&mixer.to_device(&samples, CUE_RATE));
        *lock(&self.gate_until) = Instant::now() + length + GATE_TAIL;
    }

    /// Queues spoken audio (any rate, mono).
    pub fn speak(&self, pcm: &[f32], rate: u32) {
        let Some(mixer) = self.mixer() else { return };
        mixer.queue_speech(&mixer.to_device(pcm, rate));
    }

    /// Stops speaking with a short fade (cancel → silence, VOICE §7).
    pub fn stop(&self) {
        if let Some(open) = lock(&self.open).as_ref() {
            open.mixer.stop_speech();
        }
        *lock(&self.speaking_level) = 0.0;
    }

    /// True while KIVO's voice (not an earcon) still has audio to play.
    pub fn speaking(&self) -> bool {
        lock(&self.open)
            .as_ref()
            .is_some_and(|o| o.mixer.speech_queued() > 0)
    }

    /// True while something is still to be heard.
    pub fn busy(&self) -> bool {
        lock(&self.open).as_ref().is_some_and(|o| o.mixer.busy())
    }

    /// True while KIVO's own sound is playing and the microphone should be ignored (VOICE-25).
    pub fn muting_microphone(&self) -> bool {
        Instant::now() < *lock(&self.gate_until)
    }

    /// The level of KIVO's own voice (0–1) since the last call.
    pub fn take_speaking_level(&self) -> f32 {
        let level = lock(&self.open)
            .as_ref()
            .map_or(0.0, |o| o.mixer.take_speech_peak());
        let scaled = (level * 4.0).clamp(0.0, 1.0);
        *lock(&self.speaking_level) = scaled;
        scaled
    }

    /// Closes the speaker when nothing has played for a while (call about once a second).
    pub fn release_if_idle(&self) {
        let mut open = lock(&self.open);
        let idle = open
            .as_ref()
            .is_some_and(|o| !o.mixer.busy() && o.last_used.elapsed() > KEEP_OPEN);
        if idle {
            if let Some(open) = open.take() {
                drop(open.stream);
            }
            tracing::debug!("speaker released");
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// One note: a soft, marimba-like tone (a sine with a quiet second harmonic and a quick decay).
fn note(out: &mut Vec<f32>, hz: f32, seconds: f32, gain: f32) {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let samples = (CUE_RATE as f32 * seconds) as usize;
    for i in 0..samples {
        #[allow(clippy::cast_precision_loss, reason = "sample index")]
        let t = i as f32 / CUE_RATE as f32;
        // Fast attack, exponential decay and a short release: rounded, never clicky.
        let attack = (t / 0.008).min(1.0);
        let decay = (-t * 7.0).exp();
        #[allow(clippy::cast_precision_loss, reason = "sample index")]
        let release = ((samples - i) as f32 / (CUE_RATE as f32 * 0.012)).min(1.0);
        let wave = (t * hz * std::f32::consts::TAU).sin()
            + 0.18 * (t * hz * 2.0 * std::f32::consts::TAU).sin();
        out.push(wave * 0.5 * attack * decay * release * gain);
    }
}

fn silence(out: &mut Vec<f32>, seconds: f32) {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let samples = (CUE_RATE as f32 * seconds) as usize;
    out.resize(out.len() + samples, 0.0);
}

/// KIVO's Soft sound set, generated rather than shipped as files (VOICE §6). Every cue is under
/// 300 ms, so wake → audio stays inside the 150 ms budget.
pub fn earcon(cue: Cue, gain: f32) -> Vec<f32> {
    let mut out = Vec::new();
    match cue {
        // Rising two notes: "I'm listening".
        Cue::ListenStart => {
            note(&mut out, 587.33, 0.09, gain);
            silence(&mut out, 0.01);
            note(&mut out, 880.00, 0.16, gain);
        }
        // One soft note: "I stopped listening".
        Cue::ListenStop => note(&mut out, 587.33, 0.16, gain),
        // A gentle confirmation.
        Cue::Done => {
            note(&mut out, 783.99, 0.07, gain * 0.9);
            silence(&mut out, 0.01);
            note(&mut out, 1046.50, 0.14, gain * 0.9);
        }
        // Low descending two notes: something went wrong.
        Cue::Error => {
            note(&mut out, 392.00, 0.10, gain);
            silence(&mut out, 0.01);
            note(&mut out, 293.66, 0.18, gain);
        }
        // A quiet single tick, only after a second of thinking.
        Cue::Thinking => note(&mut out, 659.25, 0.08, gain * 0.5),
        // The conversation ended.
        Cue::Hangup => {
            note(&mut out, 587.33, 0.07, gain * 0.7);
            silence(&mut out, 0.01);
            note(&mut out, 440.00, 0.14, gain * 0.7);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seconds(samples: usize) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        let s = samples as f32 / CUE_RATE as f32;
        s
    }

    #[test]
    fn every_cue_is_short_audible_and_starts_and_ends_quietly() {
        for cue in [
            Cue::ListenStart,
            Cue::ListenStop,
            Cue::Done,
            Cue::Error,
            Cue::Thinking,
            Cue::Hangup,
        ] {
            let samples = earcon(cue, 1.0);
            let length = seconds(samples.len());
            assert!(
                length < 0.3,
                "{cue:?} is {length}s (VOICE §6: under 300 ms)"
            );
            assert!(
                samples.iter().any(|s| s.abs() > 0.1),
                "{cue:?} is inaudible"
            );
            assert!(samples.iter().all(|s| s.abs() <= 1.0), "{cue:?} clips");
            assert!(samples[0].abs() < 0.01, "{cue:?} starts with a click");
            assert!(
                samples[samples.len() - 1].abs() < 0.05,
                "{cue:?} ends with a click"
            );
        }
    }

    #[test]
    fn the_volume_setting_scales_the_cues() {
        let loud = earcon(Cue::Done, 1.0);
        let quiet = earcon(Cue::Done, 0.25);
        let peak = |s: &[f32]| s.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
        assert!((peak(&loud) * 0.25 - peak(&quiet)).abs() < 1e-6);
    }

    #[test]
    fn a_muted_speaker_plays_nothing_and_never_gates_the_microphone() {
        let speaker = Speaker::new(
            Arc::new(kivo_testkit::FakeAudio::with_clip(Vec::new())),
            None,
        );
        speaker.set_sounds(false, 70);
        speaker.cue(Cue::ListenStart);
        assert!(!speaker.muting_microphone());
        assert!(!speaker.busy());
    }
}
