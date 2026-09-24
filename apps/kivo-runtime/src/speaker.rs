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
/// −12 dB.
const DUCKED_GAIN: f32 = 0.25;
/// Ignore the microphone for this long after a cue ends (VOICE §6).
const GATE_TAIL: Duration = Duration::from_millis(50);
use crate::sounds::{CUE_RATE, earcon};

/// The sounds KIVO makes (VOICE §6); the sets live in `sounds`.
pub use kivo_core::config::SoundCue as Cue;
use kivo_core::config::{SoundSet, Sounds};

struct Open {
    stream: Box<dyn AudioStream>,
    mixer: Mixer,
    last_used: Instant,
}

/// What the cues were last rendered from: the set, the cues with the user's own sounds, and the
/// notification set.
type RenderedFrom = (SoundSet, Vec<Cue>, Option<SoundSet>);

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
    /// Cues switched off in Settings → Sounds.
    off: Mutex<Vec<Cue>>,
    set: Mutex<SoundSet>,
    /// The folder with the user's imported cues (VOICE-28), and which cues use them, and the
    /// notification sound's own set: with the set, what `cues` was rendered from.
    custom_dir: Mutex<Option<std::path::PathBuf>>,
    rendered_from: Mutex<Option<RenderedFrom>>,
    /// Called when sound starts, so a sleeping level loop wakes to pulse the Island.
    on_sound: Mutex<Option<Box<dyn Fn() + Send>>>,
    /// Every cue of the current set, rendered once at full level (VOICE-24: pre-decoded, so a
    /// cue plays at once).
    cues: Mutex<Vec<(Cue, Vec<f32>)>>,
    /// Everything played, for echo cancellation (VOICE-30).
    reference: kivo_audio::echo::Reference,
    /// Bumped when the output device changes, so the echo path is found again (headphones on
    /// or off).
    device_changes: std::sync::atomic::AtomicU64,
}

impl Speaker {
    /// The audio devices (the Voice page's microphone and speaker choices).
    pub fn audio(&self) -> Arc<dyn AudioIo> {
        Arc::clone(&self.audio)
    }

