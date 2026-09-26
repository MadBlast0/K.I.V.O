//! The capture path (VOICE §1, VOICE-03). The microphone callback runs on the audio thread (MMCSS
//! "Audio"); it downmixes, converts to 16 kHz and writes a lock-free ring buffer of several
//! seconds, then wakes the detection thread. The detection thread reads the ring at its own pace.
//! Nothing on the audio thread blocks or allocates after the first call.

use crate::resample::{RateConverter, downmix};
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::Thread;
use std::time::Duration;

/// 16 kHz mono.
pub const RATE: u32 = 16_000;

struct Shared {
    /// The detection thread, woken when audio arrives.
    reader: OnceLock<Thread>,
    /// The loudest RMS since the level was last read (for the Island's waveform).
    peak: AtomicU32,
    /// Samples dropped because the reader fell behind by more than the ring holds.
    dropped: AtomicU64,
}

/// The audio-thread side.
pub struct CaptureWriter {
    shared: Arc<Shared>,
    ring: HeapProd<f32>,
    converter: Option<(u32, RateConverter)>,
    mono: Vec<f32>,
}

/// The detection-thread side.
pub struct CaptureReader {
    shared: Arc<Shared>,
    ring: HeapCons<f32>,
}

/// A capture ring holding `seconds` of 16 kHz audio (VOICE-03: at least 3 s).
pub fn capture_ring(seconds: u32) -> (CaptureWriter, CaptureReader) {
    let (prod, cons) = HeapRb::<f32>::new((RATE * seconds) as usize).split();
    let shared = Arc::new(Shared {
        reader: OnceLock::new(),
        peak: AtomicU32::new(0),
        dropped: AtomicU64::new(0),
    });
    (
        CaptureWriter {
            shared: Arc::clone(&shared),
            ring: prod,
            converter: None,
            mono: Vec::with_capacity(4096),
        },
        CaptureReader { shared, ring: cons },
    )
}

impl CaptureWriter {
    /// Takes interleaved device audio at `rate` with `channels` (call from the capture callback).
    pub fn write(&mut self, interleaved: &[f32], rate: u32, channels: u16) {
        downmix(interleaved, channels, &mut self.mono);
        let rms = rms(&self.mono);
        let _ = self
            .shared
            .peak
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |bits| {
                (rms > f32::from_bits(bits)).then_some(rms.to_bits())
            });
        if self.converter.as_ref().is_none_or(|(r, _)| *r != rate) {
            self.converter = Some((rate, RateConverter::new(rate, RATE)));
        }
        let (ring, shared, mono) = (&mut self.ring, &self.shared, &self.mono);
        if let Some((_, converter)) = &mut self.converter {
            converter.push(mono, &mut |pcm| {
                let pushed = ring.push_slice(pcm);
                if pushed < pcm.len() {
                    shared
                        .dropped
                        .fetch_add((pcm.len() - pushed) as u64, Ordering::Relaxed);
                }
            });
        }
        if let Some(reader) = self.shared.reader.get() {
            reader.unpark();
        }
    }
}

impl CaptureReader {
    /// Moves every available sample into `out` (appending).
    pub fn read(&mut self, out: &mut Vec<f32>) {
        let available = self.ring.occupied_len();
        let start = out.len();
        out.resize(start + available, 0.0);
        let got = self.ring.pop_slice(&mut out[start..]);
        out.truncate(start + got);
    }

    /// Sleeps until audio arrives or `timeout` passes. Must always be called from the same thread.
    pub fn wait(&self, timeout: Duration) {
        let _ = self.shared.reader.set(std::thread::current());
        if self.ring.is_empty() {
            std::thread::park_timeout(timeout);
        }
    }

    /// The loudest RMS since the last call (resets it).
    pub fn take_peak(&self) -> f32 {
        f32::from_bits(self.shared.peak.swap(0, Ordering::Relaxed))
    }

    pub fn dropped(&self) -> u64 {
        self.shared.dropped.load(Ordering::Relaxed)
    }

    /// Discards everything buffered (a new listening session starts from now).
    pub fn discard(&mut self) {
        self.ring.clear();
    }
}

pub(crate) fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss, reason = "a frame length")]
    let n = samples.len() as f32;
    (samples.iter().map(|s| s * s).sum::<f32>() / n).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_48k_arrives_as_16k_mono() {
        let (mut w, mut r) = capture_ring(3);
        let stereo: Vec<f32> = (0..48_000 * 2)
            .map(|i| if i % 2 == 0 { 0.2 } else { 0.0 })
            .collect();
        for chunk in stereo.chunks(960) {
            w.write(chunk, 48_000, 2);
        }
        let mut out = Vec::new();
        r.read(&mut out);
        assert!((15_800..=16_000).contains(&out.len()), "{}", out.len());
        assert!((out[8000] - 0.1).abs() < 0.01, "downmixed to the average");
        assert!((r.take_peak() - 0.1).abs() < 0.01);
        assert_eq!(r.take_peak(), 0.0, "the peak resets");
    }

    #[test]
    fn a_reader_that_falls_behind_loses_the_oldest_audio_and_counts_it() {
        let (mut w, r) = capture_ring(1);
        let second = vec![0.1_f32; 16_000];
        w.write(&second, 16_000, 1);
        w.write(&second[..1600], 16_000, 1);
        assert_eq!(r.dropped(), 1600);
    }

    #[test]
    fn the_reader_wakes_when_audio_arrives() {
        let (mut w, mut r) = capture_ring(1);
        let reader = std::thread::spawn(move || {
            let started = std::time::Instant::now();
            let mut out = Vec::new();
            while out.is_empty() {
                r.wait(Duration::from_secs(5));
                r.read(&mut out);
            }
            started.elapsed()
        });
        std::thread::sleep(Duration::from_millis(50));
        w.write(&[0.5; 160], 16_000, 1);
        assert!(reader.join().unwrap() < Duration::from_secs(2));
    }
}
