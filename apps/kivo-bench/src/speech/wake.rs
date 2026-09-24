//! `wake` (BENCHMARKS §1, BENCH-04): "Hey Kivo" on KIVO's own keyword spotter
//! (`kivo_voice::kws`), with the keywords the runtime listens for (its four spellings, at the
//! default sensitivity). No recordings from the owner are needed: the positives are synthetic
//! ("Hey Kivo" in every Kokoro voice at three speeds, through the simulated room, clean and over
//! background speech), as wake-word models are trained; the negatives are LibriSpeech test-clean
//! and test-other read speech (10.7 h). Measured: false rejects, how soon after "Hey Kivo" ends
//! the spotter reports it (the wake → earcon budget's larger part), false accepts per hour, and
//! the spotter's CPU cost (real-time factor on one thread).
//!
//! The negatives are split across the runs (warmup included): each run hears a different slice,
//! so the whole corpus is heard once and p50/p95 show how the rate varies between slices.
//! `KIVO_BENCH_WAKE_NEGATIVE_MINUTES` limits the negative audio in total (default: all of it).

use super::data::{self, Entry};
use crate::harness::{Plan, Sample, Suite};
use kivo_voice::kws::{KeywordSpotter, KwsModel};
use sherpa_onnx::{GenerationConfig, LinearResampler, OfflineTts, OfflineTtsConfig};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const RATE: usize = 16_000;
const SPEEDS: [f32; 3] = [0.85, 1.0, 1.15];

pub struct Wake {
    spotter: KeywordSpotter,
    sensitivity: f32,
    model_dir: PathBuf,
    positives: Vec<Vec<f32>>,
    /// The negatives in one slice per run, decoded one at a time during the run (5 h of audio
    /// would not fit comfortably in memory).
    slices: Vec<(Vec<Entry>, f64)>,
    next: usize,
    negative_seconds: f64,
}

/// Echo of a room plus a quieter talker in the background, for the harder positives.
fn in_room(clean: &[f32], background: Option<&[f32]>) -> Vec<f32> {
    let taps = [(0_usize, 1.0_f32), (320, 0.35), (880, 0.15)];
    let mut out = vec![0.0; clean.len() + 880 + RATE / 2];
    for (delay, gain) in taps {
        for (i, s) in clean.iter().enumerate() {
            out[RATE / 4 + i + delay] += s * gain;
        }
    }
    if let Some(bg) = background {
        for (i, s) in out.iter_mut().enumerate() {
            *s += bg.get(i).copied().unwrap_or(0.0) * 0.3;
        }
    }
    out
}

/// KIVO's keyword model: the one the runtime downloaded, or the benchmark data's copy.
fn model_dir() -> Result<PathBuf, String> {
    if let Some(paths) = kivo_platform::Paths::user()
        && let Some(installed) = kivo_store::models::ModelStore::new(paths.models())
            .installed(kivo_store::models::KEYWORD_SPOTTER)
    {
        return Ok(installed.dir);
    }
    data::model("kws-testdir")
}

/// The keywords the runtime listens for when "Hey Kivo" is on at `sensitivity`.
fn hey_kivo(sensitivity: f32) -> Vec<kivo_voice::kws::Keyword> {
    kivo_runtime::wake::keywords_for(&kivo_store::wake::WakeWord {
        id: kivo_store::wake::HEY_KIVO.into(),
        phrase: "Hey Kivo".into(),
        phonetic: None,
        enabled: true,
        built_in: true,
        sensitivity,
        samples: Vec::new(),
        quality: kivo_store::wake::Quality::Good,
        false_alarm_test: None,
        created_at: 0,
    })
}

