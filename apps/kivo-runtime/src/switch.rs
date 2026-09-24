//! Choosing a speech engine safely (VOICE §11, VOICE-45): check it fits the language and the
//! privacy setting → download it (the Voice page showed the licence first) → load it in a second,
//! separate worker → test it (a sentence spoken, or a sentence heard) → only then make it the
//! choice. Until the test passes the old engine keeps working, so a failed switch never leaves
//! KIVO deaf or mute. Progress goes out as `SystemEvent::EngineSwitch`.

use crate::core::Core;
use crate::engine::Engine;
use crate::infer::{self, Engines, Infer, InferEvent};
use crate::models::Models;
use kivo_core::event::SystemEvent;
use kivo_core::{Event, EventKind, text};
use kivo_ipc::infer::InferSlot;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// How long loading a model in the test worker may take.
const LOAD_LIMIT: Duration = Duration::from_secs(120);
/// How long one test sentence may take to speak or to hear.
const TEST_LIMIT: Duration = Duration::from_secs(60);

pub struct Switcher {
    core: Arc<Core>,
    engine: Arc<Engine>,
    models: Arc<Models>,
    /// The switch in progress per slot; a new choice for the same slot cancels it.
    running: Mutex<std::collections::HashMap<InferSlot, CancellationToken>>,
    /// The voice that says the recognizer's test sentence; the Windows voices unless set.
    test_voice: Option<Arc<dyn kivo_platform::SpeechSynth>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn slot_name(slot: InferSlot) -> &'static str {
    match slot {
        InferSlot::Stt => "stt",
        InferSlot::Tts => "tts",
    }
}

/// Checks that `id` is a speech engine for `slot` that fits the settings' language and privacy
/// mode (VOICE-48, VOICE-07); the error says why not and names engines that fit.
pub fn check(slot: InferSlot, id: &str, config: &kivo_core::KivoConfig) -> Result<(), String> {
    let wanted = match slot {
        InferSlot::Stt => kivo_voice::EngineSlot::Stt,
        InferSlot::Tts => kivo_voice::EngineSlot::Tts,
    };
    let entry = kivo_voice::registry::entry(id)
        .filter(|e| e.slot() == wanted)
        .ok_or_else(|| text::tf("voice.unknownEngine", &[("id", &id)]))?;
    if let Some(problem) = kivo_voice::registry::language_problem(id, &config.general.language) {
        return Err(if problem.alternatives.is_empty() {
            text::tf(
                "voice.incompatibleNone",
                &[("engine", &problem.engine), ("language", &problem.language)],
            )
        } else {
            text::tf(
                "voice.incompatible",
                &[
                    ("engine", &problem.engine),
                    ("language", &problem.language),
                    ("alternatives", &problem.alternatives.join(", ")),
                ],
            )
        });
    }
    if entry.engine.sends_data_off_device()
        && kivo_security::privacy::speech_egress(true, &config.privacy, &config.capabilities)
            .is_err()
    {
        return Err(text::tf(
            "voice.notAllowed",
            &[("engine", &entry.engine.name)],
        ));
    }
    Ok(())
}

impl Switcher {
    pub fn new(core: Arc<Core>, engine: Arc<Engine>, models: Arc<Models>) -> Self {
        Self {
            core,
            engine,
            models,
            running: Mutex::default(),
            test_voice: None,
        }
    }

    /// Says the recognizer's test sentence with `voice` instead of the engine's Windows voice.
    #[must_use]
    pub fn with_test_voice(mut self, voice: Arc<dyn kivo_platform::SpeechSynth>) -> Self {
        self.test_voice = Some(voice);
        self
    }

