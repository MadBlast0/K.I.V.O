//! `stt` (BENCHMARKS §1, BENCH-02): the speech-to-text candidates from VOICE §3, through
//! sherpa-onnx on the CPU. Per engine and run: cold load, the time from the end of an utterance to
//! its final transcript, real-time factor, word error rate and peak memory, on the same
//! LibriSpeech test-clean subset every time.
//!
//! `KIVO_BENCH_STT_ENGINES` (comma-separated: moonshine, parakeet, whisper) picks engines and
//! `KIVO_BENCH_STT_UTTERANCES` the subset size (default 40).

use super::data::{self, Utterance};
use super::wer::Tally;
use crate::harness::{Sample, Suite};
use crate::win;
use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig};
use std::time::Instant;

/// Inference threads per engine: what a laptop can spare while the user works.
pub const THREADS: i32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    Moonshine,
    Parakeet,
    Whisper,
}

impl Engine {
    pub const ALL: [Engine; 3] = [Engine::Moonshine, Engine::Parakeet, Engine::Whisper];

    pub fn name(self) -> &'static str {
        match self {
            Engine::Moonshine => "moonshine",
            Engine::Parakeet => "parakeet",
            Engine::Whisper => "whisper",
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            Engine::Moonshine => "Moonshine v2 base EN, quantized (2026-02-27)",
            Engine::Parakeet => "Parakeet TDT 0.6B v3, int8",
            Engine::Whisper => "Whisper large-v3-turbo",
        }
    }

    fn folder(self) -> &'static str {
        match self {
            Engine::Moonshine => "sherpa-onnx-moonshine-base-en-quantized-2026-02-27",
            Engine::Parakeet => "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8",
            Engine::Whisper => "sherpa-onnx-whisper-turbo",
        }
    }

    /// The recognizer configuration for this engine's files.
    pub fn config(self) -> Result<OfflineRecognizerConfig, String> {
        let dir = data::model(self.folder())?;
        let mut config = OfflineRecognizerConfig::default();
        let model = &mut config.model_config;
        model.tokens = Some(data::file(&dir, &["tokens"])?);
        model.num_threads = THREADS;
        model.provider = Some("cpu".into());
        // Prefer the int8 files where an engine ships both.
        let pick = |part: &str| {
            data::file(&dir, &[part, "int8"]).or_else(|_| data::file(&dir, &[part, ".onnx"]))
        };
        match self {
            Engine::Moonshine => {
                model.moonshine.encoder =
                    Some(data::file(&dir, &["encoder"]).map_err(|e| e.to_string())?);
                model.moonshine.merged_decoder = Some(data::file(&dir, &["decoder"])?);
            }
            Engine::Parakeet => {
                model.transducer.encoder = Some(pick("encoder")?);
                model.transducer.decoder = Some(pick("decoder")?);
                model.transducer.joiner = Some(pick("joiner")?);
                model.model_type = Some("nemo_transducer".into());
            }
            Engine::Whisper => {
                model.whisper.encoder = Some(pick("encoder")?);
                model.whisper.decoder = Some(pick("decoder")?);
                model.whisper.language = Some("en".into());
                model.whisper.task = Some("transcribe".into());
                model.whisper.tail_paddings = -1;
            }
        }
        Ok(config)
    }

    fn from_name(name: &str) -> Option<Engine> {
        Engine::ALL.into_iter().find(|e| e.name() == name.trim())
    }
}

pub struct Stt {
    engines: Vec<Engine>,
    utterances: Vec<Utterance>,
    audio_seconds: f64,
}

impl Stt {
    pub fn start() -> Result<Self, String> {
        let engines = match std::env::var("KIVO_BENCH_STT_ENGINES") {
            Ok(list) => list
                .split(',')
                .map(|n| Engine::from_name(n).ok_or(format!("unknown engine {n}")))
                .collect::<Result<Vec<_>, _>>()?,
            Err(_) => Engine::ALL.to_vec(),
        };
        // Check every model is there before spending minutes on the first.
        for engine in &engines {
            engine.config()?;
        }
        let count = std::env::var("KIVO_BENCH_STT_UTTERANCES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(40);
        let utterances = data::librispeech(count)?;
        let audio_seconds = utterances.iter().map(Utterance::seconds).sum();
        Ok(Self {
            engines,
            utterances,
            audio_seconds,
        })
    }
}

fn private_mb() -> f64 {
    #[allow(clippy::cast_precision_loss, reason = "memory in MB")]
    win::process_usage(std::process::id()).map_or(0.0, |u| u.committed as f64 / 1_048_576.0)
}

impl Suite for Stt {
    fn name(&self) -> &'static str {
        "stt"
    }

    fn run(&mut self) -> Result<Vec<Sample>, String> {
        let mut samples = Vec::new();
        for &engine in &self.engines {
            let name = engine.name();
            let before = private_mb();
            let t = Instant::now();
            let recognizer = OfflineRecognizer::create(&engine.config()?)
                .ok_or(format!("{name}: the model failed to load"))?;
            let load_ms = t.elapsed().as_secs_f64() * 1000.0;

            let mut tally = Tally::default();
            let mut latencies = Vec::with_capacity(self.utterances.len());
            let mut peak = private_mb();
            for utterance in &self.utterances {
                let stream = recognizer.create_stream();
                stream.accept_waveform(16_000, &utterance.samples);
                // Offline engines start when the utterance has ended: decode time is the delay
                // from the end of speech to the final transcript.
                let t = Instant::now();
                recognizer.decode(&stream);
                latencies.push(t.elapsed().as_secs_f64() * 1000.0);
                let text = stream.get_result().map(|r| r.text).unwrap_or_default();
                tally.add(&utterance.text, &text);
                peak = peak.max(private_mb());
            }
            drop(recognizer);

            let total_ms: f64 = latencies.iter().sum();
            latencies.sort_by(f64::total_cmp);
            samples.extend([
                Sample::cost(format!("{name}: cold load"), "ms", load_ms),
                Sample::cost(
                    format!("{name}: end of speech → final (median utterance)"),
                    "ms",
                    latencies[latencies.len() / 2],
                ),
                Sample::cost(
                    format!("{name}: end of speech → final (p95 utterance)"),
                    "ms",
                    latencies[latencies.len() * 95 / 100],
                ),
                Sample::cost(
                    format!("{name}: real-time factor"),
                    "×",
                    total_ms / 1000.0 / self.audio_seconds,
                ),
                Sample::cost(format!("{name}: WER"), "%", tally.percent()),
                Sample::cost(
                    format!("{name}: peak memory added"),
                    "MB",
                    (peak - before).max(0.0),
                ),
            ]);
        }
        Ok(samples)
    }

    fn notes(&self) -> Vec<String> {
        let mut notes = vec![format!(
            "LibriSpeech test-clean, first {} utterances ({:.0} s of speech), 16 kHz; WER after \
             lower-casing and removing punctuation (engines that write numbers as digits lose a \
             little against the spelled-out references).",
            self.utterances.len(),
            self.audio_seconds
        )];
        notes.extend(self.engines.iter().map(|e| {
            format!(
                "{}: {} through sherpa-onnx 1.13.8 (ONNX Runtime, CPU, {THREADS} threads).",
                e.name(),
                e.describe()
            )
        }));
        notes.push(
            "These engines decode whole utterances, so there is no first-partial time; a \
             streaming engine or chunked decoding (M1) will report it. Accented-English WER \
             needs a licensed test set (L2-ARCTIC or Common Voice) and is not measured yet."
                .into(),
        );
        notes
    }
}