    pub fn new(audio: Arc<dyn AudioIo>, device: Option<DeviceId>) -> Self {
        Self {
            audio,
            device: Mutex::new(device),
            open: Mutex::new(None),
            gate_until: Mutex::new(Instant::now()),
            speaking_level: Mutex::new(0.0),
            enabled: Mutex::new(true),
            volume: Mutex::new(0.7),
            off: Mutex::new(Vec::new()),
            set: Mutex::new(SoundSet::Soft),
            custom_dir: Mutex::new(None),
            rendered_from: Mutex::new(None),
            on_sound: Mutex::new(None),
            cues: Mutex::new(render(SoundSet::Soft)),
            reference: kivo_audio::echo::Reference::default(),
            device_changes: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Registers what to call when sound starts (the detection thread's wake-up).
    pub fn on_sound(&self, wake: impl Fn() + Send + 'static) {
        *lock(&self.on_sound) = Some(Box::new(wake));
    }

    fn sounded(&self) {
        if let Some(wake) = lock(&self.on_sound).as_ref() {
            wake();
        }
    }

    /// Settings → Sounds: cues on/off and their volume relative to the system (VOICE §6).
    pub fn set_sounds(&self, enabled: bool, volume_percent: u8) {
        *lock(&self.enabled) = enabled;
        *lock(&self.volume) = f32::from(volume_percent.min(100)) / 100.0;
    }

    /// Settings → Sounds as a whole: on/off, volume, the set and the cues switched off
    /// (VOICE-27).
    pub fn configure(&self, sounds: &Sounds) {
        self.set_sounds(sounds.enabled, sounds.volume);
        lock(&self.off).clone_from(&sounds.off);
        *lock(&self.set) = sounds.set;
        let wanted = (sounds.set, sounds.custom.clone(), sounds.notification_set);
        let mut rendered = lock(&self.rendered_from);
        if rendered.as_ref() != Some(&wanted) {
            let dir = lock(&self.custom_dir).clone();
            *lock(&self.cues) = render_with(sounds, dir.as_deref());
            *rendered = Some(wanted);
        }
    }

    /// Where imported cues are kept, once set.
    pub fn custom_dir(&self) -> Option<std::path::PathBuf> {
        lock(&self.custom_dir).clone()
    }

    /// Where imported cues are kept (`%APPDATA%\KIVO\sounds`).
    pub fn set_custom_dir(&self, dir: std::path::PathBuf) {
        *lock(&self.custom_dir) = Some(dir);
        // Rendered again on the next `configure`.
        *lock(&self.rendered_from) = None;
    }

    /// Plays an imported sound as it would play for a cue (Settings → Sounds' Preview).
    pub fn preview_custom(&self, samples: &[f32], rate: u32) {
        let volume = *lock(&self.volume);
        self.play(&crate::sounds::prepare(samples, rate, volume));
    }

    /// Plays a cue as the current settings have it, imported sound included (Settings → Sounds'
    /// per-cue Preview). It plays even when sounds are off.
    pub fn preview_current(&self, cue: Cue) {
        let volume = *lock(&self.volume);
        let samples: Vec<f32> = lock(&self.cues)
            .iter()
            .find(|(c, _)| *c == cue)
            .map(|(_, pcm)| pcm.iter().map(|s| s * volume).collect())
            .unwrap_or_default();
        self.play(&samples);
    }

    /// Plays a cue from any set without changing the settings (the set picker's preview). It
    /// plays even when sounds are off: the user asked to hear it.
    pub fn preview(&self, set: SoundSet, cue: Cue) {
        let volume = *lock(&self.volume);
        let samples: Vec<f32> = earcon(set, cue, volume);
        self.play(&samples);
    }

    pub fn set_output_device(&self, device: Option<DeviceId>) {
        let mut current = lock(&self.device);
        if *current == device {
            return;
        }
        *current = device;
        *lock(&self.open) = None; // reopened on the new device when next needed
        self.device_changes
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// How many times the output device has changed (see `device_changes`).
    pub fn device_changes(&self) -> u64 {
        self.device_changes
            .load(std::sync::atomic::Ordering::Relaxed)
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
        mixer.set_reference(self.reference.clone());
        *open = Some(Open {
            stream,
            mixer: mixer.clone(),
            last_used: Instant::now(),
        });
        Some(mixer)
    }

    /// Plays a cue. Returns when it has been queued, not when it has been heard.
    pub fn cue(&self, cue: Cue) {
        if !*lock(&self.enabled) || lock(&self.off).contains(&cue) {
            return;
        }
        let volume = *lock(&self.volume);
        let samples: Vec<f32> = lock(&self.cues)
            .iter()
            .find(|(c, _)| *c == cue)
            .map(|(_, pcm)| pcm.iter().map(|s| s * volume).collect())
            .unwrap_or_default();
        self.play(&samples);
    }

    fn play(&self, samples: &[f32]) {
        let Some(mixer) = self.mixer() else { return };
        #[allow(clippy::cast_precision_loss, reason = "a short cue")]
        let length = Duration::from_secs_f32(samples.len() as f32 / CUE_RATE as f32);
        mixer.play_cue(&mixer.to_device(samples, CUE_RATE));
        *lock(&self.gate_until) = Instant::now() + length + GATE_TAIL;
        self.sounded();
    }

    /// Queues spoken audio (any rate, mono).
    pub fn speak(&self, pcm: &[f32], rate: u32) {
        let Some(mixer) = self.mixer() else { return };
        if mixer.speech_queued() == 0 {
            // A new reply: at full level, even if the last one was ducked (VOICE-31).
            mixer.set_speech_gain(1.0);
        }
        mixer.queue_speech(&mixer.to_device(pcm, rate));
        self.sounded();
    }

    /// What KIVO plays, for echo cancellation (VOICE-30).
    pub fn reference(&self) -> kivo_audio::echo::Reference {
        self.reference.clone()
    }

    /// Barge-in (VOICE-31): KIVO's voice drops 12 dB while the user may be talking over it, and
    /// comes back if they weren't.
    pub fn duck(&self, ducked: bool) {
        if let Some(open) = lock(&self.open).as_ref() {
            open.mixer
                .set_speech_gain(if ducked { DUCKED_GAIN } else { 1.0 });
        }
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

    /// Whether the playback stream is open (it closes after going quiet for a while).
    pub fn is_open(&self) -> bool {
        lock(&self.open).is_some()
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

/// A set's cues, rendered at full level.
fn render(set: SoundSet) -> Vec<(Cue, Vec<f32>)> {
    Cue::ALL.iter().map(|&c| (c, earcon(set, c, 1.0))).collect()
}

/// The cues as Settings → Sounds has them (VOICE-28): the set's, the user's imported sound where
/// there is one, and the notification from its own set. An imported file that can't be read falls
/// back to the set's cue.
fn render_with(sounds: &Sounds, dir: Option<&std::path::Path>) -> Vec<(Cue, Vec<f32>)> {
    Cue::ALL
        .iter()
        .map(|&c| {
            let imported = (sounds.custom.contains(&c))
                .then_some(dir)
                .flatten()
                .and_then(|d| crate::sounds::custom_file(d, c))
                .and_then(|file| std::fs::read(file).ok())
                .and_then(|bytes| crate::sounds::decode(&bytes).ok())
                .map(|(pcm, rate)| crate::sounds::prepare(&pcm, rate, 1.0));
            let set = match (c, sounds.notification_set) {
                (Cue::Notification, Some(own)) => own,
                _ => sounds.set,
            };
            (c, imported.unwrap_or_else(|| earcon(set, c, 1.0)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_imported_cue_and_a_separate_notification_sound_are_used() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::copy(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/chime.ogg"),
            dir.path().join("listen-start.ogg"),
        )
        .unwrap();
        let sounds = Sounds {
            set: SoundSet::Wood,
            custom: vec![Cue::ListenStart, Cue::Done],
            notification_set: Some(SoundSet::Glass),
            ..Sounds::default()
        };
        let cues = render_with(&sounds, Some(dir.path()));
        let get = |cue: Cue| cues.iter().find(|(c, _)| *c == cue).unwrap().1.clone();
        let (pcm, rate) =
            crate::sounds::decode(&std::fs::read(dir.path().join("listen-start.ogg")).unwrap())
                .unwrap();
        assert_eq!(
            get(Cue::ListenStart),
            crate::sounds::prepare(&pcm, rate, 1.0)
        );
        // "Done" is marked custom but has no file: the set's own sound.
        assert_eq!(get(Cue::Done), earcon(SoundSet::Wood, Cue::Done, 1.0));
        assert_eq!(
            get(Cue::Notification),
            earcon(SoundSet::Glass, Cue::Notification, 1.0)
        );
        assert_eq!(get(Cue::Error), earcon(SoundSet::Wood, Cue::Error, 1.0));
    }

    #[test]
    fn switched_off_cues_are_silent_and_the_set_can_change() {
        let speaker = Speaker::new(
            Arc::new(kivo_testkit::FakeAudio::with_clip(Vec::new())),
            None,
        );
        speaker.configure(&Sounds {
            off: vec![Cue::Done],
            set: SoundSet::Glass,
            ..Sounds::default()
        });
        speaker.cue(Cue::Done);
        assert!(!speaker.muting_microphone(), "an off cue doesn't play");
        let glass = lock(&speaker.cues).clone();
        assert_eq!(glass, render(SoundSet::Glass));
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