    /// Starts switching `slot` to engine `id` (and `voice`, for TTS). What can be checked at
    /// once is; the rest runs in the background and reports by events.
    pub fn start(
        self: &Arc<Self>,
        slot: InferSlot,
        id: &str,
        voice: Option<String>,
    ) -> Result<(), String> {
        check(slot, id, &self.core.config())?;
        let cancel = self.core.shutdown().child_token();
        if let Some(previous) = lock(&self.running).insert(slot, cancel.clone()) {
            previous.cancel();
        }
        let this = Arc::clone(self);
        let id = id.to_owned();
        tokio::spawn(async move {
            let outcome = tokio::select! {
                outcome = this.try_engine(slot, &id, voice.as_deref()) => outcome,
                () = cancel.cancelled() => return,
            };
            match outcome {
                Ok(sample) => {
                    this.activate(slot, &id, voice);
                    // The new voice introduces itself (it's what the test spoke).
                    if let Some((pcm, rate)) = sample {
                        this.engine.speaker.speak(&pcm, rate);
                    }
                    this.stage(slot, &id, "ready", None);
                }
                Err(message) => this.stage(slot, &id, "failed", Some(message)),
            }
        });
        Ok(())
    }

    fn stage(&self, slot: InferSlot, id: &str, stage: &str, message: Option<String>) {
        if let Some(m) = &message {
            tracing::warn!(engine = id, message = m, "a speech engine didn't pass");
        } else {
            tracing::info!(engine = id, stage, "choosing a speech engine");
        }
        self.core
            .bus
            .publish(Event::new(EventKind::System(SystemEvent::EngineSwitch {
                slot: slot_name(slot).to_owned(),
                engine: id.to_owned(),
                stage: stage.to_owned(),
                message,
            })));
    }

    /// Downloads, loads and tests `id`. For a voice, returns what it said.
    async fn try_engine(
        &self,
        slot: InferSlot,
        id: &str,
        voice: Option<&str>,
    ) -> Result<Option<(Vec<f32>, u32)>, String> {
        self.stage(slot, id, "checking", None);
        let model = kivo_voice::engine(id).and_then(|e| e.model);
        let dir = match &model {
            Some(model) => Some(self.download(slot, id, model).await?),
            None => None,
        };
        self.stage(slot, id, "loading", None);
        let config = self.core.config();
        let mut probe = self
            .probe(slot, id, dir, &config.general.language)
            .await
            .map_err(|e| text::tf("voice.loadFailed", &[("error", &e)]))?;
        self.stage(slot, id, "testing", None);
        let tested = match slot {
            InferSlot::Tts => probe
                .speak(&text::t("voice.preview"), voice)
                .await
                .map(Some),
            InferSlot::Stt => self
                .hear(&mut probe, &config.general.language)
                .await
                .map(|()| None),
        };
        probe.close().await;
        tested.map_err(|e| text::tf("voice.testFailed", &[("error", &e)]))
    }

    /// Installs `model` (and what it needs) unless it's here, and waits for it.
    async fn download(&self, slot: InferSlot, id: &str, model: &str) -> Result<PathBuf, String> {
        if let Some(dir) = self.models.installed_dir(model) {
            return Ok(dir);
        }
        self.stage(slot, id, "downloading", None);
        let mut events = self.core.bus.subscribe();
        self.models.install(model)?;
        loop {
            if let Some(dir) = self.models.installed_dir(model) {
                return Ok(dir);
            }
            match events.recv().await {
                kivo_core::bus::Received::Event(event) => {
                    if let EventKind::System(SystemEvent::ModelChanged {
                        id: changed,
                        percent: None,
                        installed,
                    }) = &event.kind
                        && changed == model
                    {
                        return self
                            .models
                            .installed_dir(model)
                            .filter(|_| *installed)
                            .ok_or_else(|| text::t("voice.downloadFailed"));
                    }
                }
                kivo_core::bus::Received::Missed(_) => {}
                kivo_core::bus::Received::Closed => return Err(text::t("voice.downloadFailed")),
            }
        }
    }

