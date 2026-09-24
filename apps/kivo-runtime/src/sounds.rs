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

/// The longest imported cue KIVO plays; anything longer is cut, with a short fade.
pub const MAX_CUSTOM_SECONDS: f32 = 2.0;
/// The largest sound file KIVO imports.
pub const MAX_CUSTOM_BYTES: usize = 2 * 1024 * 1024;

/// A `.wav` (PCM 8/16/24/32-bit or 32-bit float, any channels) or `.ogg` (Vorbis) file as mono
/// samples and their rate (VOICE-28, the Custom set).
pub fn decode(bytes: &[u8]) -> Result<(Vec<f32>, u32), String> {
    if bytes.starts_with(b"OggS") {
        return decode_ogg(bytes);
    }
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a WAV or OGG file".into());
    }
    let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32_at =
        |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    let (mut format, mut channels, mut rate, mut bits) = (0u16, 0u16, 0u32, 0u16);
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let len = u32_at(at + 4) as usize;
        let body = at + 8;
        match &bytes[at..at + 4] {
            b"fmt " if body + 16 <= bytes.len() => {
                format = u16_at(body);
                channels = u16_at(body + 2).max(1);
                rate = u32_at(body + 4);
                bits = u16_at(body + 14);
                // WAVE_FORMAT_EXTENSIBLE: the real format is in the sub-format GUID.
                if format == 0xFFFE && body + 26 <= bytes.len() {
                    format = u16_at(body + 24);
                }
            }
            b"data" => {
                let end = (body + len).min(bytes.len());
                let data = &bytes[body..end];
                let width = usize::from(bits / 8).max(1);
                let frame = width * usize::from(channels);
                let sample = |b: &[u8]| -> f32 {
                    match (format, bits) {
                        (3, 32) => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                        (_, 8) => (f32::from(b[0]) - 128.0) / 128.0,
                        (_, 16) => f32::from(i16::from_le_bytes([b[0], b[1]])) / 32_768.0,
                        #[allow(clippy::cast_precision_loss, reason = "24-bit samples")]
                        (_, 24) => {
                            (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0
                        }
                        #[allow(clippy::cast_precision_loss, reason = "32-bit samples")]
                        (_, 32) => {
                            i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0
                        }
                        _ => 0.0,
                    }
                };
                if !matches!(format, 1 | 3) || !matches!(bits, 8 | 16 | 24 | 32) || rate == 0 {
                    return Err("an unsupported WAV encoding".into());
                }
                let samples = data
                    .chunks_exact(frame)
                    .map(|f| {
                        let sum: f32 = f.chunks_exact(width).map(sample).sum();
                        sum / f32::from(channels)
                    })
                    .collect();
                return Ok((samples, rate));
            }
            _ => {}
        }
        at = body + len + (len & 1);
    }
    Err("the WAV file has no audio".into())
}

fn decode_ogg(bytes: &[u8]) -> Result<(Vec<f32>, u32), String> {
    let mut reader = lewton::inside_ogg::OggStreamReader::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("the OGG file can't be read: {e}"))?;
    let channels = usize::from(reader.ident_hdr.audio_channels).max(1);
    let rate = reader.ident_hdr.audio_sample_rate;
    let mut out = Vec::new();
    while let Some(packet) = reader
        .read_dec_packet_itl()
        .map_err(|e| format!("the OGG file can't be read: {e}"))?
    {
        #[allow(clippy::cast_precision_loss, reason = "channel count")]
        out.extend(
            packet
                .chunks_exact(channels)
                .map(|f| f.iter().map(|&s| f32::from(s) / 32_768.0).sum::<f32>() / channels as f32),
        );
        if out.len() > rate as usize * 10 {
            break;
        }
    }
    Ok((out, rate))
}

/// An imported sound made ready to play as a cue: at KIVO's cue rate, at most two seconds, fading
/// in and out so it never clicks, at `gain`.
pub fn prepare(samples: &[f32], rate: u32, gain: f32) -> Vec<f32> {
    let mut out = resample(samples, rate, CUE_RATE);
    out.truncate(samples_for(MAX_CUSTOM_SECONDS));
    let fade = samples_for(0.01).min(out.len() / 2);
    let n = out.len();
    for i in 0..fade {
        #[allow(clippy::cast_precision_loss, reason = "fade position")]
        let level = i as f32 / fade as f32;
        out[i] *= level;
        out[n - 1 - i] *= level;
    }
    let peak = out.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
    let scale = if peak > 0.95 { 0.95 / peak } else { 1.0 };
    out.iter_mut().for_each(|v| *v *= scale * gain);
    out
}

