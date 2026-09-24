//! The microphone for a realtime conversation (BRAINS §8, BRAIN-33). The provider hears the user
//! directly, so this streams 16 kHz mono audio to the session while it is open, with KIVO's own
//! voice removed first (echo cancellation, VOICE-30) so the model doesn't hear itself. The
//! wake-word listener stays as it is: "Kivo stop" still ends the session.

use crate::speaker::Speaker;
use kivo_audio::capture::capture_ring;
use kivo_platform::{AudioIo, DeviceId};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// Audio is sent in pieces of about this length.
const PIECE: Duration = Duration::from_millis(40);
/// Echo cancellation keeps going this long after KIVO stops talking (the room's echo).
const ECHO_TAIL: Duration = Duration::from_millis(300);

/// A running capture; stops when dropped.
pub struct LiveMic {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl LiveMic {
    /// Opens the microphone and starts streaming to `out`. Fails when the microphone can't be
    /// opened (blocked in Windows privacy settings, unplugged).
    pub fn start(
        audio: Arc<dyn AudioIo>,
        device: Option<DeviceId>,
        speaker: Arc<Speaker>,
        out: mpsc::Sender<Vec<f32>>,
    ) -> Result<Self, String> {
        let (mut writer, mut reader) = capture_ring(2);
        let stream = audio
            .open_capture(
                device.as_ref(),
                Box::new(move |samples, format| {
                    writer.write(samples, format.sample_rate, format.channels);
                }),
            )
            .map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("kivo-live-mic".into())
            .spawn(move || {
                // The stream lives on this thread and closes with it.
                let _stream = stream;
                let reference = speaker.reference();
                let mut echo = kivo_audio::echo::EchoCanceller::new();
                let mut echo_until: Option<Instant> = None;
                let mut buf = Vec::new();
                let mut piece = Vec::new();
                let size = usize::try_from(
                    u128::from(kivo_audio::capture::RATE) * PIECE.as_millis() / 1_000,
                )
                .unwrap_or(640);
                while !stopping.load(Ordering::SeqCst) {
                    reader.wait(Duration::from_millis(20));
                    buf.clear();
                    reader.read(&mut buf);
                    if speaker.speaking() {
                        echo_until = Some(Instant::now() + ECHO_TAIL);
                    }
                    if echo_until.is_some_and(|until| Instant::now() < until) {
                        let (rate, played) = reference.take();
                        echo.played(rate, &played);
                        echo.clean(&mut buf);
                    } else {
                        if echo_until.take().is_some() {
                            let mut rest = Vec::new();
                            echo.flush(&mut rest);
                            rest.append(&mut buf);
                            buf = rest;
                        }
                        let _ = reference.take();
                    }
                    piece.extend_from_slice(&buf);
                    while piece.len() >= size {
                        let rest = piece.split_off(size);
                        // A full queue means the connection is behind: drop, don't wait.
                        let _ = out.try_send(std::mem::replace(&mut piece, rest));
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for LiveMic {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
