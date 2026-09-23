//! The suites (BENCHMARKS §1). Each lives in its own module.

use crate::harness::{Plan, Suite};

#[cfg(windows)]
mod audio;
#[cfg(windows)]
mod e2e;
#[cfg(windows)]
mod idle;
mod ipc;
#[cfg(windows)]
mod overlay;

/// Every suite, with a one-line description for `kivo-bench list`.
pub const ALL: &[(&str, &str)] = &[
    (
        "ipc",
        "runtime ↔ UI channel: connect, ping and state round trips",
    ),
    (
        "audio",
        "microphone held open and playback: CPU, packet timing, dropped frames, start latency",
    ),
    (
        "idle",
        "the running KIVO at rest: CPU, memory, wakeups, package power (30 min; KIVO must be running)",
    ),
    (
        "stt",
        "speech to text: Moonshine v2, Parakeet TDT v3, Whisper turbo (load, latency, RTF, WER, memory)",
    ),
    (
        "tts",
        "text to speech: Kokoro-82M (first audio, RTF, cancel, memory)",
    ),
    (
        "vad",
        "Silero VAD and AEC3 on a simulated room: latency, false triggers, echo leakage, barge-in",
    ),
    (
        "wake",
        "\"Hey Kivo\" keyword spotting: false rejects, false accepts per hour, CPU",
    ),
    (
        "e2e",
        "the plan §102 journeys spoken end to end: T spans from the end of speech (needs kivo-e2e)",
    ),
    (
        "overlay",
        "the running Island: hotkey → visible, white flash, window styles, GPU and power (KIVO must be running)",
    ),
];

pub fn create(name: &str, plan: Plan) -> Result<Box<dyn Suite>, String> {
    match name {
        "ipc" => Ok(Box::new(ipc::Ipc::start()?)),
        #[cfg(windows)]
        "audio" => Ok(Box::new(audio::Audio::start()?)),
        #[cfg(windows)]
        "idle" => Ok(Box::new(idle::Idle::start(plan.runs, plan.warmup)?)),
        #[cfg(windows)]
        "stt" => Ok(Box::new(crate::speech::stt::Stt::start()?)),
        #[cfg(windows)]
        "tts" => Ok(Box::new(crate::speech::tts::Tts::start()?)),
        #[cfg(windows)]
        "vad" => Ok(Box::new(crate::speech::vad_aec::VadAec::start()?)),
        #[cfg(windows)]
        "wake" => Ok(Box::new(crate::speech::wake::Wake::start()?)),
        #[cfg(windows)]
        "overlay" => Ok(Box::new(overlay::Overlay::start()?)),
        #[cfg(windows)]
        "e2e" => Ok(Box::new(e2e::E2e::start()?)),
        other => Err(format!("unknown suite \"{other}\" (see `kivo-bench list`)")),
    }
}