impl Wake {
    pub fn start(plan: Plan) -> Result<Self, String> {
        let model_dir = model_dir()?;
        let model = KwsModel::load(&model_dir).map_err(|e| e.detail())?;
        // `KIVO_BENCH_WAKE_SENSITIVITY` tries another setting of the Settings slider.
        let sensitivity = std::env::var("KIVO_BENCH_WAKE_SENSITIVITY")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(kivo_store::wake::HEY_KIVO_SENSITIVITY);
        let (spotter, refused) = KeywordSpotter::new(model, hey_kivo(sensitivity));
        if !refused.is_empty() {
            return Err(format!(
                "the keyword model can't spell {:?}",
                refused.iter().map(|k| &k.phrase).collect::<Vec<_>>()
            ));
        }

        // Positives: every Kokoro voice at three speeds, clean and over background speech.
        let kokoro = data::model("kokoro-int8-en-v0_19")?;
        let mut tts_config = OfflineTtsConfig::default();
        tts_config.model.kokoro.model = Some(data::file(&kokoro, &[".onnx"])?);
        tts_config.model.kokoro.voices = Some(data::file(&kokoro, &["voices"])?);
        tts_config.model.kokoro.tokens = Some(data::file(&kokoro, &["tokens"])?);
        tts_config.model.kokoro.data_dir =
            Some(kokoro.join("espeak-ng-data").to_string_lossy().into_owned());
        tts_config.model.num_threads = 4;
        let tts = OfflineTts::create(&tts_config).ok_or("Kokoro failed to load")?;
        let resampler = LinearResampler::create(tts.sample_rate(), 16_000).ok_or("resampler")?;
        let background = data::librispeech(1)?.remove(0).samples;
        // What the voices say. `KIVO_BENCH_WAKE_SAY` tries a spelling that steers the voices'
        // pronunciation.
        let say = std::env::var("KIVO_BENCH_WAKE_SAY").unwrap_or_else(|_| "Hey Kivo.".into());
        let mut positives = Vec::new();
        for sid in 0..tts.num_speakers() {
            for speed in SPEEDS {
                let generation = GenerationConfig {
                    sid,
                    speed,
                    ..GenerationConfig::default()
                };
                let audio = tts
                    .generate_with_config(&say, &generation, None::<fn(&[f32], f32) -> bool>)
                    .ok_or("generating a positive failed")?;
                resampler.reset();
                let clip = resampler.resample(audio.samples(), true);
                positives.push(in_room(&clip, None));
                positives.push(in_room(&clip, Some(&background)));
            }
        }

        let minutes: Option<f64> = std::env::var("KIVO_BENCH_WAKE_NEGATIVE_MINUTES")
            .ok()
            .and_then(|s| s.parse().ok());
        let mut negatives = Vec::new();
        let mut negative_seconds = 0.0;
        for entry in data::librispeech_all()? {
            let seconds = entry.seconds()?;
            if minutes.is_some_and(|m| negative_seconds + seconds > m * 60.0) {
                break;
            }
            negative_seconds += seconds;
            negatives.push((entry, seconds));
        }
        // One slice per run, of about the same length.
        let count = usize::try_from(plan.runs + plan.warmup).unwrap_or(1).max(1);
        #[allow(clippy::cast_precision_loss, reason = "a slice count")]
        let per_slice = negative_seconds / count as f64;
        let mut slices: Vec<(Vec<Entry>, f64)> = vec![(Vec::new(), 0.0)];
        for (entry, seconds) in negatives {
            let full = slices.last().is_some_and(|s| s.1 >= per_slice);
            if full && slices.len() < count {
                slices.push((vec![entry], seconds));
            } else if let Some(last) = slices.last_mut() {
                last.0.push(entry);
                last.1 += seconds;
            }
        }
        Ok(Self {
            spotter,
            sensitivity,
            model_dir,
            positives,
            slices,
            next: 0,
            negative_seconds,
        })
    }

    /// How many times "Hey Kivo" is heard in `audio`, from a fresh start, in 20 ms pieces as a
    /// microphone delivers them; and for the first, the audio heard by then (samples) and how long
    /// the spotter took over that piece.
    fn spot(&mut self, audio: &[f32]) -> Result<(usize, Option<(usize, Duration)>), String> {
        self.spotter.reset();
        let mut hits = 0;
        let mut first = None;
        let mut heard = 0;
        // A little silence after it lets the last frames through.
        let tail = vec![0.0; RATE / 2];
        for chunk in audio.chunks(RATE / 50).chain(tail.chunks(RATE / 50)) {
            let t = Instant::now();
            let found = self.spotter.accept(chunk).map_err(|e| e.detail())?.len();
            heard += chunk.len();
            if found > 0 && first.is_none() {
                first = Some((heard, t.elapsed()));
            }
            hits += found;
        }
        Ok((hits, first))
    }
}

/// Where the speech in a clean clip ends: the last sample above a tenth of its peak.
fn speech_end(clip: &[f32]) -> usize {
    let peak = clip.iter().fold(0.0_f32, |m, x| m.max(x.abs()));
    clip.iter()
        .rposition(|x| x.abs() > peak * 0.1)
        .map_or(clip.len(), |i| i + 1)
}

