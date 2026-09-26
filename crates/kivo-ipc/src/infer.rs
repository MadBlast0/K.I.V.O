//! The runtime ↔ `kivo-infer` protocol (ARCHITECTURE §3, ARCH-21): the UI protocol's framing and
//! JSON-RPC, over the worker's stdin/stdout, with methods for streaming audio in and results out.
//!
//! ```text
//! runtime → worker   hello · model.load · model.unload · stt.start · stt.finish · tts.speak · shutdown
//!                    (notifications) stt.audio · stt.cancel · tts.cancel
//! worker → runtime   (notifications) model.state · stt.partial · tts.audio · tts.done
//! ```
//!
//! Audio travels as base64 of little-endian 16-bit PCM: compact in JSON, and exact enough for
//! speech. Shared memory is a later optimization.

use base64::Engine as _;
use serde::{Deserialize, Serialize};

pub const INFER_PROTOCOL: u16 = 1;

pub mod method {
    pub const HELLO: &str = "hello";
    pub const MODEL_LOAD: &str = "model.load";
    pub const MODEL_UNLOAD: &str = "model.unload";
    pub const MODEL_STATE: &str = "model.state";
    pub const STT_START: &str = "stt.start";
    pub const STT_AUDIO: &str = "stt.audio";
    pub const STT_PARTIAL: &str = "stt.partial";
    pub const STT_FINISH: &str = "stt.finish";
    pub const STT_CANCEL: &str = "stt.cancel";
    pub const TTS_VOICES: &str = "tts.voices";
    pub const TTS_SPEAK: &str = "tts.speak";
    pub const TTS_AUDIO: &str = "tts.audio";
    pub const TTS_DONE: &str = "tts.done";
    pub const TTS_CANCEL: &str = "tts.cancel";
    pub const SHUTDOWN: &str = "shutdown";
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InferHello {
    pub protocol: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InferWelcome {
    pub protocol: u16,
    pub version: String,
    pub pid: u32,
    /// The graphics-card backend this worker was built with (VOICE-50); `None` for a processor-only
    /// build. The runtime starts the worker whose backend it chose, and checks it here.
    #[serde(default)]
    pub backend: Option<GpuBackend>,
}

/// A GPU backend a speech worker can be built with (VOICE-50).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GpuBackend {
    Cuda,
    Vulkan,
    Metal,
}

/// Where a GPU-capable model runs: the backend and the card, by the name the system gives it
/// (DXGI on Windows). The worker finds that card among its backend's devices.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GpuTarget {
    pub backend: GpuBackend,
    pub device: String,
}

/// Which engine slot a model fills in the worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InferSlot {
    Stt,
    Tts,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelLoad {
    pub slot: InferSlot,
    /// Engine id: `moonshine-base-en`, `system`, …
    pub engine: String,
    /// The installed model folder, for engines that need one.
    pub dir: Option<String>,
    pub threads: usize,
    /// The language to speak or hear (BCP-47), for engines that cover several.
    #[serde(default)]
    pub language: Option<String>,
    /// For a cloud engine: the user's key (from Credential Manager) and where to reach it.
    #[serde(default)]
    pub cloud: Option<CloudLoad>,
    /// The graphics card and backend the GPU policy gives a GPU-capable recognizer (PLAN-09,
    /// VOICE-50); otherwise the processor.
    #[serde(default)]
    pub gpu: Option<GpuTarget>,
}

/// How the worker reaches a cloud speech service (VOICE-10/11). The key travels only over the
/// runtime's private pipe to its own worker.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloudLoad {
    pub key: String,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
}

impl std::fmt::Debug for CloudLoad {
    /// Never prints the key.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudLoad")
            .field("base_url", &self.base_url)
            .field("region", &self.region)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelUnload {
    pub slot: InferSlot,
}

/// Model residency (plan §26, §93; PLAN-02).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Residency {
    Unloaded,
    Warming,
    Warm,
    Active,
    Idle,
    Unloading,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelState {
    pub slot: InferSlot,
    pub engine: String,
    pub state: Residency,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SttStart {
    pub id: u64,
    pub language: String,
    #[serde(default)]
    pub vocabulary: Vec<String>,
}

/// 16 kHz mono speech for utterance `id`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SttAudio {
    pub id: u64,
    pub pcm: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SttPartial {
    pub id: u64,
    pub text: String,
    /// True when this text won't change any more (a stable prefix).
    pub stable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UtteranceId {
    pub id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SttFinal {
    pub text: String,
    /// Milliseconds from `stt.finish` to the result (T4 → T5 in the latency spans).
    pub millis: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TtsSpeak {
    pub id: u64,
    pub text: String,
    pub voice: Option<String>,
    /// Speaking speed, 1.0 normal (UX-61).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<f32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TtsAudio {
    pub id: u64,
    pub rate: u32,
    pub pcm: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TtsDone {
    pub id: u64,
    /// A user-safe reason when speaking failed.
    pub error: Option<String>,
    pub cancelled: bool,
}

/// `f32` samples → base64 of 16-bit little-endian PCM.
pub fn encode_pcm(samples: &[f32]) -> String {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        #[allow(clippy::cast_possible_truncation, reason = "clamped to the i16 range")]
        let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// The reverse of `encode_pcm`; `None` for malformed input.
pub fn decode_pcm(text: &str) -> Option<Vec<f32>> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(text)
        .ok()?;
    if bytes.len() % 2 != 0 {
        return None;
    }
    Some(
        bytes
            .chunks_exact(2)
            .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0)
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcm_round_trips_within_16_bit_precision() {
        let samples = [0.0, 0.5, -0.5, 1.0, -1.0, 0.123_45];
        let back = decode_pcm(&encode_pcm(&samples)).unwrap();
        for (a, b) in samples.iter().zip(&back) {
            assert!((a - b).abs() < 1e-4, "{a} vs {b}");
        }
        assert!(decode_pcm("not base64!").is_none());
        assert!(decode_pcm("AA==").is_none(), "odd byte count");
    }

    #[test]
    fn messages_use_camel_case_and_reject_unknown_fields() {
        let load = serde_json::json!({"slot":"stt","engine":"moonshine-base-en","dir":"C:\\m","threads":2});
        assert!(serde_json::from_value::<ModelLoad>(load).is_ok());
        let bad = serde_json::json!({"slot":"stt","engine":"x","dir":null,"threads":1,"extra":1});
        assert!(serde_json::from_value::<ModelLoad>(bad).is_err());
        assert_eq!(serde_json::to_value(Residency::Warming).unwrap(), "warming");
    }
}
