//! Audio processing between the platform's audio I/O and the speech engines (VOICE §1): converting
//! capture to 16 kHz mono through a lock-free ring buffer, framing, the energy gate in front of the
//! VAD, and the playback mixer for speech and earcons.

pub mod capture;
pub mod echo;
pub mod frames;
pub mod gate;
pub mod mixer;
pub mod resample;

pub use capture::{CaptureReader, CaptureWriter, capture_ring};
pub use frames::{BATCH_80MS, Chunker, FRAME_10MS, History};
pub use gate::{EnergyGate, level_db};
pub use mixer::{DeviceFormat, Mixer};
pub use resample::{RateConverter, downmix, upmix};
