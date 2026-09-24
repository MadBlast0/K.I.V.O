//! `stt` (BENCHMARKS §1, BENCH-02): KIVO's own speech-to-text engines (Moonshine, Parakeet,
//! Whisper, and whisper.cpp on the graphics card), run the way KIVO runs them: in the `kivo-infer`
//! speech worker. Per engine and run: cold load; spoken live, the first partial and the time from
//! the end of speech to the final transcript; given whole recordings, the time to the transcript
//! and the real-time factor; word error rate on the same LibriSpeech test-clean subset every time
//! and on accented English (EdAcc); and the worker's peak memory. (`Engine::config` keeps the
//! sherpa-onnx recognizer the `vad` suite hears with.)
//!
//! `KIVO_BENCH_STT_ENGINES` (comma-separated: moonshine, parakeet, whisper, and whisper.cpp's
//! whisper-cpp-small, whisper-cpp-turbo, whisper-cpp-base-en from `models/whisper-cpp`) picks engines,
//! `KIVO_BENCH_STT_UTTERANCES` the subset size (default 40) and `KIVO_BENCH_STT_GPU` a graphics
//! card for the engines that can use one.

use super::data::{self, Utterance};
use super::wer::Tally;
use crate::harness::{Sample, Suite};
use crate::win;
use kivo_runtime::infer::{self, Engines, Infer};
use sherpa_onnx::OfflineRecognizerConfig;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// Inference threads per engine: what a laptop can spare while the user works.
pub const THREADS: i32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    Moonshine,
    Parakeet,
    Whisper,
    /// whisper.cpp (Vulkan on the graphics card, else the processor): one of
    /// `kivo_voice::whisper_cpp::VARIANTS`.
    WhisperCpp(usize),
}

impl Engine {
    /// The engines a run measures by default (the processor's; whisper.cpp is named on purpose).
    pub const ALL: [Engine; 3] = [Engine::Moonshine, Engine::Parakeet, Engine::Whisper];

    pub fn name(self) -> &'static str {
        match self {
            Engine::Moonshine => "moonshine",
            Engine::Parakeet => "parakeet",
            Engine::Whisper => "whisper",
            Engine::WhisperCpp(i) => kivo_voice::whisper_cpp::VARIANTS[i].id,
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            Engine::Moonshine => "Moonshine v2 base EN, quantized (2026-02-27)",
            Engine::Parakeet => "Parakeet TDT 0.6B v3, int8",
            Engine::Whisper => "Whisper large-v3-turbo",
            Engine::WhisperCpp(i) => kivo_voice::whisper_cpp::VARIANTS[i].file,
        }
    }

    fn folder(self) -> &'static str {
        match self {
            Engine::Moonshine => "sherpa-onnx-moonshine-base-en-quantized-2026-02-27",
            Engine::Parakeet => "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8",
            Engine::Whisper => "sherpa-onnx-whisper-turbo",
            Engine::WhisperCpp(_) => "whisper-cpp",
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
            Engine::WhisperCpp(_) => return Err("sherpa-onnx has no whisper.cpp".into()),
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

    /// KIVO's engine id for it.
    fn kivo_id(self) -> &'static str {
        match self {
            Engine::Moonshine => "moonshine-base-en",
            Engine::Parakeet => "parakeet-tdt-v3",
            Engine::Whisper => "whisper-large-v3-turbo",
            Engine::WhisperCpp(i) => kivo_voice::whisper_cpp::VARIANTS[i].id,
        }
    }

    fn from_name(name: &str) -> Option<Engine> {
        let cpp = (0..kivo_voice::whisper_cpp::VARIANTS.len()).map(Engine::WhisperCpp);
        Engine::ALL
            .into_iter()
            .chain(cpp)
            .find(|e| e.name() == name.trim())
    }

    /// Far slower than real time on the processor: a smaller share of the set.
    fn slow_on_cpu(self) -> bool {
        matches!(self, Engine::Whisper | Engine::WhisperCpp(1))
    }
}