    /// A Windows voice says a known sentence and the engine under test transcribes it: what was
    /// said and what was heard. Without a Windows voice, silence is transcribed (loading and
    /// running is then the whole test).
    async fn sample(&self, probe: &mut Probe, language: &str) -> Result<(String, String), String> {
        let said = text::t("voice.testPhrase");
        let Some(synth) = self
            .test_voice
            .clone()
            .or_else(|| self.engine.system_voice())
        else {
            let heard = probe.transcribe(&vec![0.0; 16_000], language).await?;
            return Ok((String::new(), heard));
        };
        let sentence = said.clone();
        let spoken = tokio::task::spawn_blocking(move || synth.synthesize(&sentence, None))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let mut audio =
            kivo_audio::RateConverter::convert_all(spoken.rate, 16_000, &spoken.samples);
        audio.extend(std::iter::repeat_n(0.0, 8_000));
        let heard = probe.transcribe(&audio, language).await?;
        Ok((said, heard))
    }

    /// The recognizer test: the new engine must hear most of the sentence. Engines for other
    /// languages only need to run (the Windows voice speaks English).
    async fn hear(&self, probe: &mut Probe, language: &str) -> Result<(), String> {
        let (said, heard) = self.sample(probe, language).await?;
        let english = language.split('-').next() == Some("en");
        if english && !said.is_empty() && !heard_enough(&said, &heard) {
            return Err(text::tf(
                "voice.misheard",
                &[("heard", &heard.trim()), ("said", &said)],
            ));
        }
        Ok(())
    }

    /// "Try sample" on a recognizer's card (UX-60): it transcribes the test sentence in a
    /// separate worker; the engine KIVO listens with is untouched.
    pub async fn try_sample(&self, id: &str) -> Result<(String, String), String> {
        let config = self.core.config();
        check(InferSlot::Stt, id, &config)?;
        let model = kivo_voice::engine(id)
            .and_then(|e| e.model)
            .unwrap_or_else(|| id.to_owned());
        let dir = self
            .models
            .installed_dir(&model)
            .ok_or_else(|| text::t("voice.notDownloaded"))?;
        let mut probe = self
            .probe(InferSlot::Stt, id, Some(dir), &config.general.language)
            .await?;
        let result = self.sample(&mut probe, &config.general.language).await;
        probe.close().await;
        result
    }

    /// Scores every installed recognizer for the language on the owner's enrollment recordings
    /// (VOICE-23), each in a separate worker so the engine KIVO listens with is untouched.
    /// Returns word error rates, 0–1, per engine.
    pub async fn voice_wer(
        &self,
        clips: &[(String, Vec<f32>)],
    ) -> std::collections::BTreeMap<String, f64> {
        let config = self.core.config();
        let language = config.general.language.clone();
        let mut out = std::collections::BTreeMap::new();
        for entry in kivo_voice::registry::compatible(kivo_voice::EngineSlot::Stt, &language) {
            let id = entry.engine.id.clone();
            let Some(dir) = entry
                .engine
                .model
                .as_deref()
                .and_then(|m| self.models.installed_dir(m))
            else {
                continue;
            };
            let Ok(mut probe) = self.probe(InferSlot::Stt, &id, Some(dir), &language).await else {
                continue;
            };
            let mut tally = kivo_voice::wer::Tally::default();
            let mut heard_all = true;
            for (said, audio) in clips {
                let mut padded = audio.clone();
                padded.extend(std::iter::repeat_n(0.0, 8_000));
                match probe.transcribe(&padded, &language).await {
                    Ok(heard) => tally.add(said, &heard),
                    Err(_) => {
                        heard_all = false;
                        break;
                    }
                }
            }
            probe.close().await;
            if heard_all && tally.words > 0 {
                out.insert(id, tally.rate());
            }
        }
        out
    }

