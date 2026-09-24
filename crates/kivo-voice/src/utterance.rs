//! Live results for engines that transcribe a whole utterance at a time (Moonshine, Parakeet,
//! Whisper): while the user is still speaking the audio so far is transcribed again every half
//! second, which gives partial transcripts; words two partials agree on become stable.

use crate::error::VoiceResult;
use crate::traits::{SAMPLE_RATE, SttEvent, SttStream};
use tokio_util::sync::CancellationToken;

/// How often a partial transcript is made while the user speaks.
pub const PARTIAL_EVERY_MS: u64 = 500;
const PARTIAL_EVERY: usize = SAMPLE_RATE as usize * PARTIAL_EVERY_MS as usize / 1000;
/// Too little audio to be worth a partial.
const MIN_AUDIO: usize = SAMPLE_RATE as usize / 4;

/// An engine that turns a whole utterance into text.
pub trait Transcriber: Send {
    fn transcribe(&mut self, audio: &[f32], cancel: &CancellationToken) -> VoiceResult<String>;
}

/// One utterance: audio so far and what has been shown.
pub struct Utterance<'a, T: Transcriber + ?Sized> {
    engine: &'a mut T,
    cancel: CancellationToken,
    audio: Vec<f32>,
    transcribed_at: usize,
    stable: String,
    last: String,
}

impl<'a, T: Transcriber + ?Sized> Utterance<'a, T> {
    pub fn new(engine: &'a mut T, cancel: CancellationToken) -> Self {
        Self {
            engine,
            cancel,
            audio: Vec::with_capacity(SAMPLE_RATE as usize * 10),
            transcribed_at: 0,
            stable: String::new(),
            last: String::new(),
        }
    }
}

impl<T: Transcriber + ?Sized> SttStream for Utterance<'_, T> {
    fn accept(&mut self, audio: &[f32]) -> VoiceResult<Vec<SttEvent>> {
        self.audio.extend_from_slice(audio);
        if self.audio.len() < MIN_AUDIO || self.audio.len() - self.transcribed_at < PARTIAL_EVERY {
            return Ok(Vec::new());
        }
        self.transcribed_at = self.audio.len();
        let text = self.engine.transcribe(&self.audio, &self.cancel)?;
        let mut events = Vec::new();
        // Words two partials agree on (all but the last) won't change any more.
        let stable = common_words(&self.last, &text);
        if stable.len() > self.stable.len() {
            self.stable.clone_from(&stable);
            events.push(SttEvent::Stable(stable));
        }
        if !text.is_empty() && text != self.last {
            events.push(SttEvent::Partial(text.clone()));
        }
        self.last = text;
        Ok(events)
    }

    fn finish(&mut self) -> VoiceResult<String> {
        self.engine.transcribe(&self.audio, &self.cancel)
    }
}

/// The leading words `a` and `b` share, leaving out the last word of the shorter one (it may still
/// be growing).
pub fn common_words(a: &str, b: &str) -> String {
    let shared: Vec<&str> = a
        .split_whitespace()
        .zip(b.split_whitespace())
        .take_while(|(x, y)| x == y)
        .map(|(x, _)| x)
        .collect();
    let shorter = a
        .split_whitespace()
        .count()
        .min(b.split_whitespace().count());
    let keep = if shared.len() == shorter {
        shared.len().saturating_sub(1)
    } else {
        shared.len()
    };
    shared[..keep].join(" ")
}

/// A 16-bit PCM mono WAV file's samples, for tests and fixtures.
pub fn wav_samples(bytes: &[u8]) -> Vec<f32> {
    // Find the "data" chunk rather than assuming a 44-byte header.
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let len = u32::from_le_bytes([bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]])
            as usize;
        if id == b"data" {
            let end = (at + 8 + len).min(bytes.len());
            return bytes[at + 8..end]
                .chunks_exact(2)
                .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32_768.0)
                .collect();
        }
        at += 8 + len + (len & 1);
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_words_leave_the_last_one_out() {
        assert_eq!(common_words("open the", "open the door"), "open");
        assert_eq!(common_words("open the door", "open the window"), "open the");
        assert_eq!(common_words("", "hello"), "");
    }
}
