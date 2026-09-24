//! Speech engine suites (VOICE §3, BENCHMARKS §1): speech-to-text, text-to-speech, voice
//! activity and echo cancellation, and the wake word, with their test data and scoring.

pub mod data;
pub mod edacc;
pub mod stt;
pub mod tts;
pub mod vad_aec;
pub mod wake;
pub mod wer;
