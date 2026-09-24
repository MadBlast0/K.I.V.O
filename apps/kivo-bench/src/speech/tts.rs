//! `tts` (BENCHMARKS §1, BENCH-03): KIVO's own voices — Kokoro-82M (fp16, the "Natural" profile),
//! Supertonic 3 ("Multilingual") and the Windows voices ("Lightweight") — run the way KIVO runs
//! them: in the `kivo-infer` speech worker, on the CPU. Per voice and run: cold load; for a
//! short, a medium and a long text, the time to the first audio and the real-time factor;
//! cancel → silence (stop asked at the first audio → the worker's end of speech); and the
//! worker's peak memory.
//!
//! `KIVO_BENCH_TTS_ENGINES` (comma-separated: kokoro, supertonic, windows) picks voices.

use super::data;
use crate::harness::{Sample, Suite};
use crate::win;
use kivo_runtime::infer::{self, Engines, Infer, InferEvent};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

const THREADS: usize = 4;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Voice {
    Kokoro,
    Supertonic,
    Windows,
}

impl Voice {
    const ALL: [Voice; 3] = [Voice::Kokoro, Voice::Supertonic, Voice::Windows];

    fn name(self) -> &'static str {
        match self {
            Voice::Kokoro => "kokoro",
            Voice::Supertonic => "supertonic",
            Voice::Windows => "windows",
        }
    }

    fn kivo_id(self) -> &'static str {
        match self {
            Voice::Kokoro => "kokoro-82m",
            Voice::Supertonic => "supertonic-3",
            Voice::Windows => "system",
        }
    }

    fn describe(self) -> &'static str {
        match self {
            Voice::Kokoro => "Kokoro-82M v1.0, fp16 (the file KIVO downloads)",
            Voice::Supertonic => "Supertonic 3",
            Voice::Windows => "the Windows voices (SAPI/OneCore)",
        }
    }

    /// The model folder: KIVO's own download when it's installed, else the benchmark data's.
    fn folder(self) -> Result<Option<PathBuf>, String> {
        let bench = match self {
            Voice::Kokoro => "kivo-kokoro",
            Voice::Supertonic => "supertonic-3",
            Voice::Windows => return Ok(None),
        };
        if let Some(paths) = kivo_platform::Paths::user()
            && let Some(installed) =
                kivo_store::models::ModelStore::new(paths.models()).installed(self.kivo_id())
        {
            return Ok(Some(installed.dir));
        }
        data::model(bench).map(Some)
    }

    fn from_name(name: &str) -> Option<Voice> {
        Voice::ALL.into_iter().find(|v| v.name() == name.trim())
    }
}

pub struct Tts {
    rt: tokio::runtime::Runtime,
    worker: PathBuf,
    voices: Vec<Voice>,
}

impl Tts {
    pub fn start() -> Result<Self, String> {
        let voices = match std::env::var("KIVO_BENCH_TTS_ENGINES") {
            Ok(list) => list
                .split(',')
                .map(|n| Voice::from_name(n).ok_or(format!("unknown voice {n}")))
                .collect::<Result<Vec<_>, _>>()?,
            Err(_) => Voice::ALL.to_vec(),
        };
        for voice in &voices {
            voice.folder()?;
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
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self { rt, worker, voices })
    }