    /// The engine passed: it becomes the choice, and the running worker switches to it.
    fn activate(&self, slot: InferSlot, id: &str, voice: Option<String>) {
        let saved = self.core.update_config(|c| match slot {
            InferSlot::Stt => c.voice.stt_engine = id.to_owned(),
            InferSlot::Tts => {
                if c.voice.tts_engine != id || voice.is_some() {
                    c.voice.tts_voice = voice.unwrap_or_default();
                }
                c.voice.tts_engine = id.to_owned();
            }
        });
        self.engine.infer.forgive(id);
        self.models.apply_engines(&saved);
        self.engine.settings_changed(&saved);
    }

    /// Speaks the preview sentence with an engine and voice that may not be the chosen ones,
    /// in a separate worker, and plays it (the voice cards' Preview, VOICE §11).
    pub async fn preview(&self, id: &str, voice: Option<&str>) -> Result<(), String> {
        check(InferSlot::Tts, id, &self.core.config())?;
        let model = kivo_voice::engine(id).and_then(|e| e.model);
        let dir = match model {
            Some(model) => Some(
                self.models
                    .installed_dir(&model)
                    .ok_or_else(|| text::t("voice.notDownloaded"))?,
            ),
            None => None,
        };
        let language = self.core.config().general.language;
        let mut probe = self.probe(InferSlot::Tts, id, dir, &language).await?;
        let spoken = probe.speak(&text::t("voice.preview"), voice).await;
        probe.close().await;
        let (pcm, rate) = spoken?;
        self.engine.speaker.speak(&pcm, rate);
        Ok(())
    }

    /// A test worker holding engine `id`, where KIVO would run it: on the graphics card when the
    /// GPU policy puts it there (DECISIONS "Local models on the GPU first"), with the speech
    /// models' thread count.
    async fn probe(
        &self,
        slot: InferSlot,
        id: &str,
        dir: Option<PathBuf>,
        language: &str,
    ) -> Result<Probe, String> {
        let config = self.core.config();
        Probe::start(
            self.engine.infer.program().to_path_buf(),
            slot,
            id,
            dir,
            language,
            self.models.cloud_access(id, &config),
            Place {
                gpu: self.models.gpu_for(id, &config),
                threads: self.models.speech_threads(),
            },
        )
        .await
    }

