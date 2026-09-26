//! The operating system's own voices (VOICE §3, "TTS: System"): always available, no download, the
//! fallback when no other voice is installed.

use crate::error::PlatformResult;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemVoice {
    pub id: String,
    pub name: String,
    /// BCP-47 tag, e.g. "en-US".
    pub language: String,
    /// `female`, `male`, or empty when the system doesn't say.
    #[serde(default)]
    pub gender: String,
}

/// Mono audio as the voice produced it.
#[derive(Clone, Debug, PartialEq)]
pub struct SynthAudio {
    pub rate: u32,
    pub samples: Vec<f32>,
}

pub trait SpeechSynth: Send + Sync {
    fn voices(&self) -> PlatformResult<Vec<SystemVoice>>;
    /// Speaks `text` with `voice` (an id from `voices`; the system default when `None` or
    /// unknown) into memory.
    fn synthesize(&self, text: &str, voice: Option<&str>) -> PlatformResult<SynthAudio>;
    /// As `synthesize`, at `rate` times the normal speed (0.5–2.0). Voices without a rate
    /// control speak at their normal speed.
    fn synthesize_at(
        &self,
        text: &str,
        voice: Option<&str>,
        rate: f64,
    ) -> PlatformResult<SynthAudio> {
        let _ = rate;
        self.synthesize(text, voice)
    }
}
