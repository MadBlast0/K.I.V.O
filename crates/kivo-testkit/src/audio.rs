//! A fake audio device: capture replays a scripted clip on a thread; playback pulls a fixed number
//! of buffers from its source and records what it would have played.

use kivo_platform::{
    AudioDevice, AudioIo, AudioStream, DeviceId, FrameSink, FrameSource, PlatformResult,
    StreamFormat,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;

/// 16 kHz mono, KIVO's internal format, so tests don't depend on resampling.
pub const FAKE_FORMAT: StreamFormat = StreamFormat {
    sample_rate: 16_000,
    channels: 1,
};
/// 10 ms frames, as the real pipeline uses internally (VOICE §1).
pub const CHUNK: usize = 160;

pub struct FakeAudio {
    /// Samples the capture stream delivers, once, in 10 ms chunks.
    pub clip: Vec<f32>,
    /// How many 10 ms buffers a playback stream pulls from its source.
    pub playback_chunks: usize,
    /// Everything playback streams produced.
    pub played: Arc<Mutex<Vec<f32>>>,
}

impl FakeAudio {
    pub fn with_clip(clip: Vec<f32>) -> Self {
        Self {
            clip,
            playback_chunks: 1,
            played: Arc::default(),
        }
    }

    fn device() -> AudioDevice {
        AudioDevice {
            id: DeviceId("fake".into()),
            name: "Fake device".into(),
            is_default: true,
        }
    }
}

/// Runs its worker on a thread; dropping the stream stops and joins it.
struct FakeStream {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl FakeStream {
    fn spawn(work: impl FnOnce(&AtomicBool) + Send + 'static) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        Self {
            stop,
            worker: Some(std::thread::spawn(move || work(&flag))),
        }
    }
}

impl AudioStream for FakeStream {
    fn format(&self) -> StreamFormat {
        FAKE_FORMAT
    }
}

impl Drop for FakeStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl AudioIo for FakeAudio {
    fn input_devices(&self) -> PlatformResult<Vec<AudioDevice>> {
        Ok(vec![Self::device()])
    }

    fn output_devices(&self) -> PlatformResult<Vec<AudioDevice>> {
        Ok(vec![Self::device()])
    }

    fn open_capture(
        &self,
        _device: Option<&DeviceId>,
        mut sink: FrameSink,
    ) -> PlatformResult<Box<dyn AudioStream>> {
        let clip = self.clip.clone();
        Ok(Box::new(FakeStream::spawn(move |stop| {
            for chunk in clip.chunks(CHUNK) {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                sink(chunk, FAKE_FORMAT);
            }
        })))
    }

    /// Pulls `playback_chunks` buffers synchronously before returning, so tests are deterministic.
    fn open_playback(
        &self,
        _device: Option<&DeviceId>,
        mut source: FrameSource,
    ) -> PlatformResult<Box<dyn AudioStream>> {
        let mut buffer = [0.0f32; CHUNK];
        let mut played = self.played.lock().unwrap_or_else(PoisonError::into_inner);
        for _ in 0..self.playback_chunks {
            buffer.fill(0.0);
            source(&mut buffer, FAKE_FORMAT);
            played.extend_from_slice(&buffer);
        }
        Ok(Box::new(FakeStream::spawn(|_| {})))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn capture_delivers_the_whole_clip_in_10ms_chunks() {
        let audio = FakeAudio::with_clip((0..1000).map(|i| i as f32).collect());
        let (tx, rx) = mpsc::channel();
        let stream = audio
            .open_capture(
                None,
                Box::new(move |frames, _| tx.send(frames.to_vec()).unwrap()),
            )
            .unwrap();
        let chunks: Vec<Vec<f32>> = rx.iter().collect(); // ends when the worker drops its sender
        drop(stream);
        assert_eq!(chunks.len(), 7, "6 full chunks of 160 and one of 40");
        assert!(chunks[..6].iter().all(|c| c.len() == CHUNK));
        assert_eq!(
            chunks.concat(),
            (0..1000).map(|i| i as f32).collect::<Vec<_>>()
        );
    }

    #[test]
    fn dropping_a_capture_stream_stops_it() {
        let audio = FakeAudio::with_clip(vec![0.0; CHUNK * 100_000]);
        let (tx, rx) = mpsc::channel();
        let stream = audio
            .open_capture(
                None,
                Box::new(move |_, _| {
                    let _ = tx.send(());
                }),
            )
            .unwrap();
        rx.recv().unwrap();
        drop(stream); // must return promptly instead of replaying the whole clip
        assert!(rx.iter().count() < 100_000);
    }

    #[test]
    fn playback_records_what_the_source_wrote() {
        let mut audio = FakeAudio::with_clip(Vec::new());
        audio.playback_chunks = 2;
        let stream = audio
            .open_playback(None, Box::new(|buf, _| buf.fill(0.25)))
            .unwrap();
        drop(stream);
        assert_eq!(*audio.played.lock().unwrap(), vec![0.25; CHUNK * 2]);
    }

    #[test]
    fn lists_one_default_device_each_way() {
        let audio = FakeAudio::with_clip(Vec::new());
        assert!(audio.input_devices().unwrap()[0].is_default);
        assert_eq!(audio.output_devices().unwrap().len(), 1);
    }
}