    /// "Benchmark this engine" (BENCH-15, VOICE §11): measures an installed engine on this PC in
    /// a separate worker, so the engines KIVO uses are untouched. A recognizer transcribes
    /// commands a Windows voice speaks, clean and over noise: end of speech → final text, real-
    /// time factor, word error rates. A voice speaks a short and a long text: first audio,
    /// real-time factor, cancel → silence. Both report the worker's CPU share and memory.
    pub async fn benchmark(&self, id: &str) -> Result<kivo_voice::registry::Measured, String> {
        let config = self.core.config();
        let entry = kivo_voice::engine(id).ok_or_else(|| text::t("voice.notAnEngine"))?;
        let slot = match entry.slot {
            kivo_voice::EngineSlot::Stt => InferSlot::Stt,
            kivo_voice::EngineSlot::Tts => InferSlot::Tts,
            _ => return Err(text::t("voice.notAnEngine")),
        };
        check(slot, id, &config)?;
        let dir = match entry.model {
            Some(model) => Some(
                self.models
                    .installed_dir(&model)
                    .ok_or_else(|| text::t("voice.notDownloaded"))?,
            ),
            None => None,
        };
        let language = config.general.language.clone();
        // The spoken commands come from a Windows voice, before the test worker takes the CPU.
        let mut commands = Vec::new();
        if slot == InferSlot::Stt {
            let synth = self
                .test_voice
                .clone()
                .or_else(|| self.engine.system_voice())
                .ok_or_else(|| text::t("voice.benchmarkNeedsVoice"))?;
            for said in BENCH_COMMANDS {
                let synth = Arc::clone(&synth);
                let audio = tokio::task::spawn_blocking(move || synth.synthesize(said, None))
                    .await
                    .map_err(|e| e.to_string())?
                    .map_err(|e| e.to_string())?;
                commands.push((
                    *said,
                    kivo_audio::RateConverter::convert_all(audio.rate, 16_000, &audio.samples),
                ));
            }
        }
        let mut probe = self.probe(slot, id, dir, &language).await?;
        let system = self.models.system_info();
        let pid = probe.infer.worker_pid();
        let usage = || pid.and_then(|p| system.as_ref().and_then(|s| s.process_usage(p)));
        let before = usage();
        let started = std::time::Instant::now();
        let measured = match slot {
            InferSlot::Stt => bench_stt(&mut probe, &commands, &language).await,
            // The chosen voice, when this is the engine KIVO speaks with; else its first voice.
            InferSlot::Tts if config.voice.tts_engine == id => {
                bench_tts(&mut probe, &config.voice.tts_voice).await
            }
            InferSlot::Tts => bench_tts(&mut probe, "").await,
        };
        let wall_ms = started.elapsed().as_secs_f64() * 1e3;
        let after = usage();
        let vram = pid.and_then(|p| system.as_ref().and_then(|s| s.gpu_memory(&[p])));
        probe.close().await;
        let mut measured = measured?;
        let cpus = system
            .as_ref()
            .and_then(|s| s.snapshot().ok())
            .map_or(1, |s| s.logical_cpus.max(1));
        if let (Some(before), Some(after)) = (before, after) {
            #[allow(clippy::cast_precision_loss, reason = "CPU milliseconds")]
            let busy = after.cpu_ms.saturating_sub(before.cpu_ms) as f64;
            measured.cpu_percent = Some(busy * 100.0 / (wall_ms.max(1.0) * f64::from(cpus)));
            #[allow(clippy::cast_precision_loss, reason = "megabytes")]
            let mb = after.memory_bytes as f64 / 1_048_576.0;
            measured.memory_mb = Some(mb);
        }
        // The worker's GPU memory counts with its memory when it uses the GPU.
        if let Some(bytes) = vram.filter(|b| *b > 0) {
            #[allow(clippy::cast_precision_loss, reason = "megabytes")]
            let mb = bytes as f64 / 1_048_576.0;
            measured.memory_mb = Some(measured.memory_mb.unwrap_or(0.0) + mb);
        }
        measured.measured_at = kivo_store::brains::now_ms();
        Ok(measured)
    }
}

/// Commands a recognizer is benchmarked on (BENCH-15): KIVO's own kind of speech.
const BENCH_COMMANDS: &[&str] = &[
    "Open the calculator",
    "Set the volume to thirty percent",
    "Remind me to call Maya at five",
    "What's the weather like tomorrow in Lisbon",
    "Take a screenshot of this window",
];
/// A voice's short and long texts.
const BENCH_SHORT: &str = "Done. The volume is at thirty percent.";
const BENCH_LONG: &str = "Here's your afternoon. At two you have the design review with Maya, \
    which usually runs long, so I moved your focus block to four. The build finished with two \
    warnings, both in the settings page, and the report you asked for is on your desktop. It will \
    rain after six, so if you're cycling home, leave a little earlier.";

fn median(mut values: Vec<f64>) -> Option<f64> {
    values.sort_by(f64::total_cmp);
    values.get(values.len() / 2).copied()
}

/// Deterministic background noise (a room's hiss and mains hum) about 10 dB under the speech.
fn with_noise(audio: &[f32]) -> Vec<f32> {
    #[allow(clippy::cast_precision_loss, reason = "a loudness level")]
    let rms = (audio.iter().map(|s| s * s).sum::<f32>() / audio.len().max(1) as f32).sqrt();
    let level = rms * 0.3;
    let mut seed: u32 = 0x2545_f491;
    audio
        .iter()
        .enumerate()
        .map(|(i, s)| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            #[allow(clippy::cast_precision_loss, reason = "noise")]
            let hiss = (seed as f32 / u32::MAX as f32) * 2.0 - 1.0;
            #[allow(clippy::cast_precision_loss, reason = "a sample index")]
            let hum = (i as f32 * std::f32::consts::TAU * 120.0 / 16_000.0).sin();
            s + level * 0.8f32.mul_add(hiss, 0.6 * hum)
        })
        .collect()
}

