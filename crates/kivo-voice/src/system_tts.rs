//! The operating system's voices as a `TtsEngine` (VOICE §3, "TTS: System"). The text is split
//! into sentences and each is synthesized and handed on as soon as it is ready, so a long answer
//! starts playing after its first sentence.

use crate::engine::{Accel, EngineInfo, EngineKind, EngineSlot, ResourceEstimate};
use crate::error::{VoiceError, VoiceResult};
use crate::traits::{AudioSink, TtsEngine, VoiceInfo};
use kivo_platform::SpeechSynth;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub const ENGINE_ID: &str = "system";

pub struct SystemTts {
    info: EngineInfo,
    synth: Arc<dyn SpeechSynth>,
}

impl SystemTts {
    pub fn new(synth: Arc<dyn SpeechSynth>) -> Self {
        Self {
            info: EngineInfo {
                id: ENGINE_ID.into(),
                name: "Windows voices".into(),
                slot: EngineSlot::Tts,
                kind: EngineKind::System,
                license: "System".into(),
                languages: vec!["*".into()],
                streaming: true,
                accel: vec![Accel::Cpu],
                resources: ResourceEstimate {
                    ram_mb: 30,
                    vram_mb: 0,
                    disk_mb: 0,
                },
                model: None,
            },
            synth,
        }
    }
}

impl TtsEngine for SystemTts {
    fn info(&self) -> &EngineInfo {
        &self.info
    }

    fn voices(&self) -> Vec<VoiceInfo> {
        self.synth
            .voices()
            .unwrap_or_default()
            .into_iter()
            .map(|v| VoiceInfo {
                id: v.id,
                name: v.name,
                language: v.language,
            })
            .collect()
    }

    fn speak(
        &mut self,
        text: &str,
        voice: Option<&str>,
        cancel: &CancellationToken,
        sink: AudioSink<'_>,
    ) -> VoiceResult<()> {
        for sentence in sentences(text) {
            if cancel.is_cancelled() {
                return Err(VoiceError::Cancelled);
            }
            let audio = self
                .synth
                .synthesize(sentence, voice)
                .map_err(|e| VoiceError::Engine(format!("{e:?}")))?;
            if cancel.is_cancelled() {
                return Err(VoiceError::Cancelled);
            }
            sink(&audio.samples, audio.rate)?;
        }
        Ok(())
    }
}

/// Splits text after `.`, `!`, `?` or a line break followed by whitespace, keeping the punctuation.
pub fn sentences(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    for (i, c) in text.char_indices() {
        let ends = matches!(c, '.' | '!' | '?' | '\n')
            && bytes.get(i + 1).is_none_or(u8::is_ascii_whitespace);
        if ends {
            let piece = text[start..=i].trim();
            if !piece.is_empty() {
                out.push(piece);
            }
            start = i + 1;
        }
    }
    let rest = text[start..].trim();
    if !rest.is_empty() {
        out.push(rest);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use kivo_platform::{PlatformResult, SynthAudio, SystemVoice};
    use std::sync::Mutex;

    #[test]
    fn text_splits_into_sentences_but_not_inside_numbers() {
        assert_eq!(
            sentences("Done. Volume is 42.5 percent! Anything else?"),
            ["Done.", "Volume is 42.5 percent!", "Anything else?"]
        );
        assert_eq!(sentences("no punctuation"), ["no punctuation"]);
        assert!(sentences("  ").is_empty());
    }

    struct Recorder(Mutex<Vec<String>>);

    impl SpeechSynth for Recorder {
        fn voices(&self) -> PlatformResult<Vec<SystemVoice>> {
            Ok(Vec::new())
        }
        fn synthesize(&self, text: &str, _voice: Option<&str>) -> PlatformResult<SynthAudio> {
            self.0.lock().unwrap().push(text.to_owned());
            Ok(SynthAudio {
                rate: 16_000,
                samples: vec![0.1; 160],
            })
        }
    }

    #[test]
    fn each_sentence_is_delivered_as_soon_as_it_is_ready_and_cancel_stops_the_rest() {
        let recorder = Arc::new(Recorder(Mutex::new(Vec::new())));
        let mut tts = SystemTts::new(recorder.clone());
        let cancel = CancellationToken::new();
        let mut chunks = 0;
        tts.speak("One. Two. Three.", None, &cancel, &mut |_, _| {
            chunks += 1;
            if chunks == 2 {
                cancel.cancel();
            }
            Ok(())
        })
        .unwrap_err();
        assert_eq!(chunks, 2);
        assert_eq!(*recorder.0.lock().unwrap(), ["One.", "Two."]);
    }
}
