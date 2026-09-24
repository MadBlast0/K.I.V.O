//! Where a model runs (VOICE §8, PLAN-09): on the processor, or on the graphics card through
//! DirectML when the runtime's GPU policy allows it. A GPU the driver refuses is not an error: the
//! model runs on the processor instead, and says so.

use crate::error::VoiceResult;
use ort::ep::ExecutionProvider as _;
use ort::session::Session;
use std::path::Path;

/// Where a model should run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Device {
    #[default]
    Cpu,
    /// DirectML on the given adapter (0 is the default one).
    Gpu(u32),
}

/// A session for `model`, on `device` if it can be, else on `threads` CPU threads. Returns
/// whether it ended up on the GPU.
pub fn session(model: &Path, threads: usize, device: Device) -> VoiceResult<(Session, bool)> {
    if let Device::Gpu(adapter) = device {
        let dml = ort::ep::DirectML::default().with_device_id(i32::try_from(adapter).unwrap_or(0));
        if dml.is_available().unwrap_or(false) {
            // DirectML needs sequential execution and no memory patterns.
            let built = (|| -> Result<Session, String> {
                let mut builder = Session::builder()
                    .map_err(|e| e.to_string())?
                    .with_parallel_execution(false)
                    .map_err(|e| e.to_string())?
                    .with_memory_pattern(false)
                    .map_err(|e| e.to_string())?
                    .with_execution_providers([dml.build().error_on_failure()])
                    .map_err(|e| e.to_string())?;
                builder.commit_from_file(model).map_err(|e| e.to_string())
            })();
            match built {
                Ok(s) => return Ok((s, true)),
                Err(e) => tracing_free_warn(&format!("the GPU couldn't take {model:?}: {e}")),
            }
        }
    }
    Ok((
        Session::builder()?
            .with_intra_threads(threads.max(1))?
            .with_inter_threads(1)?
            .commit_from_file(model)?,
        false,
    ))
}

/// kivo-voice has no logger; the worker reports device fallbacks through the load result, so this
/// only notes it on stderr for the worker's log.
fn tracing_free_warn(message: &str) {
    eprintln!("{message}");
}