async fn bench_stt(
    probe: &mut Probe,
    commands: &[(&str, Vec<f32>)],
    language: &str,
) -> Result<kivo_voice::registry::Measured, String> {
    let mut clean = kivo_voice::wer::Tally::default();
    let mut noisy = kivo_voice::wer::Tally::default();
    let mut finals = Vec::new();
    let mut busy_ms = 0.0;
    let mut audio_ms = 0.0;
    for (said, audio) in commands {
        let mut padded = audio.clone();
        padded.extend(std::iter::repeat_n(0.0, 8_000));
        let (heard, final_ms, total_ms) = probe.transcribe_timed(&padded, language).await?;
        clean.add(said, &heard);
        finals.push(final_ms);
        busy_ms += total_ms;
        #[allow(clippy::cast_precision_loss, reason = "milliseconds of audio")]
        let ms = padded.len() as f64 / 16.0;
        audio_ms += ms;
        let (heard, _, _) = probe
            .transcribe_timed(&with_noise(&padded), language)
            .await?;
        noisy.add(said, &heard);
    }
    Ok(kivo_voice::registry::Measured {
        latency_ms: median(finals),
        real_time_factor: Some(busy_ms / f64::max(audio_ms, 1.0)),
        word_error_rate: Some(clean.rate()),
        noisy_word_error_rate: Some(noisy.rate()),
        ..Default::default()
    })
}

async fn bench_tts(
    probe: &mut Probe,
    voice: &str,
) -> Result<kivo_voice::registry::Measured, String> {
    let voice = (!voice.is_empty()).then_some(voice);
    let mut firsts = Vec::new();
    for _ in 0..3 {
        firsts.push(probe.speak_timed(BENCH_SHORT, voice, false).await?.first_ms);
    }
    let long = probe.speak_timed(BENCH_LONG, voice, false).await?;
    let cancelled = probe.speak_timed(BENCH_LONG, voice, true).await?;
    Ok(kivo_voice::registry::Measured {
        latency_ms: median(firsts),
        real_time_factor: Some(long.total_ms / f64::max(long.seconds * 1e3, 1.0)),
        cancel_ms: cancelled.cancel_ms,
        ..Default::default()
    })
}

/// What speaking one text took.
struct Spoken {
    first_ms: f64,
    total_ms: f64,
    seconds: f64,
    cancel_ms: Option<f64>,
}

/// True when most of the sentence's words were heard ("Open the calculator" → "open the
/// calculator." passes; "Oven the cal" doesn't).
fn heard_enough(said: &str, heard: &str) -> bool {
    let words = |s: &str| -> Vec<String> {
        s.split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(str::to_lowercase)
            .collect()
    };
    let said = words(said);
    let heard = words(heard);
    let found = said.iter().filter(|w| heard.contains(w)).count();
    !said.is_empty() && found * 3 >= said.len() * 2
}

/// Where a test worker runs its engine.
struct Place {
    gpu: Option<u32>,
    threads: usize,
}