/// Utterances spoken live (fed at the speed they were spoken), and the piece size.
const LIVE_UTTERANCES: usize = 5;
const CHUNK: usize = 1_280;
const CHUNK_TIME: Duration = Duration::from_millis(80);

/// The first few utterances as a microphone delivers them (80 ms at a time, in real time), the
/// way KIVO hears a request: the time from the start of speech to the first partial transcript,
/// and from the end of speech to the final one (VOICE §10's "end of speech → final"). When no
/// partial arrives before the end, the final is the first text.
async fn live(
    infer: &Infer,
    events: &mut tokio::sync::mpsc::UnboundedReceiver<infer::InferEvent>,
    utterances: &[Utterance],
    name: &str,
) -> Result<(Vec<f64>, Vec<f64>), String> {
    let (mut firsts, mut finals) = (Vec::new(), Vec::new());
    for utterance in utterances.iter().take(LIVE_UTTERANCES) {
        let id = infer.next_utterance();
        infer
            .start_stt(id, "en", Vec::new())
            .await
            .map_err(|e| format!("{name}: {e}"))?;
        let started = tokio::time::Instant::now();
        let mut first = None;
        for (i, chunk) in utterance.samples.chunks(CHUNK).enumerate() {
            let due = started + CHUNK_TIME * u32::try_from(i).unwrap_or(u32::MAX);
            // Until this piece is due, watch for the first partial.
            loop {
                match tokio::time::timeout_at(due, events.recv()).await {
                    Ok(Some(infer::InferEvent::Partial { id: of, .. })) if of == id => {
                        first.get_or_insert(started.elapsed());
                    }
                    Ok(Some(_)) => {}
                    Ok(None) => return Err(format!("{name}: the worker went away")),
                    Err(_) => break,
                }
            }
            infer
                .send_audio(id, chunk)
                .map_err(|e| format!("{name}: {e}"))?;
        }
        // The speaker has stopped.
        let ended = Instant::now();
        infer
            .finish_stt(id)
            .await
            .map_err(|e| format!("{name}: {e}"))?;
        finals.push(ended.elapsed().as_secs_f64() * 1000.0);
        let first = first.unwrap_or_else(|| started.elapsed());
        firsts.push(first.as_secs_f64() * 1000.0);
    }
    Ok((firsts, finals))
}

/// One utterance through the worker with all its audio at once: the transcript, and the time
/// from the end of the audio to it.
async fn transcribe(
    infer: &Infer,
    utterance: &Utterance,
    name: &str,
) -> Result<(String, f64), String> {
    let id = infer.next_utterance();
    infer
        .start_stt(id, "en", Vec::new())
        .await
        .map_err(|e| format!("{name}: {e}"))?;
    for chunk in utterance.samples.chunks(CHUNK) {
        infer
            .send_audio(id, chunk)
            .map_err(|e| format!("{name}: {e}"))?;
    }
    // The utterance has ended: this is the delay to the final transcript.
    let t = Instant::now();
    let done = infer
        .finish_stt(id)
        .await
        .map_err(|e| format!("{name}: {e}"))?;
    Ok((done.text, t.elapsed().as_secs_f64() * 1000.0))
}

