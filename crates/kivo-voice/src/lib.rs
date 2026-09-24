//! Speech engines behind provider traits (VOICE §2–3, §9): voice activity detection, speech to
//! text and text to speech, plus the wake-word, speaker and turn-detection traits that M2 fills
//! in. Local engines run on ONNX Runtime (`ort`); nothing here links GPL code (DIST-17).

pub mod accel;
pub mod bpe;
pub mod chatterbox;
pub mod cloud;
pub mod embed;
pub mod engine;
pub mod error;
pub mod fbank;
pub mod interrupt;
pub mod kokoro;
pub mod kws;
pub mod language;
pub mod moonshine;
pub mod parakeet;
pub mod recommend;
pub mod registry;
pub mod silero;
pub mod smart_turn;
pub mod speaker;
pub mod supertonic;
pub mod system_tts;
pub mod traits;
pub mod utterance;
pub mod wakeword;
pub mod wer;
pub mod whisper;
pub mod whisper_cpp;

pub use engine::{Accel, EngineInfo, EngineKind, EngineSlot, ResourceEstimate};

/// Every speech engine KIVO ships, for choosing one and for the privacy check (VOICE-07).
pub fn engines() -> Vec<EngineInfo> {
    let mut all = moonshine::infos();
    all.push(parakeet::info());
    all.push(whisper::info());
    all.extend([
        system_tts::info(),
        kokoro::info(),
        supertonic::info(),
        chatterbox::info(),
    ]);
    // Cloud engines are listed too, so the egress check always knows them as cloud.
    all.extend(cloud::PROVIDERS.iter().map(cloud::info));
    all
}

/// The engine with this id, if KIVO has it.
pub fn engine(id: &str) -> Option<EngineInfo> {
    engines().into_iter().find(|e| e.id == id)
}
pub use error::{VoiceError, VoiceResult};
pub use traits::{
    AudioSink, Embedding, SAMPLE_RATE, SpeakerVerifier, SttEngine, SttEvent, SttOptions, SttStream,
    TtsEngine, TurnDetector, VadEngine, VerifyResult, VoiceInfo, WakeDetector, WakeHit,
    WakeVerifier,
};