/// A second speech worker holding one engine to try it out; the real one is untouched.
struct Probe {
    infer: Infer,
    events: mpsc::UnboundedReceiver<InferEvent>,
    stop: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

impl Probe {
    async fn start(
        program: PathBuf,
        slot: InferSlot,
        id: &str,
        dir: Option<PathBuf>,
        language: &str,
        cloud: Option<kivo_ipc::infer::CloudLoad>,
        place: Place,
    ) -> Result<Self, String> {
        let (infer, events, sender) = Infer::new(program);
        let stop = CancellationToken::new();
        let task = tokio::spawn(infer::supervise(infer.clone(), sender, stop.clone()));
        let mut probe = Self {
            infer,
            events,
            stop,
            task,
        };
        let engine = id.to_owned();
        probe.infer.set_engines(Engines {
            stt: match (slot, &dir) {
                (InferSlot::Stt, Some(dir)) => Some((engine.clone(), dir.clone())),
                // A cloud recognizer has no folder, only its key.
                (InferSlot::Stt, None) if cloud.is_some() => Some((engine.clone(), PathBuf::new())),
                _ => None,
            },
            stt_fallback: None,
            tts: (slot == InferSlot::Tts).then_some((engine, dir)),
            threads: place.threads,
            language: language.to_owned(),
            cloud: cloud
                .map(|c| std::collections::BTreeMap::from([(id.to_owned(), c)]))
                .unwrap_or_default(),
            gpu: place.gpu,
        });
        let ready = probe.infer.wait_ready(LOAD_LIMIT).await;
        // A failed load shows up as a fallback (the worker stays up) or a lost worker.
        while let Ok(event) = probe.events.try_recv() {
            if let Some(error) = failure(&event) {
                probe.close().await;
                return Err(error);
            }
        }
        if !ready {
            probe.close().await;
            return Err("it took too long to load".into());
        }
        Ok(probe)
    }

    async fn speak(
        &mut self,
        sentence: &str,
        voice: Option<&str>,
    ) -> Result<(Vec<f32>, u32), String> {
        let id = self.infer.next_utterance();
        self.infer
            .speak(id, sentence, voice)
            .await
            .map_err(|e| e.to_string())?;
        let mut pcm = Vec::new();
        let mut rate = 16_000;
        tokio::time::timeout(TEST_LIMIT, async {
            while let Some(event) = self.events.recv().await {
                match event {
                    InferEvent::Speech {
                        id: of,
                        rate: r,
                        pcm: chunk,
                    } if of == id => {
                        rate = r;
                        pcm.extend(chunk);
                    }
                    InferEvent::SpeakDone { id: of, error, .. } if of == id => {
                        return error.map_or(Ok(()), Err);
                    }
                    other => {
                        if let Some(error) = failure(&other) {
                            return Err(error);
                        }
                    }
                }
            }
            Err("the test worker stopped".to_owned())
        })
        .await
        .map_err(|_| "it took too long to speak".to_owned())??;
        #[allow(clippy::cast_precision_loss, reason = "a loudness check")]
        let rms = (pcm.iter().map(|s| s * s).sum::<f32>() / pcm.len().max(1) as f32).sqrt();
        #[allow(clippy::cast_precision_loss, reason = "seconds of audio")]
        let seconds = pcm.len() as f32 / rate.max(1) as f32;
        if rms < 0.003 || seconds < 0.4 {
            return Err(text::t("voice.silent"));
        }
        Ok((pcm, rate))
    }