    fn measure(&self, voice: Voice) -> Result<Vec<Sample>, String> {
        let name = voice.name();
        let dir = voice.folder()?;
        self.rt.block_on(async {
            let (infer, mut events, sender) = Infer::new(self.worker.clone());
            let stop = CancellationToken::new();
            let task = tokio::spawn(infer::supervise(infer.clone(), sender, stop.clone()));
            let t = Instant::now();
            infer.set_engines(Engines {
                tts: Some((voice.kivo_id().to_owned(), dir)),
                threads: THREADS,
                language: "en".into(),
                ..Engines::default()
            });
            let ready = infer.wait_ready(Duration::from_secs(180)).await;
            let load_ms = t.elapsed().as_secs_f64() * 1000.0;
            let result = async {
                if !ready {
                    return Err(format!("{name}: KIVO's worker didn't load it"));
                }
                while let Ok(event) = events.try_recv() {
                    if let InferEvent::Fallback { error, .. } = event {
                        return Err(format!("{name}: {error}"));
                    }
                }
                let pid = infer.worker_pid().ok_or(format!("{name}: no worker"))?;
                let memory = || {
                    #[allow(clippy::cast_precision_loss, reason = "memory in MB")]
                    win::process_usage(pid).map_or(0.0, |u| u.ram as f64 / 1_048_576.0)
                };
                let mut samples = vec![Sample::cost(format!("{name}: cold load"), "ms", load_ms)];
                let mut peak = memory();
                for (label, text) in TEXTS {
                    let spoken = speak(&infer, &mut events, text, false).await?;
                    peak = peak.max(memory());
                    samples.push(Sample::cost(
                        format!("{name}: {label}: first audio"),
                        "ms",
                        spoken.first_ms,
                    ));
                    samples.push(Sample::cost(
                        format!("{name}: {label}: real-time factor"),
                        "×",
                        spoken.total_ms / (spoken.seconds * 1000.0).max(1.0),
                    ));
                    // The Voice page's summary: first audio of a short reply, speed on long text.
                    if label == "short" {
                        samples.push(Sample::cost(
                            format!("{name}: first audio"),
                            "ms",
                            spoken.first_ms,
                        ));
                    }
                    if label == "long" {
                        samples.push(Sample::cost(
                            format!("{name}: real-time factor"),
                            "×",
                            spoken.total_ms / (spoken.seconds * 1000.0).max(1.0),
                        ));
                    }
                }
                let cancelled = speak(&infer, &mut events, TEXTS[2].1, true).await?;
                samples.push(Sample::cost(
                    format!("{name}: cancel → silence"),
                    "ms",
                    cancelled.cancel_ms.unwrap_or(0.0),
                ));
                samples.push(Sample::cost(
                    format!("{name}: worker peak memory"),
                    "MB",
                    peak,
                ));
                Ok(samples)
            }
            .await;
            stop.cancel();
            let _ = tokio::time::timeout(Duration::from_secs(5), task).await;
            result
        })
    }
}

/// What speaking one text took.
struct Spoken {
    first_ms: f64,
    total_ms: f64,
    seconds: f64,
    cancel_ms: Option<f64>,
}

/// Speaks `text` in the worker (nothing is played), timing the first audio and the whole;
/// `cancel` stops it at its first audio and times cancel → the end of its speech.
async fn speak(
    infer: &Infer,
    events: &mut mpsc::UnboundedReceiver<InferEvent>,
    text: &str,
    cancel: bool,
) -> Result<Spoken, String> {
    let id = infer.next_utterance();
    let started = Instant::now();
    infer
        .speak(id, text, None)
        .await
        .map_err(|e| e.to_string())?;
    let mut first_ms = None;
    let mut cancelled_at: Option<Instant> = None;
    let mut samples = 0usize;
    let mut rate = 24_000u32;
    tokio::time::timeout(Duration::from_secs(120), async {
        while let Some(event) = events.recv().await {
            match event {
                InferEvent::Speech {
                    id: of,
                    rate: r,
                    pcm,
                } if of == id => {
                    rate = r;
                    samples += pcm.len();
                    if first_ms.is_none() {
                        first_ms = Some(started.elapsed().as_secs_f64() * 1000.0);
                        if cancel {
                            cancelled_at = Some(Instant::now());
                            infer.cancel_speech(id).map_err(|e| e.to_string())?;
                        }
                    }
                }
                InferEvent::SpeakDone { id: of, error, .. } if of == id => {
                    return match error {
                        Some(e) if cancelled_at.is_none() => Err(e),
                        _ => Ok(()),
                    };
                }
                InferEvent::Lost | InferEvent::Crashed { .. } => {
                    return Err("the speech worker stopped".to_owned());
                }
                _ => {}
            }
        }
        Err("the speech worker stopped".to_owned())
    })
    .await
    .map_err(|_| "it took too long to speak".to_owned())??;
    #[allow(clippy::cast_precision_loss, reason = "seconds of audio")]
    let seconds = samples as f64 / f64::from(rate.max(1));
    Ok(Spoken {
        first_ms: first_ms.ok_or("it made no sound")?,
        total_ms: started.elapsed().as_secs_f64() * 1000.0,
        seconds,
        cancel_ms: cancelled_at.map(|t| t.elapsed().as_secs_f64() * 1000.0),
    })
}

impl Suite for Tts {
    fn name(&self) -> &'static str {
        "tts"
    }

    fn run(&mut self) -> Result<Vec<Sample>, String> {
        let mut samples = Vec::new();
        for &voice in &self.voices {
            samples.extend(self.measure(voice)?);
        }
        Ok(samples)
    }

    fn notes(&self) -> Vec<String> {
        let mut notes: Vec<String> = self
            .voices
            .iter()
            .map(|v| {
                format!(
                    "{}: {} in KIVO's own engine, in the `kivo-infer` worker as KIVO runs it (CPU, \
                     {THREADS} threads, the default voice); each run loads it cold in a fresh worker.",
                    v.name(),
                    v.describe()
                )
            })
            .collect();
        notes.push(
            "Speech streams sentence by sentence, so first audio arrives after the first \
             sentence; nothing is played aloud."
                .into(),
        );
        notes
    }
}
