//! Audio capture/playback and echo cancellation (VOICE §1).

use crate::error::PlatformResult;
use crate::types::DeviceId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
    pub id: DeviceId,
    pub name: String,
    pub is_default: bool,
}

/// The native format a stream delivers or expects. KIVO resamples to 16 kHz mono itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

/// Receives interleaved `f32` frames on the audio thread. It must not block or allocate heavily
/// (VOICE §1: capture writes a lock-free ring buffer).
pub type FrameSink = Box<dyn FnMut(&[f32], StreamFormat) + Send>;

/// Fills each output buffer (interleaved `f32`) on the audio thread; silence is all zeros.
pub type FrameSource = Box<dyn FnMut(&mut [f32], StreamFormat) + Send>;

/// An open stream. Dropping it stops the stream and releases the device.
pub trait AudioStream: Send {
    fn format(&self) -> StreamFormat;
}

pub trait AudioIo: Send + Sync {
    fn input_devices(&self) -> PlatformResult<Vec<AudioDevice>>;
    fn output_devices(&self) -> PlatformResult<Vec<AudioDevice>>;
    /// Starts capturing from `device` (the default input when `None`) into `sink`.
    fn open_capture(
        &self,
        device: Option<&DeviceId>,
        sink: FrameSink,
    ) -> PlatformResult<Box<dyn AudioStream>>;
    /// Starts playback on `device`; `source` fills each buffer the device asks for.
    fn open_playback(
        &self,
        device: Option<&DeviceId>,
        source: FrameSource,
    ) -> PlatformResult<Box<dyn AudioStream>>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AecKind {
    /// The OS's own AEC (Windows 11 22621+ where the device exposes it).
    Os,
    /// WebRTC AEC3 in-process.
    WebRtc,
}

/// Removes KIVO's own output from the microphone signal, so KIVO can hear the user over itself.
pub trait EchoCancel: Send {
    fn kind(&self) -> AecKind;
    /// Cleans `capture` in place, given the `reference` that was playing at the same time.
    /// Both are 16 kHz mono frames of equal length.
    fn process(&mut self, capture: &mut [f32], reference: &[f32]);
}
