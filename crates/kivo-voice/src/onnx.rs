//! ONNX Runtime sessions for KIVO's ONNX engines. They all run on the processor: local models on
//! the graphics card are GGML engines instead (DECISIONS "GGML on Vulkan, no DirectML").

use crate::error::VoiceResult;
use ort::session::Session;
use std::path::Path;

/// A session for `model` on `threads` processor threads (at least one), running its graph nodes
/// one after another.
pub fn session(model: &Path, threads: usize) -> VoiceResult<Session> {
    Ok(Session::builder()?
        .with_intra_threads(threads.max(1))?
        .with_inter_threads(1)?
        .commit_from_file(model)?)
}