pub struct Stt {
    rt: tokio::runtime::Runtime,
    /// `KIVO_BENCH_STT_GPU=<adapter>`: GPU-capable engines run on that graphics card, as KIVO's
    /// GPU policy puts them (PLAN-09).
    gpu: Option<u32>,
    worker: PathBuf,
    engines: Vec<Engine>,
    utterances: Vec<Utterance>,
    audio_seconds: f64,
    /// EdAcc's accented English (`KIVO_BENCH_STT_ACCENTED` utterances, default 40; 0 skips it).
    accented: Vec<Utterance>,
    accents: usize,
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
            data::model(engine.folder())?;
        }
        let worker = std::env::current_exe()
            .map_err(|e| e.to_string())?
            .with_file_name("kivo-infer.exe");
        if !worker.is_file() {
            return Err(format!(
                "KIVO's speech worker must be beside kivo-bench (cargo build --release -p kivo-infer): {} is missing",
                worker.display()
            ));
        }
        let count = std::env::var("KIVO_BENCH_STT_UTTERANCES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(40);
        let utterances = data::librispeech(count)?;
        let audio_seconds = utterances.iter().map(Utterance::seconds).sum();
        let accented_count = std::env::var("KIVO_BENCH_STT_ACCENTED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(40);
        let (accented, accents) = if accented_count == 0 {
            (Vec::new(), 0)
        } else {
            super::edacc::utterances(accented_count)?
        };
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let gpu = std::env::var("KIVO_BENCH_STT_GPU")
            .ok()
            .and_then(|g| g.parse().ok());
        Ok(Self {
            rt,
            gpu,
            worker,
            engines,
            utterances,
            audio_seconds,
            accented,
            accents,
        })
    }

    /// One engine, loaded cold in a fresh worker, through every utterance.
    fn measure(&self, engine: Engine) -> Result<Vec<Sample>, String> {
        let name = engine.name();
        let dir = data::model(engine.folder())?;
        // Whisper on the processor is far slower than real time: a smaller share of the set
        // (`KIVO_BENCH_STT_SLOW_UTTERANCES`, default 10) keeps a run within minutes.
        let (utterances, accented) = if engine.slow_on_cpu() && self.gpu.is_none() {
            let n = std::env::var("KIVO_BENCH_STT_SLOW_UTTERANCES")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10_usize);
            (
                &self.utterances[..n.min(self.utterances.len())],
                &self.accented[..n.min(self.accented.len())],
            )
        } else {
            (&self.utterances[..], &self.accented[..])
        };
        let audio_seconds: f64 = utterances.iter().map(Utterance::seconds).sum();
        self.rt.block_on(async {
            let (infer, mut events, sender) = Infer::new(self.worker.clone());
            let stop = CancellationToken::new();
            let task = tokio::spawn(infer::supervise(infer.clone(), sender, stop.clone()));
            let t = Instant::now();
            infer.set_engines(Engines {
                stt: Some((engine.kivo_id().to_owned(), dir)),
                stt_fallback: None,
                tts: None,
                threads: usize::try_from(THREADS).unwrap_or(4),
                language: "en".into(),
                cloud: std::collections::BTreeMap::new(),
                gpu: self.gpu,
            });
            let ready = infer.wait_ready(Duration::from_secs(180)).await;
            let load_ms = t.elapsed().as_secs_f64() * 1000.0;
            let result = async {
                if !ready {
                    return Err(format!("{name}: KIVO's worker didn't load it"));
                }
                while let Ok(event) = events.try_recv() {
                    if let infer::InferEvent::Fallback { error, .. } = event {
                        return Err(format!("{name}: {error}"));
                    }
                }
                let pid = infer.worker_pid().ok_or(format!("{name}: no worker"))?;
                let memory = || {
                    #[allow(clippy::cast_precision_loss, reason = "memory in MB")]
                    win::process_usage(pid).map_or(0.0, |u| u.ram as f64 / 1_048_576.0)
                };
                let (firsts, live_finals) = live(&infer, &mut events, utterances, name).await?;
                let mut tally = Tally::default();
                let mut latencies = Vec::with_capacity(utterances.len());
                let mut peak = memory();
                for utterance in utterances {
                    let (text, ms) = transcribe(&infer, utterance, name).await?;
                    latencies.push(ms);
                    tally.add(&utterance.text, &text);
                    peak = peak.max(memory());
                }
                let mut accented_tally = Tally::default();
                for utterance in accented {
                    let (text, _) = transcribe(&infer, utterance, name).await?;
                    accented_tally.add(&utterance.text, &text);
                    peak = peak.max(memory());
                }
                Ok((tally, accented_tally, latencies, peak, firsts, live_finals))
            }
            .await;
            stop.cancel();
            let _ = tokio::time::timeout(Duration::from_secs(5), task).await;
            let (tally, accented_tally, mut latencies, peak, mut firsts, mut live_finals) = result?;
            firsts.sort_by(f64::total_cmp);
            live_finals.sort_by(f64::total_cmp);
            let total_ms: f64 = latencies.iter().sum();
            latencies.sort_by(f64::total_cmp);
            let mut samples = vec![
                Sample::cost(format!("{name}: cold load"), "ms", load_ms),
                Sample::cost(
                    format!("{name}: speech start → first partial (live, median utterance)"),
                    "ms",
                    firsts[firsts.len() / 2],
                ),
                Sample::cost(
                    format!("{name}: end of speech → final (live, median utterance)"),
                    "ms",
                    live_finals[live_finals.len() / 2],
                ),
                Sample::cost(
                    format!("{name}: all audio at once → final (median utterance)"),
                    "ms",
                    latencies[latencies.len() / 2],
                ),
                Sample::cost(
                    format!("{name}: all audio at once → final (p95 utterance)"),
                    "ms",
                    latencies[latencies.len() * 95 / 100],
                ),
                Sample::cost(
                    format!("{name}: real-time factor"),
                    "×",
                    total_ms / 1000.0 / audio_seconds,
                ),
                Sample::cost(format!("{name}: WER"), "%", tally.percent()),
            ];
            if !accented.is_empty() {
                samples.push(Sample::cost(
                    format!("{name}: WER, accented English (EdAcc)"),
                    "%",
                    accented_tally.percent(),
                ));
            }
            samples.push(Sample::cost(
                format!("{name}: worker peak memory"),
                "MB",
                peak,
            ));
            Ok(samples)
        })
    }
}

