//! The suites (BENCHMARKS §1). Each lives in its own module.

use crate::harness::{Plan, Suite};

#[cfg(windows)]
mod audio;
mod brain;
#[cfg(windows)]
mod e2e;
#[cfg(windows)]
mod idle;
mod ipc;
#[cfg(windows)]
mod open;
#[cfg(windows)]
mod overlay;
mod router;

/// Every suite, with a one-line description for `kivo-bench list`.
pub const ALL: &[(&str, &str)] = &[
    (
        "ipc",
        "runtime ↔ UI channel: connect, ping and state round trips",
    ),
    (
        "router",
        "intent router: a command to the fast path, a request on to a brain (bundled grammar)",
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
        "speech to text: KIVO's Moonshine, Parakeet and Whisper (load, latency, RTF, WER, memory)",
    ),
    (
        "tts",
        "text to speech: KIVO's Kokoro, Supertonic and Windows voices (first audio, RTF, cancel, memory)",
    ),
    (
        "vad",
        "Silero VAD and AEC3 on a simulated room: latency, false triggers, echo leakage, barge-in",
    ),
    (
        "wake",
        "\"Hey Kivo\" on KIVO's keyword spotter: false rejects, false accepts per hour, CPU",
    ),
    (
        "brain",
        "one brain on fixed prompts: first token, speed, tool call, JSON, cancel, errors (KIVO_BENCH_BRAIN)",
    ),
    (
        "e2e",
        "the plan §102 journeys spoken end to end: T spans from the end of speech (needs kivo-e2e)",
    ),
    (
        "open",
        "the Control Center's warm open: KIVO launched again with its window closed to the tray (KIVO must be running)",
    ),
    (
        "overlay",
        "the running Island: hotkey → visible, white flash, window styles, GPU and power (KIVO must be running)",
    ),
];

pub fn create(name: &str, plan: Plan) -> Result<Box<dyn Suite>, String> {
    match name {
        "ipc" => Ok(Box::new(ipc::Ipc::start()?)),
        "router" => Ok(Box::new(router::Router::start()?)),
        "brain" => Ok(Box::new(brain::Brain::start()?)),
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
        "wake" => Ok(Box::new(crate::speech::wake::Wake::start(plan)?)),
        #[cfg(windows)]
        "open" => Ok(Box::new(open::Open::start()?)),
        #[cfg(windows)]
        "overlay" => Ok(Box::new(overlay::Overlay::start()?)),
        #[cfg(windows)]
        "e2e" => Ok(Box::new(e2e::E2e::start()?)),
        other => Err(format!("unknown suite \"{other}\" (see `kivo-bench list`)")),
    }
}
