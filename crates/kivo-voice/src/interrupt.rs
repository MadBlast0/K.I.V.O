//! Stopping a model mid-run: a long sentence can keep ONNX Runtime busy for seconds, and a cancel
//! checked only between runs would wait for it (VOICE §10: cancel → silence ≤ 100 ms, and the next
//! reply can't start until the worker is free). `interruptible` gives the run `RunOptions` that a
//! small watcher terminates as soon as the token is cancelled.

use crate::error::{VoiceError, VoiceResult};
use ort::session::RunOptions;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// How often the watcher looks at the token while a run is in progress.
const POLL: Duration = Duration::from_millis(5);

/// Runs `run` with options that are terminated when `cancel` fires; a terminated (or cancelled)
/// run is `VoiceError::Cancelled`.
pub fn interruptible<T>(
    cancel: &CancellationToken,
    run: impl FnOnce(&RunOptions) -> VoiceResult<T>,
) -> VoiceResult<T> {
    if cancel.is_cancelled() {
        return Err(VoiceError::Cancelled);
    }
    let options = Arc::new(RunOptions::new()?);
    let finished = Arc::new(AtomicBool::new(false));
    let watcher = {
        let options = Arc::clone(&options);
        let finished = Arc::clone(&finished);
        let cancel = cancel.clone();
        std::thread::spawn(move || {
            while !finished.load(Ordering::Acquire) {
                if cancel.is_cancelled() {
                    let _ = options.terminate();
                    return;
                }
                std::thread::sleep(POLL);
            }
        })
    };
    let result = run(&options);
    finished.store(true, Ordering::Release);
    let _ = watcher.join();
    if cancel.is_cancelled() {
        return Err(VoiceError::Cancelled);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cancelled_token_stops_before_running() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let ran = interruptible(&cancel, |_| Ok(1));
        assert!(matches!(ran, Err(VoiceError::Cancelled)));
    }

    #[test]
    fn a_run_that_is_cancelled_while_it_works_ends_cancelled() {
        let cancel = CancellationToken::new();
        let stop = cancel.clone();
        let ran = interruptible(&cancel, |_| {
            stop.cancel();
            std::thread::sleep(Duration::from_millis(20));
            Ok(1)
        });
        assert!(matches!(ran, Err(VoiceError::Cancelled)));
        assert_eq!(
            interruptible(&CancellationToken::new(), |_| Ok(2)).unwrap(),
            2
        );
    }
}
