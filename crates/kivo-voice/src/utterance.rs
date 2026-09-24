//! Live results for engines that transcribe a whole utterance at a time (Moonshine, Parakeet,
//! Whisper): while the user is still speaking the audio so far is transcribed again every half
//! second, which gives partial transcripts; words two partials agree on become stable.
//!
//! Engines with a longest input (Moonshine's model fails past about 9.5 s, Whisper hears 30 s at
//! a time) get long speech in segments, each cut at the quietest moment near its end; finished
//! segments keep their text, so a partial only transcribes the segment still growing.

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

    /// The longest audio it can take at once, in samples; longer speech is cut into segments.
    fn max_samples(&self) -> Option<usize> {
        None
    }
}

/// Segments are cut at the quietest 20 ms in their last 40%.
const QUIET_WINDOW: usize = SAMPLE_RATE as usize / 50;

/// Where to end a segment of `audio` that may be at most `max` long: the quietest moment in its
/// last 40%, so a cut falls between words when it can.
pub fn quiet_cut(audio: &[f32], max: usize) -> usize {
    let end = audio.len().min(max);
    let from = end * 3 / 5;
    let mut best = end;
    let mut quietest = f32::MAX;
    let mut at = from;
    while at + QUIET_WINDOW <= end {
        let energy: f32 = audio[at..at + QUIET_WINDOW].iter().map(|s| s * s).sum();
        if energy < quietest {
            quietest = energy;
            best = at + QUIET_WINDOW / 2;
        }
        at += QUIET_WINDOW;
    }
    best
}

fn join(done: &str, more: &str) -> String {
    match (done.is_empty(), more.trim().is_empty()) {
        (true, _) => more.trim().to_owned(),
        (false, true) => done.to_owned(),
        (false, false) => format!("{done} {}", more.trim()),
    }
}

/// Transcribes `audio` of any length in segments the engine can take (tests, one-off use).
pub fn transcribe_long<T: Transcriber + ?Sized>(
    engine: &mut T,
    audio: &[f32],
    cancel: &CancellationToken,
) -> VoiceResult<String> {
    let Some(max) = engine.max_samples() else {
        return engine.transcribe(audio, cancel);
    };
    let mut text = String::new();
    let mut at = 0;
    while audio.len() - at > max {
        let cut = at + quiet_cut(&audio[at..], max);
        text = join(&text, &engine.transcribe(&audio[at..cut], cancel)?);
        at = cut;
    }
    Ok(join(&text, &engine.transcribe(&audio[at..], cancel)?))
}

/// One utterance: audio so far and what has been shown.
pub struct Utterance<'a, T: Transcriber + ?Sized> {
    engine: &'a mut T,
    cancel: CancellationToken,
    audio: Vec<f32>,
    transcribed_at: usize,
    stable: String,
    last: String,
    /// Finished segments of long speech: where the growing one starts, and their text.
    done_at: usize,
    done_text: String,
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
            done_at: 0,
            done_text: String::new(),
        }
    }

    /// The whole utterance's text: finished segments are closed off (and kept) while the
    /// growing one is longer than the engine takes, then the growing one is transcribed.
    fn text(&mut self) -> VoiceResult<String> {
        if let Some(max) = self.engine.max_samples() {
            while self.audio.len() - self.done_at > max {
                let cut = self.done_at + quiet_cut(&self.audio[self.done_at..], max);
                let segment = self
                    .engine
                    .transcribe(&self.audio[self.done_at..cut], &self.cancel)?;
                self.done_text = join(&self.done_text, &segment);
                self.done_at = cut;
            }
        }
        let rest = self
            .engine
            .transcribe(&self.audio[self.done_at..], &self.cancel)?;
        Ok(join(&self.done_text, &rest))
    }
}

impl<T: Transcriber + ?Sized> SttStream for Utterance<'_, T> {
    fn accept(&mut self, audio: &[f32]) -> VoiceResult<Vec<SttEvent>> {
        self.audio.extend_from_slice(audio);
        if self.audio.len() < MIN_AUDIO || self.audio.len() - self.transcribed_at < PARTIAL_EVERY {
            return Ok(Vec::new());
        }
        self.transcribed_at = self.audio.len();
        let text = self.text()?;
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
        self.text()
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

    /// Hears only up to `max` samples, like Moonshine: longer input is an error. Its text is one
    /// word per second of audio.
    struct Limited {
        max: usize,
        calls: Vec<usize>,
    }

    impl Transcriber for Limited {
        fn transcribe(&mut self, audio: &[f32], _: &CancellationToken) -> VoiceResult<String> {
            assert!(
                audio.len() <= self.max,
                "{} is over {}",
                audio.len(),
                self.max
            );
            self.calls.push(audio.len());
            Ok(vec!["word"; audio.len() / SAMPLE_RATE as usize].join(" "))
        }

        fn max_samples(&self) -> Option<usize> {
            Some(self.max)
        }
    }

    /// Speech with a pause every `every` seconds.
    fn speech(seconds: usize, every: usize) -> Vec<f32> {
        let rate = SAMPLE_RATE as usize;
        (0..seconds * rate)
            .map(|i| {
                let t = i % (every * rate);
                if t < rate / 5 { 0.0 } else { 0.3 }
            })
            .collect()
    }

    #[test]
    fn long_speech_is_heard_in_segments_cut_at_pauses() {
        let rate = SAMPLE_RATE as usize;
        let mut engine = Limited {
            max: 9 * rate,
            calls: Vec::new(),
        };
        let audio = speech(25, 4);
        let text = transcribe_long(&mut engine, &audio, &CancellationToken::new()).unwrap();
        // Every segment fit, and every cut fell in a pause (a multiple of 4 s).
        assert!(engine.calls.len() >= 3, "{:?}", engine.calls);
        let mut at = 0;
        for len in &engine.calls[..engine.calls.len() - 1] {
            at += len;
            let into_pause = at % (4 * rate);
            assert!(into_pause < rate / 5, "cut at {at} isn't in a pause");
        }
        assert_eq!(engine.calls.iter().sum::<usize>(), audio.len());
        assert!(text.split_whitespace().count() >= 20, "{text}");
    }

    #[test]
    fn partials_keep_finished_segments_and_only_redo_the_growing_one() {
        let rate = SAMPLE_RATE as usize;
        let mut engine = Limited {
            max: 9 * rate,
            calls: Vec::new(),
        };
        let audio = speech(20, 4);
        let text = {
            let mut utterance = Utterance::new(&mut engine, CancellationToken::new());
            for chunk in audio.chunks(rate / 10) {
                utterance.accept(chunk).unwrap();
            }
            utterance.finish().unwrap()
        };
        // No call was ever over the limit (the engine asserts it), and the finished segments
        // weren't transcribed again for every partial: at most a few calls are long.
        let long_calls = engine.calls.iter().filter(|&&n| n > 7 * rate).count();
        assert!(long_calls < 12, "{long_calls} long calls");
        assert!(text.split_whitespace().count() >= 15, "{text}");
    }

    #[test]
    fn stable_words_leave_the_last_one_out() {
        assert_eq!(common_words("open the", "open the door"), "open");
        assert_eq!(common_words("open the door", "open the window"), "open the");
        assert_eq!(common_words("", "hello"), "");
    }
}
