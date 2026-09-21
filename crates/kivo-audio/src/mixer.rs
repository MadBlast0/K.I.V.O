//! The playback mixer (VOICE §1, §6–7): speech from the TTS engine and short earcons, summed into
//! the speaker's buffer on the audio thread. Producers convert their audio to the device format
//! before queueing it, so the audio thread only copies and sums. Stopping speech fades it out over
//! 30 ms instead of cutting it (VOICE §7), and the speech level drives the Island's pulse.

use crate::resample::{RateConverter, upmix};
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

/// Seconds of device audio each track can hold ahead of playback.
const SPEECH_SECONDS: usize = 30;
const CUE_SECONDS: usize = 2;
/// The fade when speech is stopped (VOICE §7).
const FADE_MS: usize = 30;

/// The speaker's format, as the stream reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceFormat {
    pub rate: u32,
    pub channels: u16,
}

struct Track {
    prod: Mutex<HeapProd<f32>>,
    cons: Mutex<HeapCons<f32>>,
}

impl Track {
    fn new(samples: usize) -> Self {
        let (prod, cons) = HeapRb::<f32>::new(samples).split();
        Self {
            prod: Mutex::new(prod),
            cons: Mutex::new(cons),
        }
    }
}

struct Shared {
    format: DeviceFormat,
    speech: Track,
    cue: Track,
    /// Set by `stop_speech`; the audio thread fades the speech out, clears it and resets this.
    stopping: AtomicBool,
    /// Speech gain in 1/1000 (barge-in ducking, M2).
    gain: AtomicU32,
    /// Loudest speech RMS since last read.
    peak: AtomicU32,
}

/// The mixer for one open playback stream.
#[derive(Clone)]
pub struct Mixer(Arc<Shared>);

impl Mixer {
    pub fn new(format: DeviceFormat) -> Self {
        let per_second = format.rate as usize * usize::from(format.channels.max(1));
        Self(Arc::new(Shared {
            format,
            speech: Track::new(per_second * SPEECH_SECONDS),
            cue: Track::new(per_second * CUE_SECONDS),
            stopping: AtomicBool::new(false),
            gain: AtomicU32::new(1000),
            peak: AtomicU32::new(0),
        }))
    }

    pub fn format(&self) -> DeviceFormat {
        self.0.format
    }

    /// Converts mono audio at `rate` to the device format.
    pub fn to_device(&self, mono: &[f32], rate: u32) -> Vec<f32> {
        let converted = RateConverter::convert_all(rate, self.0.format.rate, mono);
        let mut out = Vec::new();
        upmix(&converted, self.0.format.channels, &mut out);
        out
    }

    /// Queues speech already in the device format. Returns how many samples fit.
    pub fn queue_speech(&self, device_audio: &[f32]) -> usize {
        self.0.stopping.store(false, Ordering::Relaxed);
        lock(&self.0.speech.prod).push_slice(device_audio)
    }

    /// Queues an earcon already in the device format; a new cue replaces one still playing.
    pub fn play_cue(&self, device_audio: &[f32]) {
        if let Ok(mut cons) = self.0.cue.cons.lock() {
            cons.clear();
        }
        lock(&self.0.cue.prod).push_slice(device_audio);
    }

    /// Fades the speech out and drops what's queued (cancel → silence, VOICE §7).
    pub fn stop_speech(&self) {
        self.0.stopping.store(true, Ordering::Relaxed);
    }