impl Suite for Wake {
    fn name(&self) -> &'static str {
        "wake"
    }

    fn run(&mut self) -> Result<Vec<Sample>, String> {
        // Clean and over background speech alternate (see `start`).
        let positives = std::mem::take(&mut self.positives);
        let mut missed = [0_usize; 2];
        let mut delays = Vec::new();
        for (i, clip) in positives.iter().enumerate() {
            match self.spot(clip)? {
                (0, _) => missed[i % 2] += 1,
                // Clean clips only: over a talker, the end of "Hey Kivo" can't be told apart.
                (_, Some((heard, busy))) if i % 2 == 0 => {
                    let end = speech_end(clip);
                    #[allow(clippy::cast_precision_loss, reason = "sample counts")]
                    let late = heard.saturating_sub(end) as f64 / RATE as f64 * 1000.0;
                    delays.push(late + busy.as_secs_f64() * 1000.0);
                }
                _ => {}
            }
        }
        delays.sort_by(f64::total_cmp);
        let total = positives.len();
        self.positives = positives;

        let (entries, seconds) = self.slices[self.next % self.slices.len()].clone();
        self.next += 1;
        let mut false_accepts = 0;
        let mut busy = Duration::ZERO;
        for entry in &entries {
            let audio = entry.load()?;
            let t = Instant::now();
            false_accepts += self.spot(&audio.samples)?.0;
            busy += t.elapsed();
        }
        let busy = busy.as_secs_f64();
        #[allow(clippy::cast_precision_loss, reason = "counts")]
        Ok(vec![
            Sample::cost(
                "false rejects (synthetic positives)",
                "%",
                (missed[0] + missed[1]) as f64 / total.max(1) as f64 * 100.0,
            ),
            Sample::cost(
                "false rejects, clean",
                "%",
                missed[0] as f64 / (total / 2).max(1) as f64 * 100.0,
            ),
            Sample::cost(
                "false rejects, over background speech",
                "%",
                missed[1] as f64 / (total / 2).max(1) as f64 * 100.0,
            ),
            Sample::cost(
                "end of \"Hey Kivo\" → detected (clean, median)",
                "ms",
                delays.get(delays.len() / 2).copied().unwrap_or(f64::NAN),
            ),
            Sample::cost(
                "false accepts",
                "/hour",
                false_accepts as f64 / (seconds.max(1.0) / 3600.0),
            ),
            Sample::cost(
                "spotter CPU (one thread, share of real time)",
                "%",
                busy / seconds.max(1.0) * 100.0,
            ),
        ])
    }

    fn notes(&self) -> Vec<String> {
        vec![
            format!(
                "KIVO's own keyword spotter (`kivo_voice::kws`, zipformer GigaSpeech 3.3M, from {}) \
                 with the runtime's \"Hey Kivo\" keywords (its four spellings) at sensitivity {} \
                 (KIVO's default is {}).",
                self.model_dir.display(),
                self.sensitivity,
                kivo_store::wake::HEY_KIVO_SENSITIVITY
            ),
            format!(
                "Positives: {} synthetic clips (every Kokoro EN voice × speeds 0.85/1.0/1.15, \
                 through a room echo, clean and over background speech at −10 dB). No owner \
                 recordings are needed.",
                self.positives.len()
            ),
            String::from(
                "Detection delay: the clean clips arrive in 20 ms pieces, as from a microphone; \
                 it runs from where the speech ends (its last sample above a tenth of the peak) to \
                 the end of the piece the spotter reports \"Hey Kivo\" in, plus its compute time \
                 on that piece. KIVO's cue then starts within a few ms (the e2e journeys measure \
                 press → cue at 6 ms), so this is most of VOICE §10's wake → earcon.",
            ),
            format!(
                "Negatives: {:.1} h of LibriSpeech test-clean and test-other read speech in all \
                 (test-other is the harder half), split into one slice per run (about {:.0} min \
                 each). The spec's podcasts and TV aren't freely licensed; read speech from many \
                 speakers stands in for them.",
                self.negative_seconds / 3600.0,
                self.slices.first().map_or(0.0, |s| s.1 / 60.0)
            ),
        ]
    }
}
