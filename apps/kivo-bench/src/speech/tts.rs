//! `tts` (BENCHMARKS §1, BENCH-03): Kokoro-82M (VOICE §3 "Natural") through sherpa-onnx on the
//! CPU. For a short, a medium and a long text: time to the first audio, real-time factor and
//! peak memory; and cancel-to-silence (stop requested at the first chunk → generation returns).
//!
//! Benchmark only: sherpa-onnx phonemizes with espeak-ng (GPL-3.0), which KIVO's shipped TTS must
//! not link (DECISIONS "Kokoro phonemizer"); the model's speed is what is measured here.

use super::data;
use crate::harness::{Sample, Suite};
use crate::win;
use sherpa_onnx::{GenerationConfig, OfflineTts, OfflineTtsConfig};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const FOLDER: &str = "kokoro-int8-en-v0_19";
const THREADS: i32 = 4;

/// Texts of increasing length, with numbers, a URL and code-like words (BENCHMARKS §1).
const TEXTS: [(&str, &str); 3] = [
    ("short", "Done. Chrome is open."),
    (
        "medium",
        "It's 4:30 PM. You have two meetings left today, and the build finished with 3 warnings.",
    ),
    (
        "long",
        "Here's what I found. The error comes from line 42 in src/main.rs: Config::load returns a \
         Result, but the variable is typed as Config. Adding a question mark fixes it. I also \
         checked https://docs.rs for the crate's changelog, and version 2.1 renamed the method. \
         Want me to make the change and run cargo check again?",
    ),
];

pub struct Tts {
    config: OfflineTtsConfig,
}

impl Tts {
    pub fn start() -> Result<Self, String> {
        let dir = data::model(FOLDER)?;
        let mut config = OfflineTtsConfig::default();
        let kokoro = &mut config.model.kokoro;
        kokoro.model = Some(data::file(&dir, &[".onnx"])?);
        kokoro.voices = Some(data::file(&dir, &["voices"])?);
        kokoro.tokens = Some(data::file(&dir, &["tokens"])?);
        kokoro.data_dir = Some(dir.join("espeak-ng-data").to_string_lossy().into_owned());
        config.model.num_threads = THREADS;
        config.model.provider = Some("cpu".into());
        // One sentence per chunk, so the first audio comes after the first sentence.
        config.max_num_sentences = 1;
        Ok(Self { config })
    }
}

fn private_mb() -> f64 {
    #[allow(clippy::cast_precision_loss, reason = "memory in MB")]
    win::process_usage(std::process::id()).map_or(0.0, |u| u.committed as f64 / 1_048_576.0)
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

impl Suite for Tts {
    fn name(&self) -> &'static str {
        "tts"
    }

    fn run(&mut self) -> Result<Vec<Sample>, String> {
        let before = private_mb();
        let t = Instant::now();
        let tts = OfflineTts::create(&self.config).ok_or("Kokoro failed to load")?;
        let mut samples = vec![Sample::cost("cold load", "ms", ms(t.elapsed()))];
        #[allow(clippy::cast_precision_loss, reason = "a sample rate")]
        let rate = tts.sample_rate() as f64;
        let generation = GenerationConfig::default();

        for (label, text) in TEXTS {
            let first: Arc<Mutex<Option<Instant>>> = Arc::default();
            let seen = Arc::clone(&first);
            let started = Instant::now();
            let audio = tts
                .generate_with_config(
                    text,
                    &generation,
                    Some(move |_chunk: &[f32], _progress: f32| {
                        if let Ok(mut first) = seen.lock() {
                            first.get_or_insert_with(Instant::now);
                        }
                        true
                    }),
                )
                .ok_or("generation failed")?;
            let total = started.elapsed();
            let first_audio = first
                .lock()
                .map_err(|_| "poisoned")?
                .map_or(total, |t| t.duration_since(started));
            #[allow(clippy::cast_precision_loss, reason = "sample counts")]
            let seconds = audio.samples().len() as f64 / rate;
            samples.extend([
                Sample::cost(format!("{label}: first audio"), "ms", ms(first_audio)),
                Sample::cost(
                    format!("{label}: real-time factor"),
                    "×",
                    total.as_secs_f64() / seconds.max(0.001),
                ),
            ]);
        }

        // Cancel: ask to stop as soon as the first chunk arrives, time until generation returns.
        let asked: Arc<Mutex<Option<Instant>>> = Arc::default();
        let mark = Arc::clone(&asked);
        tts.generate_with_config(
            TEXTS[2].1,
            &generation,
            Some(move |_chunk: &[f32], _progress: f32| {
                if let Ok(mut asked) = mark.lock() {
                    asked.get_or_insert_with(Instant::now);
                }
                false
            }),
        );
        let stopped = Instant::now();
        let cancel = asked
            .lock()
            .map_err(|_| "poisoned")?
            .map_or(0.0, |t| ms(stopped.duration_since(t)));
        samples.push(Sample::cost("cancel → silence", "ms", cancel));
        samples.push(Sample::cost(
            "peak memory added",
            "MB",
            (private_mb() - before).max(0.0),
        ));
        Ok(samples)
    }

    fn notes(&self) -> Vec<String> {
        vec![
            format!(
                "Kokoro-82M int8 (EN v0.19, voice 0) through sherpa-onnx 1.13.8 (ONNX Runtime, \
                 CPU, {THREADS} threads), one sentence per chunk, so first audio arrives after \
                 the first sentence is synthesized."
            ),
            "Texts: short (4 words), medium (numbers, a time), long (a path, code, a URL; 5 \
             sentences). Cancel is requested at the first chunk; the engine stops at chunk \
             boundaries, so cancel-to-silence here is the time to finish that chunk. KIVO's \
             player stops sound immediately on cancel (VOICE §7) regardless."
                .into(),
            "Benchmark only: sherpa-onnx phonemizes with espeak-ng (GPL-3.0).".into(),
        ]
    }
}
