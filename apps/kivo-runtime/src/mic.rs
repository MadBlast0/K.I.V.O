//! The microphone while KIVO listens (VOICE §1, ARCHITECTURE §3). It follows the session: the mic
//! opens when listening starts and is released when it ends, so it is never open otherwise. While
//! open, its level is published about 30 times a second for the Island's waveform (the IPC
//! `levels` stream): silence shows as dots, speech moves the bars.
//!
//! Speech recognition (M1) will consume the same capture stream.

use crate::core::Core;
use kivo_core::SessionState;
use kivo_platform::{AudioIo, AudioStream};
use kivo_platform_windows::WindowsAudio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use tokio::sync::watch;

/// How often the level is published (ARCHITECTURE §3: 30–60 Hz).
const LEVEL_PERIOD: Duration = Duration::from_millis(33);
/// Levels at or below this are silence (dots); at or above the top, full bars.
const FLOOR_DB: f32 = -60.0;
const TOP_DB: f32 = -10.0;

fn listening(session: SessionState) -> bool {
    matches!(session, SessionState::Listening | SessionState::FollowUp)
}

/// RMS of interleaved samples.
fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss, reason = "a frame length")]
    let n = samples.len() as f32;
    (samples.iter().map(|s| s * s).sum::<f32>() / n).sqrt()
}

/// RMS → 0–1 on a decibel scale.
pub fn level(rms: f32) -> f32 {
    if rms <= 0.0 {
        return 0.0;
    }
    ((20.0 * rms.log10() - FLOOR_DB) / (TOP_DB - FLOOR_DB)).clamp(0.0, 1.0)
}

/// The open microphone: the stream, and the loudest RMS since the last tick.
struct Open {
    _stream: Box<dyn AudioStream>,
    peak: Arc<AtomicU32>,
}

fn open(peak: &Arc<AtomicU32>) -> Option<Open> {
    let sink_peak = Arc::clone(peak);
    let stream = WindowsAudio.open_capture(
        None,
        Box::new(move |samples, _format| {
            let value = rms(samples);
            // Keep the loudest frame of the period (peak hold), without locks on the audio thread.
            let _ = sink_peak.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |bits| {
                (value > f32::from_bits(bits)).then_some(value.to_bits())
            });
        }),
    );
    match stream {
        Ok(stream) => Some(Open {
            _stream: stream,
            peak: Arc::clone(peak),
        }),
        Err(e) => {
            // No microphone (or it's blocked): listening still works for the Island, silently.
            tracing::warn!(%e, "couldn't open the microphone");
            None
        }
    }
}

/// Opens and closes the mic with the session and publishes its level into `levels`.
pub async fn run(core: Arc<Core>, levels: watch::Sender<f32>) {
    let mut state = core.state();
    let shutdown = core.shutdown();
    let peak = Arc::new(AtomicU32::new(0));
    let mut mic: Option<Open> = None;
    let mut ticker = tokio::time::interval(LEVEL_PERIOD);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        let want = listening(state.borrow_and_update().session);
        if want && mic.is_none() {
            peak.store(0, Ordering::Relaxed);
            mic = open(&peak);
            tracing::debug!(open = mic.is_some(), "microphone opened for listening");
        } else if !want && mic.is_some() {
            mic = None; // dropping the stream releases the device
            levels.send_replace(0.0);
            tracing::debug!("microphone released");
        }
        tokio::select! {
            changed = state.changed() => if changed.is_err() { break },
            // The level ticks only while the mic is open.
            _ = ticker.tick(), if mic.is_some() => {
                if let Some(open) = &mic {
                    let bits = open.peak.swap(0, Ordering::Relaxed);
                    levels.send_replace(level(f32::from_bits(bits)));
                }
            }
            () = shutdown.cancelled() => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_follow_a_decibel_scale() {
        assert_eq!(level(0.0), 0.0, "silence");
        assert_eq!(level(0.0005), 0.0, "below −60 dBFS is silence");
        assert!((level(0.01) - 0.4).abs() < 1e-3, "−40 dBFS");
        assert_eq!(level(0.5), 1.0, "loud speech is full scale");
    }

    #[test]
    fn rms_of_a_square_wave_is_its_amplitude() {
        assert!((rms(&[0.5, -0.5, 0.5, -0.5]) - 0.5).abs() < 1e-6);
        assert_eq!(rms(&[]), 0.0);
    }

    #[test]
    fn the_mic_is_open_only_while_listening() {
        assert!(listening(SessionState::Listening));
        assert!(listening(SessionState::FollowUp));
        for s in [
            SessionState::Idle,
            SessionState::Paused,
            SessionState::Thinking,
        ] {
            assert!(!listening(s));
        }
    }
}
