//! Framing (VOICE §1): audio moves in 10 ms frames internally and is batched to 80 ms for models
//! (fewer model calls is the idle-power lever); Silero takes 32 ms. `Chunker` cuts a stream into
//! fixed-size frames, and `History` keeps the last few seconds for pre-roll (VOICE-05).

use std::collections::VecDeque;

/// 16 kHz samples in 10 ms.
pub const FRAME_10MS: usize = 160;
/// 16 kHz samples in 80 ms: one batch for the models.
pub const BATCH_80MS: usize = FRAME_10MS * 8;

/// Cuts a stream into frames of exactly `size` samples.
pub struct Chunker {
    size: usize,
    buf: Vec<f32>,
}

impl Chunker {
    pub fn new(size: usize) -> Self {
        Self {
            size,
            buf: Vec::with_capacity(size),
        }
    }

    /// Adds audio; calls `frame` for every complete frame.
    pub fn push(&mut self, mut audio: &[f32], frame: &mut dyn FnMut(&[f32])) {
        while !audio.is_empty() {
            let take = (self.size - self.buf.len()).min(audio.len());
            self.buf.extend_from_slice(&audio[..take]);
            audio = &audio[take..];
            if self.buf.len() == self.size {
                frame(&self.buf);
                self.buf.clear();
            }
        }
    }

    /// Returns the incomplete frame, if any, and starts over.
    pub fn take_rest(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.buf)
    }

    pub fn clear(&mut self) {
        self.buf.clear();
    }
}

/// The last `capacity` samples.
pub struct History {
    capacity: usize,
    samples: VecDeque<f32>,
}

impl History {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            samples: VecDeque::with_capacity(capacity),
        }
    }

    pub fn push(&mut self, audio: &[f32]) {
        let audio = &audio[audio.len().saturating_sub(self.capacity)..];
        let overflow = (self.samples.len() + audio.len()).saturating_sub(self.capacity);
        self.samples.drain(..overflow);
        self.samples.extend(audio);
    }

    /// The last `n` samples (fewer if the history is shorter).
    pub fn last(&self, n: usize) -> Vec<f32> {
        let skip = self.samples.len().saturating_sub(n);
        self.samples.iter().skip(skip).copied().collect()
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_are_exact_whatever_the_input_sizes() {
        let mut c = Chunker::new(4);
        let mut frames = Vec::new();
        c.push(&[1.0, 2.0, 3.0], &mut |f| frames.push(f.to_vec()));
        c.push(&[4.0, 5.0, 6.0, 7.0, 8.0, 9.0], &mut |f| {
            frames.push(f.to_vec())
        });
        assert_eq!(frames, [vec![1.0, 2.0, 3.0, 4.0], vec![5.0, 6.0, 7.0, 8.0]]);
        assert_eq!(c.take_rest(), [9.0]);
    }

    #[test]
    fn history_keeps_only_the_most_recent_samples() {
        let mut h = History::new(3);
        h.push(&[1.0, 2.0]);
        h.push(&[3.0, 4.0]);
        assert_eq!(h.last(10), [2.0, 3.0, 4.0]);
        assert_eq!(h.last(2), [3.0, 4.0]);
        h.push(&[5.0, 6.0, 7.0, 8.0]);
        assert_eq!(h.last(3), [6.0, 7.0, 8.0]);
    }
}
