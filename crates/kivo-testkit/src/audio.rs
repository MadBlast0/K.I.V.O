//! A fake audio device: capture replays a scripted clip on a thread; playback pulls a fixed number
//! of buffers from its source and records what it would have played, or, in real-time mode, keeps
//! pulling a 10 ms buffer every 10 ms like a speaker until the stream is dropped.

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
    /// Deliver the clip in real time, one 10 ms chunk every 10 ms, like a microphone (a virtual
    /// mic for end-to-end tests, BENCH-08), and play like a speaker, one 10 ms buffer every 10 ms.
    /// Otherwise the clip is delivered as fast as possible and playback pulls `playback_chunks`.
    pub realtime: bool,
    /// Clips for the next captures, one per opening of the microphone, before `clip` (a scripted
    /// microphone for a series of turns, BENCH-08).
    pub script: Mutex<std::collections::VecDeque<Vec<f32>>>,
}

impl FakeAudio {
    pub fn with_clip(clip: Vec<f32>) -> Self {
        Self {
            clip,
            playback_chunks: 1,
            played: Arc::default(),
            realtime: false,
            script: Mutex::default(),
        }
    }

    /// Queues `clip` for the next time the microphone opens.
    pub fn say_next(&self, clip: Vec<f32>) {
        self.script
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_back(clip);
    }

    /// A virtual microphone: the clip arrives at the pace speech does.
    pub fn microphone(clip: Vec<f32>) -> Self {
        Self {
            realtime: true,
            ..Self::with_clip(clip)
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
        let clip = self
            .script
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
            .unwrap_or_else(|| self.clip.clone());
        let realtime = self.realtime;
        Ok(Box::new(FakeStream::spawn(move |stop| {
            let started = std::time::Instant::now();
            for (i, chunk) in clip.chunks(CHUNK).enumerate() {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                if realtime {
                    // Chunk i is due 10 ms × i after the stream starts.
                    let due = std::time::Duration::from_millis(10 * u64::try_from(i).unwrap_or(0));
                    if let Some(wait) = due.checked_sub(started.elapsed()) {
                        std::thread::sleep(wait);
                    }
                }
                sink(chunk, FAKE_FORMAT);
            }
        })))
    }

    /// Pulls `playback_chunks` buffers synchronously before returning, so tests are deterministic;
    /// in real-time mode a thread pulls a buffer every 10 ms until the stream is dropped.
    fn open_playback(
        &self,
        _device: Option<&DeviceId>,
        mut source: FrameSource,
    ) -> PlatformResult<Box<dyn AudioStream>> {
        if self.realtime {
            let played = Arc::clone(&self.played);
            return Ok(Box::new(FakeStream::spawn(move |stop| {
                let mut buffer = [0.0f32; CHUNK];
                let started = std::time::Instant::now();
                let mut n: u64 = 0;
                while !stop.load(Ordering::SeqCst) {
                    buffer.fill(0.0);
                    source(&mut buffer, FAKE_FORMAT);
                    played
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .extend_from_slice(&buffer);
                    n += 1;
                    let due = std::time::Duration::from_millis(10 * n);
                    if let Some(wait) = due.checked_sub(started.elapsed()) {
                        std::thread::sleep(wait);
                    }
                }
            })));
        }
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
    fn a_virtual_microphone_delivers_at_the_pace_of_speech() {
        let audio = FakeAudio::microphone(vec![0.0; 16_000 / 5]);
        let (tx, rx) = mpsc::channel();
        let started = std::time::Instant::now();
        let stream = audio
            .open_capture(
                None,
                Box::new(move |frames, _| tx.send(frames.len()).unwrap()),
            )
            .unwrap();
        let samples: usize = rx.iter().sum();
        drop(stream);
        assert_eq!(samples, 3200);
        assert!(
            started.elapsed() >= std::time::Duration::from_millis(180),
            "200 ms of audio took {:?}",
            started.elapsed()
        );
    }

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
    fn a_realtime_speaker_keeps_playing_until_dropped() {
        let audio = FakeAudio::microphone(Vec::new());
        let stream = audio
            .open_playback(None, Box::new(|buf, _| buf.fill(0.5)))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
        drop(stream);
        let played = audio.played.lock().unwrap().len();
        assert!((CHUNK * 5..=CHUNK * 15).contains(&played), "{played}");
    }

    #[test]
    fn scripted_clips_play_one_per_capture_then_the_default() {
        let audio = FakeAudio::with_clip(vec![0.0; 10]);
        audio.say_next(vec![1.0; 5]);
        let capture = |audio: &FakeAudio| {
            let (tx, rx) = mpsc::channel();
            let stream = audio
                .open_capture(None, Box::new(move |f, _| tx.send(f.to_vec()).unwrap()))
                .unwrap();
            let all: Vec<f32> = rx.iter().flatten().collect();
            drop(stream);
            all
        };
        assert_eq!(capture(&audio), vec![1.0; 5]);
        assert_eq!(capture(&audio), vec![0.0; 10]);
    }

    #[test]
    fn lists_one_default_device_each_way() {
        let audio = FakeAudio::with_clip(Vec::new());
        assert!(audio.input_devices().unwrap()[0].is_default);
        assert_eq!(audio.output_devices().unwrap().len(), 1);
    }
}