fn samples_for(seconds: f32) -> usize {
    samples(seconds)
}

/// Linear resampling (cues are short; quality beyond this isn't audible in a chime).
fn resample(audio: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || audio.is_empty() {
        return audio.to_vec();
    }
    let ratio = f64::from(from) / f64::from(to);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "positions"
    )]
    let n = (audio.len() as f64 / ratio) as usize;
    (0..n)
        .map(|i| {
            #[allow(clippy::cast_precision_loss, reason = "positions")]
            let x = i as f64 * ratio;
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "floor"
            )]
            let j = x as usize;
            #[allow(clippy::cast_possible_truncation, reason = "fraction")]
            let t = (x - j as f64) as f32;
            let a = audio[j.min(audio.len() - 1)];
            let b = audio[(j + 1).min(audio.len() - 1)];
            a + (b - a) * t
        })
        .collect()
}

/// Where an imported cue lives: `<dir>/<cue>.wav` or `.ogg`.
pub fn custom_file(dir: &std::path::Path, cue: Cue) -> Option<std::path::PathBuf> {
    let name = serde_json::to_value(cue).ok()?.as_str()?.to_owned();
    ["wav", "ogg"]
        .iter()
        .map(|ext| dir.join(format!("{name}.{ext}")))
        .find(|p| p.is_file())
}

/// The sets people can pick; Custom plays the user's own sounds where they imported one and Soft
/// for the rest.
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

    /// A WAV file of `samples` frames, `channels` channels, in `format` (1 PCM, 3 float) at `bits`.
    fn wav(format: u16, bits: u16, channels: u16, rate: u32, data: &[u8]) -> Vec<u8> {
        let mut out = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        out.extend(16u32.to_le_bytes());
        out.extend(format.to_le_bytes());
        out.extend(channels.to_le_bytes());
        out.extend(rate.to_le_bytes());
        out.extend((rate * u32::from(channels) * u32::from(bits / 8)).to_le_bytes());
        out.extend((channels * bits / 8).to_le_bytes());
        out.extend(bits.to_le_bytes());
        out.extend(b"LIST");
        out.extend(4u32.to_le_bytes());
        out.extend(b"INFO");
        out.extend(b"data");
        out.extend(u32::try_from(data.len()).unwrap().to_le_bytes());
        out.extend(data);
        out
    }

    #[test]
    fn imported_wav_and_ogg_files_decode() {
        // 16-bit stereo: the channels are averaged.
        let pcm: Vec<u8> = [16_384i16, -16_384, 8_192, 8_192]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        let (mono, rate) = decode(&wav(1, 16, 2, 44_100, &pcm)).unwrap();
        assert_eq!(rate, 44_100);
        assert_eq!(mono, [0.0, 0.25]);
        // 32-bit float and 24-bit, mono.
        let floats: Vec<u8> = [0.5f32, -0.5]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        assert_eq!(
            decode(&wav(3, 32, 1, 16_000, &floats)).unwrap().0,
            [0.5, -0.5]
        );
        let (s24, _) = decode(&wav(1, 24, 1, 16_000, &[0x00, 0x00, 0x40])).unwrap();
        assert!((s24[0] - 0.5).abs() < 1e-6);
        // OGG Vorbis (a 0.3 s tone made for this test).
        let ogg = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/chime.ogg"
        ))
        .unwrap();
        let (tone, rate) = decode(&ogg).unwrap();
        assert_eq!(rate, 44_100);
        #[allow(clippy::cast_precision_loss, reason = "a short sound")]
        let seconds = tone.len() as f32 / rate as f32;
        assert!((seconds - 0.3).abs() < 0.05, "{seconds}");
        let peak = tone.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.05, "audible: {peak}");
        // Anything else is refused.
        assert!(decode(b"not audio at all").is_err());
        assert!(
            decode(&wav(2, 4, 1, 8_000, &[1, 2])).is_err(),
            "ADPCM isn't supported"
        );
    }

    #[test]
    fn an_imported_cue_is_resampled_capped_and_click_free() {
        let long: Vec<f32> = vec![0.8; 48_000 * 5];
        let cue = prepare(&long, 48_000, 1.0);
        assert_eq!(cue.len(), (CUE_RATE as f32 * MAX_CUSTOM_SECONDS) as usize);
        assert!(
            cue[0].abs() < 0.01 && cue[cue.len() - 1].abs() < 0.01,
            "fades"
        );
        let quiet = prepare(&long, 48_000, 0.5);
        assert!((quiet[CUE_RATE as usize] - 0.4).abs() < 0.01);
    }

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
