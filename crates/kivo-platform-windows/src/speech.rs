//! Windows' built-in voices through WinRT `Windows.Media.SpeechSynthesis` (VOICE §3, "TTS:
//! System"). The synthesizer returns a WAV stream in memory; KIVO reads it and plays it through its
//! own mixer, so earcons, ducking and cancel apply to it like any other voice.

use crate::com::{Com, os_error};
use kivo_platform::{PlatformError, PlatformResult, SpeechSynth, SynthAudio, SystemVoice};
use windows::Media::SpeechSynthesis::{SpeechSynthesizer, VoiceGender};
use windows::Storage::Streams::DataReader;
use windows::core::HSTRING;

#[derive(Default)]
pub struct WindowsSpeech;

impl SpeechSynth for WindowsSpeech {
    fn voices(&self) -> PlatformResult<Vec<SystemVoice>> {
        let _com = Com::init()?;
        let all = SpeechSynthesizer::AllVoices().map_err(|e| os_error(&e))?;
        let mut voices = Vec::new();
        for info in all {
            voices.push(SystemVoice {
                id: info.Id().map_err(|e| os_error(&e))?.to_string(),
                name: info.DisplayName().map_err(|e| os_error(&e))?.to_string(),
                language: info.Language().map_err(|e| os_error(&e))?.to_string(),
                gender: match info.Gender() {
                    Ok(VoiceGender::Female) => "female".into(),
                    Ok(VoiceGender::Male) => "male".into(),
                    _ => String::new(),
                },
            });
        }
        Ok(voices)
    }

    fn synthesize(&self, text: &str, voice: Option<&str>) -> PlatformResult<SynthAudio> {
        self.synthesize_at(text, voice, 1.0)
    }

    fn synthesize_at(
        &self,
        text: &str,
        voice: Option<&str>,
        rate: f64,
    ) -> PlatformResult<SynthAudio> {
        let _com = Com::init()?;
        let synth = SpeechSynthesizer::new().map_err(|e| os_error(&e))?;
        if (rate - 1.0).abs() > f64::EPSILON {
            // SpeakingRate is 0.5–6.0 (Windows 10 1803+); older builds ignore it.
            if let Ok(options) = synth.Options() {
                let _ = options.SetSpeakingRate(rate.clamp(0.5, 2.0));
            }
        }
        if let Some(wanted) = voice {
            let all = SpeechSynthesizer::AllVoices().map_err(|e| os_error(&e))?;
            if let Some(info) = all
                .into_iter()
                .find(|v| v.Id().is_ok_and(|id| id == wanted))
            {
                synth.SetVoice(&info).map_err(|e| os_error(&e))?;
            }
        }
        let stream = synth
            .SynthesizeTextToStreamAsync(&HSTRING::from(text))
            .and_then(|op| op.join())
            .map_err(|e| os_error(&e))?;
        let size = u32::try_from(stream.Size().map_err(|e| os_error(&e))?)
            .map_err(|_| PlatformError::Unsupported)?;
        let input = stream.GetInputStreamAt(0).map_err(|e| os_error(&e))?;
        let reader = DataReader::CreateDataReader(&input).map_err(|e| os_error(&e))?;
        reader
            .LoadAsync(size)
            .and_then(|op| op.join())
            .map_err(|e| os_error(&e))?;
        let mut wav = vec![0u8; size as usize];
        reader.ReadBytes(&mut wav).map_err(|e| os_error(&e))?;
        parse_wav(&wav).ok_or(PlatformError::Os {
            code: 0,
            message: "the system voice returned audio KIVO can't read".into(),
        })
    }
}

/// Reads 16-bit PCM from a RIFF/WAVE file, averaging channels to mono.
fn parse_wav(wav: &[u8]) -> Option<SynthAudio> {
    if wav.len() < 12 || &wav[..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return None;
    }
    let (mut rate, mut channels, mut bits, mut data) = (0u32, 0u16, 0u16, None);
    let mut at = 12;
    while at + 8 <= wav.len() {
        let id = &wav[at..at + 4];
        let len = u32::from_le_bytes(wav[at + 4..at + 8].try_into().ok()?) as usize;
        let body = wav.get(at + 8..(at + 8 + len).min(wav.len()))?;
        match id {
            b"fmt " if body.len() >= 16 => {
                let format = u16::from_le_bytes([body[0], body[1]]);
                if format != 1 {
                    return None; // only integer PCM
                }
                channels = u16::from_le_bytes([body[2], body[3]]);
                rate = u32::from_le_bytes(body[4..8].try_into().ok()?);
                bits = u16::from_le_bytes([body[14], body[15]]);
            }
            b"data" => data = Some(body),
            _ => {}
        }
        at += 8 + len + (len & 1);
    }
    let data = data?;
    if bits != 16 || channels == 0 || rate == 0 {
        return None;
    }
    let frame = usize::from(channels) * 2;
    let samples = data
        .chunks_exact(frame)
        .map(|f| {
            let sum: f32 = f
                .chunks_exact(2)
                .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0)
                .sum();
            sum / f32::from(channels)
        })
        .collect();
    Some(SynthAudio { rate, samples })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// UX-61: the speaking speed reaches the Windows voice (into memory; nothing is played).
    #[test]
    fn a_faster_speaking_rate_gives_shorter_audio() {
        let text = "This sentence is spoken at two different speeds for the test.";
        let normal = WindowsSpeech
            .synthesize(text, None)
            .expect("a Windows voice");
        let fast = WindowsSpeech
            .synthesize_at(text, None, 1.6)
            .expect("a Windows voice");
        #[allow(clippy::cast_precision_loss, reason = "sample counts")]
        let ratio = fast.samples.len() as f64 / normal.samples.len() as f64;
        assert!(ratio < 0.85, "fast/normal = {ratio}");
    }

    fn wav(rate: u32, channels: u16, pcm: &[i16]) -> Vec<u8> {
        let data: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + u32::try_from(data.len()).unwrap()).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
        out.extend_from_slice(&(channels * 2).to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
        out.extend_from_slice(&data);
        out
    }

    #[test]
    fn reads_mono_and_averages_stereo() {
        let mono = parse_wav(&wav(22_050, 1, &[16384, -16384])).unwrap();
        assert_eq!(mono.rate, 22_050);
        assert_eq!(mono.samples, [0.5, -0.5]);
        let stereo = parse_wav(&wav(16_000, 2, &[16384, 0])).unwrap();
        assert_eq!(stereo.samples, [0.25]);
        assert!(parse_wav(b"not a wav file").is_none());
    }

    #[test]
    fn the_system_voice_speaks_into_memory() {
        let speech = WindowsSpeech;
        let voices = speech.voices().unwrap();
        assert!(!voices.is_empty(), "Windows ships at least one voice");
        let audio = speech.synthesize("Done.", Some(&voices[0].id)).unwrap();
        assert!(audio.rate >= 16_000);
        #[allow(clippy::cast_precision_loss)]
        let seconds = audio.samples.len() as f32 / audio.rate as f32;
        assert!((0.2..3.0).contains(&seconds), "{seconds}s");
        assert!(audio.samples.iter().any(|s| s.abs() > 0.05), "not silent");
    }
}