impl Suite for Stt {
    fn name(&self) -> &'static str {
        // Its own section and row: GPU numbers don't replace the processor's.
        if self.gpu.is_some() { "stt-gpu" } else { "stt" }
    }

    fn run(&mut self) -> Result<Vec<Sample>, String> {
        let mut samples = Vec::new();
        for &engine in &self.engines {
            samples.extend(self.measure(engine)?);
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
        let gpu = self.gpu.is_some();
        notes.extend(self.engines.iter().map(|e| {
            let runtime = match (e, gpu) {
                (Engine::WhisperCpp(_), true) => "whisper.cpp, Vulkan on the graphics card".into(),
                (Engine::WhisperCpp(_), false) => format!("whisper.cpp, CPU, {THREADS} threads"),
                (Engine::Parakeet, true) => "ONNX Runtime, encoder on DirectML".into(),
                _ => format!("ONNX Runtime, CPU, {THREADS} threads"),
            };
            format!(
                "{}: {} in KIVO's own engine (`kivo-voice`), in the `kivo-infer` worker as KIVO \
                 runs it ({runtime}); each run loads it cold in a fresh worker.",
                e.name(),
                e.describe()
            )
        }));
        if self.engines.iter().any(|e| e.slow_on_cpu()) && self.gpu.is_none() {
            notes.push(
                "Whisper large-v3-turbo on the processor hears only the first 10 utterances of each \
                 set: it runs far slower than real time there."
                    .into(),
            );
        }
        notes.push(format!(
            "Live: the first {LIVE_UTTERANCES} utterances arrive as a microphone delivers them \
             (80 ms at a time, in real time), as KIVO hears a request. KIVO transcribes the \
             speech so far every {} ms while it goes on, so the first partial is about that \
             interval plus one transcription, and at the end of speech only the last stretch is \
             left (VOICE §10: ≤ 300 ms). \"All audio at once\" gives the engine a whole recording \
             with nothing transcribed yet: the time a long file takes, and the real-time factor.",
            kivo_voice::utterance::PARTIAL_EVERY_MS
        ));
        if !self.accented.is_empty() {
            notes.push(format!(
                "Accented English: {} turns of 2–20 s from EdAcc (University of Edinburgh, \
                 CC BY-SA 4.0; test shard 4 at a pinned revision), taken in turn from {} accents. \
                 These are spontaneous conversation, not read text, so the WER is higher than \
                 LibriSpeech's for every engine; annotations such as <laugh> are removed from the \
                 references.",
                self.accented.len(),
                self.accents
            ));
        }
        notes
    }
}
