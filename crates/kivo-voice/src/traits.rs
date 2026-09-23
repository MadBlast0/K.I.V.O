//! The provider traits (VOICE §2). Every engine KIVO runs sits behind one of these, so engines are
//! interchangeable per language, per tier and per machine. They are synchronous: the always-on
//! ones run on the runtime's detection thread and the heavy ones on `kivo-infer`'s worker threads.
//!
//! Audio is 16 kHz mono `f32` in −1…1 everywhere (VOICE §1) unless an engine says otherwise.

use crate::engine::EngineInfo;
use crate::error::VoiceResult;
use tokio_util::sync::CancellationToken;

/// The sample rate every engine receives.
pub const SAMPLE_RATE: u32 = 16_000;

/// Voice activity detection.
pub trait VadEngine: Send {
    fn info(&self) -> &EngineInfo;
    /// Samples per call to `process`.
    fn frame_len(&self) -> usize;
    /// The probability (0–1) that `frame` (exactly `frame_len` samples) contains speech.
    fn process(&mut self, frame: &[f32]) -> VoiceResult<f32>;
    /// Forgets the recurrent state (a new utterance).
    fn reset(&mut self);
}

/// A wake-word hit (stage 1).
#[derive(Clone, Debug, PartialEq)]
pub struct WakeHit {
    pub word_id: String,
    pub score: f32,
    /// Where the phrase is in the audio just given, in samples from its start.
    pub span: std::ops::Range<usize>,
}

/// Stage 1 of wake-word detection: cheap per-word detectors on every frame (M2, VOICE-14).
pub trait WakeDetector: Send {
    fn info(&self) -> &EngineInfo;
    fn process(&mut self, frames: &[f32]) -> VoiceResult<Option<WakeHit>>;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VerifyResult {
    Accept { score: f32 },
    Reject { score: f32 },
}

/// Stage 2: confirms a stage-1 hit on the surrounding audio (M2, VOICE-14).
pub trait WakeVerifier: Send + Sync {
    fn info(&self) -> &EngineInfo;
    fn verify(&self, audio: &[f32], hit: &WakeHit) -> VoiceResult<VerifyResult>;
}

/// A fixed-length voice embedding.
pub type Embedding = Vec<f32>;

/// Speaker recognition, a convenience filter only: it never authorizes risky actions (VOICE §5).
pub trait SpeakerVerifier: Send + Sync {
    fn info(&self) -> &EngineInfo;
    fn embed(&self, audio: &[f32]) -> VoiceResult<Embedding>;
    /// Similarity of `embedding` to the enrolled `profile`, 0–1.
    fn score(&self, embedding: &Embedding, profile: &[Embedding]) -> f32;
}

/// End-of-turn prediction (M2, VOICE-33).
pub trait TurnDetector: Send {
    fn info(&self) -> &EngineInfo;
    /// The probability that the speaker has finished, given the latest audio and transcript.
    fn end_probability(&mut self, audio_tail: &[f32], transcript: &str) -> VoiceResult<f32>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SttOptions {
    /// BCP-47 tag of the language to transcribe.
    pub language: String,
    /// Names, apps and projects to favour where the engine supports it (VOICE-23).
    pub vocabulary: Vec<String>,
}

/// Results while listening (plan §31). Partials may change; stable text won't; the final is the
/// whole utterance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SttEvent {
    Partial(String),
    Stable(String),
    Final(String),
}

/// One utterance being transcribed.
pub trait SttStream: Send {
    /// Adds audio and returns any results it produced.
    fn accept(&mut self, audio: &[f32]) -> VoiceResult<Vec<SttEvent>>;
    /// The utterance ended: the final transcript.
    fn finish(&mut self) -> VoiceResult<String>;
}

pub trait SttEngine: Send {
    fn info(&self) -> &EngineInfo;
    /// Starts an utterance. Work stops with `VoiceError::Cancelled` once `cancel` fires.
    fn start(&mut self, options: &SttOptions, cancel: CancellationToken)
    -> Box<dyn SttStream + '_>;
}

/// A voice a TTS engine offers.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceInfo {
    pub id: String,
    pub name: String,
    /// BCP-47 tag.
    pub language: String,
}

/// Receives synthesized audio as it is produced: samples and their rate.
pub type AudioSink<'a> = &'a mut dyn FnMut(&[f32], u32) -> VoiceResult<()>;

pub trait TtsEngine: Send {
    fn info(&self) -> &EngineInfo;
    /// Speaking speed for what follows, 1.0 normal, 0.5–2.0 (UX-61). Engines without a speed
    /// control ignore it.
    fn set_speed(&mut self, _speed: f32) {}
    fn voices(&self) -> Vec<VoiceInfo>;
    /// Speaks `text` in `voice` (an id from `voices`, or the default), streaming audio into
    /// `sink` sentence by sentence. Stops with `VoiceError::Cancelled` once `cancel` fires.
    fn speak(
        &mut self,
        text: &str,
        voice: Option<&str>,
        cancel: &CancellationToken,
        sink: AudioSink<'_>,
    ) -> VoiceResult<()>;
}
