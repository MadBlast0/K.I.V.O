//! Speech engines behind provider traits (VOICE §2–3, §9): voice activity detection, speech to
//! text and text to speech, plus the wake-word, speaker and turn-detection traits that M2 fills
//! in. Local engines run on ONNX Runtime (`ort`); nothing here links GPL code (DIST-17).

pub mod engine;
pub mod error;
pub mod language;
pub mod moonshine;
pub mod recommend;
pub mod silero;
pub mod system_tts;
pub mod traits;

pub use engine::{Accel, EngineInfo, EngineKind, EngineSlot, ResourceEstimate};
pub use error::{VoiceError, VoiceResult};
pub use traits::{
    AudioSink, Embedding, SAMPLE_RATE, SpeakerVerifier, SttEngine, SttEvent, SttOptions, SttStream,
    TtsEngine, TurnDetector, VadEngine, VerifyResult, VoiceInfo, WakeDetector, WakeHit,
    WakeVerifier,
};
