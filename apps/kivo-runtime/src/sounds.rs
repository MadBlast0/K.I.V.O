//! KIVO's sound sets (VOICE §6, VOICE-26/27), generated rather than shipped as files. Every cue is
//! a short pattern of notes; each set plays those patterns with its own timbre:
//!
//! | Set | Character |
//! |---|---|
//! | Soft (default) | rounded marimba-like tones |
//! | Glass | bright, airy bells |
//! | Pulse | short electronic blips |
//! | Wood | warm, percussive |
//! | Minimal | single quiet clicks |
//!
//! Every cue in every set is under 300 ms, starts and ends without a click, and plays at the
//! volume set relative to the system's.

use kivo_core::config::{SoundCue as Cue, SoundSet};

/// The rate cues are generated at.
pub const CUE_RATE: u32 = 24_000;
const GAP: f32 = 0.01;

/// One note of a pattern: pitch, length, loudness.
type Note = (f32, f32, f32);

/// What each cue says, as notes (the same in every set, so a cue keeps its meaning).
fn pattern(cue: Cue) -> &'static [Note] {
    match cue {
        // Rising two notes: "I'm listening".
        Cue::ListenStart => &[(587.33, 0.09, 1.0), (880.00, 0.16, 1.0)],
        // One soft note: "I stopped listening".
        Cue::ListenStop => &[(587.33, 0.16, 1.0)],
        // A gentle confirmation.
        Cue::Done => &[(783.99, 0.07, 0.9), (1046.50, 0.14, 0.9)],
        // Low descending two notes: something went wrong.
        Cue::Error => &[(392.00, 0.10, 1.0), (293.66, 0.18, 1.0)],
        // A quiet single tick, only after a second of thinking.
        Cue::Thinking => &[(659.25, 0.08, 0.5)],
        // The conversation ended.
        Cue::Hangup => &[(587.33, 0.07, 0.7), (440.00, 0.14, 0.7)],
        // Rising, and higher than "listening": "this needs your answer".
        Cue::Question => &[(659.25, 0.08, 0.9), (1108.73, 0.15, 0.9)],
        // A soft tick: approved.
        Cue::Approved => &[(1318.51, 0.06, 0.6)],
        // A short descending tone: cancelled.
        Cue::Cancelled => &[(659.25, 0.06, 0.7), (493.88, 0.12, 0.7)],
        // Three light notes: something for you.
        Cue::Notification => &[
            (880.00, 0.07, 0.8),
            (1174.66, 0.07, 0.8),
            (1567.98, 0.10, 0.8),
        ],
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn samples(seconds: f32) -> usize {
    (CUE_RATE as f32 * seconds) as usize
}

/// One note in the set's timbre.
fn note(out: &mut Vec<f32>, set: SoundSet, hz: f32, seconds: f32, gain: f32) {
    use std::f32::consts::TAU;
    // Each set's pitch, length, decay and attack (seconds).
    let (hz, seconds, decay, attack) = match set {
        SoundSet::Soft | SoundSet::Custom => (hz, seconds, 7.0, 0.008),
        SoundSet::Glass => (hz * 1.5, seconds, 9.0, 0.004),
        SoundSet::Pulse => (hz, seconds * 0.7, 12.0, 0.003),
        SoundSet::Wood => (hz * 0.75, seconds.min(0.12), 25.0, 0.002),
        SoundSet::Minimal => (2_000.0 * hz / 587.33, 0.025, 60.0, 0.001),
    };
    let n = samples(seconds);
    let release_len = CUE_RATE as f32 * 0.012;
    for i in 0..n {
        #[allow(clippy::cast_precision_loss, reason = "sample index")]
        let t = i as f32 / CUE_RATE as f32;
        let phase = t * hz * TAU;
        let wave = match set {
            // A sine with a quiet second harmonic.
            SoundSet::Soft | SoundSet::Custom => phase.sin() + 0.18 * (2.0 * phase).sin(),
            // Bell partials.
            SoundSet::Glass => {
                phase.sin()
                    + 0.35 * (2.76 * phase).sin() * (-t * 20.0).exp()
                    + 0.2 * (5.4 * phase).sin() * (-t * 35.0).exp()
            }
            // A soft square: the first odd harmonics.
            SoundSet::Pulse => phase.sin() + (3.0 * phase).sin() / 3.0 + (5.0 * phase).sin() / 5.0,
            // A damped block with a woody overtone.
            SoundSet::Wood => phase.sin() + 0.4 * (2.4 * phase).sin() * (-t * 60.0).exp(),
            SoundSet::Minimal => phase.sin(),
        };
        let level = (t / attack).min(1.0) * (-t * decay).exp();
        #[allow(clippy::cast_precision_loss, reason = "sample index")]
        let release = ((n - i) as f32 / release_len).min(1.0);
        let loudness = if set == SoundSet::Minimal { 0.35 } else { 0.5 };
        out.push(wave * loudness * level * release * gain);
    }
}

/// A cue in a set, at `gain` (0–1). Custom sets use Soft until imported sounds exist.
pub fn earcon(set: SoundSet, cue: Cue, gain: f32) -> Vec<f32> {
    let mut out = Vec::new();
    for (i, &(hz, seconds, level)) in pattern(cue).iter().enumerate() {
        if i > 0 {
            out.resize(out.len() + samples(GAP), 0.0);
        }
        note(&mut out, set, hz, seconds, gain * level);
    }
    // Normalize so a timbre with extra partials never clips.
    let peak = out.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
    if peak > 0.95 {
        out.iter_mut().for_each(|v| *v *= 0.95 / peak);
    }
    out
}

/// The sets people can pick (Custom arrives with imported sounds).
pub const SETS: [SoundSet; 5] = [
    SoundSet::Soft,
    SoundSet::Glass,
    SoundSet::Pulse,
    SoundSet::Wood,
    SoundSet::Minimal,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::cast_precision_loss)]
    fn seconds(samples: usize) -> f32 {
        samples as f32 / CUE_RATE as f32
    }

    #[test]
    fn every_cue_in_every_set_is_short_audible_and_click_free() {
        for set in SETS {
            for cue in Cue::ALL {
                let s = earcon(set, cue, 1.0);
                let length = seconds(s.len());
                assert!(length < 0.3, "{set:?}/{cue:?} is {length}s (VOICE §6)");
                assert!(
                    s.iter().any(|v| v.abs() > 0.05),
                    "{set:?}/{cue:?} is inaudible"
                );
                assert!(s.iter().all(|v| v.abs() <= 1.0), "{set:?}/{cue:?} clips");
                assert!(s[0].abs() < 0.01, "{set:?}/{cue:?} starts with a click");
                assert!(
                    s[s.len() - 1].abs() < 0.05,
                    "{set:?}/{cue:?} ends with a click"
                );
            }
        }
    }

    #[test]
    fn the_cues_of_a_set_sound_different_from_each_other() {
        for set in SETS {
            for (i, a) in Cue::ALL.iter().enumerate() {
                for b in &Cue::ALL[i + 1..] {
                    assert_ne!(
                        earcon(set, *a, 1.0),
                        earcon(set, *b, 1.0),
                        "{set:?}: {a:?} = {b:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn sets_sound_different_from_each_other() {
        for (i, a) in SETS.iter().enumerate() {
            for b in &SETS[i + 1..] {
                assert_ne!(earcon(*a, Cue::Done, 1.0), earcon(*b, Cue::Done, 1.0));
            }
        }
    }

    #[test]
    fn the_volume_scales_the_cues() {
        let loud = earcon(SoundSet::Soft, Cue::Done, 1.0);
        let quiet = earcon(SoundSet::Soft, Cue::Done, 0.25);
        let peak = |s: &[f32]| s.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
        assert!((peak(&loud) * 0.25 - peak(&quiet)).abs() < 1e-6);
    }
}
