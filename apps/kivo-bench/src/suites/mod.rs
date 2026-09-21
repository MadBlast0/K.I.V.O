//! The suites (BENCHMARKS §1). Each lives in its own module.

use crate::harness::{Plan, Suite};

#[cfg(windows)]
mod audio;
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
        "overlay" => Ok(Box::new(overlay::Overlay::start()?)),
        other => Err(format!("unknown suite \"{other}\" (see `kivo-bench list`)")),
    }
}