    async fn transcribe(&mut self, audio: &[f32], language: &str) -> Result<String, String> {
        let id = self.infer.next_utterance();
        let infer = &self.infer;
        tokio::time::timeout(TEST_LIMIT, async {
            infer
                .start_stt(id, language, Vec::new())
                .await
                .map_err(|e| e.to_string())?;
            for chunk in audio.chunks(1_280) {
                infer.send_audio(id, chunk).map_err(|e| e.to_string())?;
            }
            infer
                .finish_stt(id)
                .await
                .map(|f| f.text)
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|_| "it took too long to listen".to_owned())?
    }

    /// Transcribes like `transcribe`, timing it: the text, end of audio → final (ms), and the
    /// whole transcription (ms).
    async fn transcribe_timed(
        &mut self,
        audio: &[f32],
        language: &str,
    ) -> Result<(String, f64, f64), String> {
        let id = self.infer.next_utterance();
        let infer = &self.infer;
        tokio::time::timeout(TEST_LIMIT, async {
            let started = std::time::Instant::now();
            infer
                .start_stt(id, language, Vec::new())
                .await
                .map_err(|e| e.to_string())?;
            for chunk in audio.chunks(1_280) {
                infer.send_audio(id, chunk).map_err(|e| e.to_string())?;
            }
            let ended = std::time::Instant::now();
            let text = infer
                .finish_stt(id)
                .await
                .map(|f| f.text)
                .map_err(|e| e.to_string())?;
            Ok((
                text,
                ended.elapsed().as_secs_f64() * 1e3,
                started.elapsed().as_secs_f64() * 1e3,
            ))
        })
        .await
        .map_err(|_| "it took too long to listen".to_owned())?
    }

    /// Speaks a text without playing it, timing the first audio and the whole; `cancel` stops
    /// it at its first audio and times cancel → the end of its speech.
    async fn speak_timed(
        &mut self,
        sentence: &str,
        voice: Option<&str>,
        cancel: bool,
    ) -> Result<Spoken, String> {
        let id = self.infer.next_utterance();
        let started = std::time::Instant::now();
        self.infer
            .speak(id, sentence, voice)
            .await
            .map_err(|e| e.to_string())?;
        let mut first_ms = None;
        let mut cancelled_at: Option<std::time::Instant> = None;
        let mut samples = 0usize;
        let mut rate = 16_000u32;
        let infer = self.infer.clone();
        let events = &mut self.events;
        tokio::time::timeout(TEST_LIMIT, async {
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
                            first_ms = Some(started.elapsed().as_secs_f64() * 1e3);
                            if cancel {
                                cancelled_at = Some(std::time::Instant::now());
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
                    other => {
                        if let Some(error) = failure(&other) {
                            return Err(error);
                        }
                    }
                }
            }
            Err("the test worker stopped".to_owned())
        })
        .await
        .map_err(|_| "it took too long to speak".to_owned())??;
        #[allow(clippy::cast_precision_loss, reason = "seconds of audio")]
        let seconds = samples as f64 / f64::from(rate.max(1));
        Ok(Spoken {
            first_ms: first_ms.ok_or_else(|| text::t("voice.silent"))?,
            total_ms: started.elapsed().as_secs_f64() * 1e3,
            seconds,
            cancel_ms: cancelled_at.map(|t| t.elapsed().as_secs_f64() * 1e3),
        })
    }

    async fn close(self) {
        self.stop.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(5), self.task).await;
    }
}

/// The error in a worker event that means the engine under test failed.
fn failure(event: &InferEvent) -> Option<String> {
    match event {
        InferEvent::Fallback { error, .. } => Some(if error.is_empty() {
            "it couldn't start".to_owned()
        } else {
            error.clone()
        }),
        InferEvent::Lost | InferEvent::Crashed { .. } => Some("the test worker stopped".to_owned()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_heard_sentence_needs_most_of_its_words() {
        assert!(heard_enough("Open the calculator", "open the calculator."));
        assert!(heard_enough("Open the calculator", "Open a calculator"));
        assert!(!heard_enough("Open the calculator", "Oven cal"));
        assert!(!heard_enough("Open the calculator", ""));
    }

    #[test]
    fn engines_are_checked_against_the_language_and_the_slot() {
        let mut config = kivo_core::KivoConfig::default();
        assert!(check(InferSlot::Stt, "moonshine-base-en", &config).is_ok());
        assert!(check(InferSlot::Tts, "kokoro-82m", &config).is_ok());
        assert!(
            check(InferSlot::Tts, "moonshine-base-en", &config).is_err(),
            "a recognizer isn't a voice"
        );
        assert!(check(InferSlot::Stt, "nonsense", &config).is_err());
        config.general.language = "es-ES".into();
        let refused = check(InferSlot::Stt, "moonshine-base-en", &config).unwrap_err();
        assert!(refused.contains("Moonshine Base (Spanish)"), "{refused}");
        assert!(check(InferSlot::Stt, "moonshine-base-es", &config).is_ok());
        assert!(check(InferSlot::Tts, "supertonic-3", &config).is_ok());
    }
}