    /// Speech gain 0–1 (ducking while the user talks over KIVO).
    pub fn set_speech_gain(&self, gain: f32) {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped"
        )]
        self.0
            .gain
            .store((gain.clamp(0.0, 1.0) * 1000.0) as u32, Ordering::Relaxed);
    }

    /// Samples of speech still queued.
    pub fn speech_queued(&self) -> usize {
        self.0.speech.cons.lock().map_or(0, |c| c.occupied_len())
    }

    /// True while anything is still to be heard.
    pub fn busy(&self) -> bool {
        let queued = |t: &Track| t.cons.lock().map_or(0, |c| c.occupied_len());
        queued(&self.0.speech) + queued(&self.0.cue) > 0
    }

    /// The loudest speech RMS since the last call.
    pub fn take_speech_peak(&self) -> f32 {
        f32::from_bits(self.0.peak.swap(0, Ordering::Relaxed))
    }

    /// Fills one device buffer (call from the playback callback). Never blocks: if a producer
    /// holds a track at this instant, that track is silent for one buffer.
    pub fn fill(&self, out: &mut [f32]) {
        out.fill(0.0);
        let shared = &self.0;
        #[allow(clippy::cast_precision_loss, reason = "gain in thousandths")]
        let gain = shared.gain.load(Ordering::Relaxed) as f32 / 1000.0;
        if let Ok(mut speech) = shared.speech.cons.try_lock() {
            let got = speech.pop_slice(out);
            if shared.stopping.load(Ordering::Relaxed) {
                let fade = (shared.format.rate as usize * FADE_MS / 1000)
                    * usize::from(shared.format.channels);
                let len = got.min(fade);
                #[allow(clippy::cast_precision_loss, reason = "a short fade")]
                for (i, s) in out[..len].iter_mut().enumerate() {
                    *s *= 1.0 - i as f32 / len as f32;
                }
                out[len..got].fill(0.0);
                speech.clear();
                shared.stopping.store(false, Ordering::Relaxed);
            }
            for s in &mut out[..got] {
                *s *= gain;
            }
            let level = crate::capture::rms(&out[..got]);
            let _ = shared
                .peak
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |bits| {
                    (level > f32::from_bits(bits)).then_some(level.to_bits())
                });
        }
        if let Ok(mut cue) = shared.cue.cons.try_lock() {
            let mut i = 0;
            while i < out.len() {
                let Some(s) = cue.try_pop() else { break };
                out[i] = (out[i] + s).clamp(-1.0, 1.0);
                i += 1;
            }
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORMAT: DeviceFormat = DeviceFormat {
        rate: 48_000,
        channels: 2,
    };

    #[test]
    fn speech_and_cues_are_summed() {
        let mixer = Mixer::new(FORMAT);
        mixer.queue_speech(&[0.25; 8]);
        mixer.play_cue(&[0.5; 4]);
        let mut out = [0.0; 8];
        mixer.fill(&mut out);
        assert_eq!(out, [0.75, 0.75, 0.75, 0.75, 0.25, 0.25, 0.25, 0.25]);
        assert!(!mixer.busy());
    }

    #[test]
    fn stopping_fades_out_and_drops_the_rest() {
        let mixer = Mixer::new(FORMAT);
        mixer.queue_speech(&vec![0.5; 48_000]);
        mixer.stop_speech();
        let mut out = vec![0.0; 9600];
        mixer.fill(&mut out);
        assert_eq!(out[0], 0.5, "the fade starts at full level");
        assert!(out[2800] < 0.02, "nearly silent after 30 ms");
        assert!(out[3000..].iter().all(|&s| s == 0.0));
        assert_eq!(mixer.speech_queued(), 0);
        mixer.queue_speech(&[0.5; 4]);
        let mut next = [0.0; 4];
        mixer.fill(&mut next);
        assert_eq!(next, [0.5; 4], "new speech plays normally");
    }

    #[test]
    fn ducking_scales_speech_but_not_cues() {
        let mixer = Mixer::new(FORMAT);
        mixer.set_speech_gain(0.25);
        mixer.queue_speech(&[0.8; 2]);
        mixer.play_cue(&[0.1; 2]);
        let mut out = [0.0; 2];
        mixer.fill(&mut out);
        assert!((out[0] - 0.3).abs() < 1e-6);
        assert!(mixer.take_speech_peak() > 0.19);
    }

    #[test]
    fn audio_is_converted_to_the_device_format() {
        let mixer = Mixer::new(FORMAT);
        let device = mixer.to_device(&vec![0.1; 24_000], 24_000);
        assert_eq!(device.len(), 48_000 * 2);
    }
}
