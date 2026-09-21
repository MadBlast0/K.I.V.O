//! `vad` (BENCHMARKS §1, BENCH-05): Silero VAD v6 and echo cancellation (WebRTC AEC3, `sonora`)
//! on a simulated room. Playing test audio through the speakers would make noise on the owner's
//! PC, so the loop is digital: KIVO's own speech (Kokoro) goes through a room echo (delay,
//! reflections, attenuation, noise) into the "microphone". Measured:
//!
//! - VAD onset and end latency on real speech (LibriSpeech);
//! - false VAD triggers and words leaking into speech-to-text while only KIVO's echo is heard,
//!   without and with AEC3, and how much echo AEC3 removes (ERLE);
//! - onset latency of the user talking over KIVO (barge-in, VOICE §7), with AEC3.

use super::data::{self, Utterance};
use super::stt::Engine;
use crate::harness::{Sample, Suite};
use sherpa_onnx::{
    GenerationConfig, LinearResampler, OfflineRecognizer, OfflineTts, OfflineTtsConfig,
    VadModelConfig, VoiceActivityDetector,
};
use sonora::config::EchoCanceller;
use sonora::{AudioProcessing, Config, StreamConfig};

const RATE: usize = 16_000;
/// 10 ms frames, as the pipeline uses (VOICE §1).
const FRAME: usize = RATE / 100;
/// What KIVO says while the echo is measured (about 20 s of speech).
const KIVO_SAYS: &str = "Here's what I found. The error comes from line 42 in src/main.rs: \
    Config::load returns a Result, but the variable is typed as Config. Adding a question mark \
    fixes it. I also checked the crate's changelog, and version 2.1 renamed the method. Your \
    next meeting starts at four thirty, and the build finished with three warnings. Want me to \
    make the change and run the tests again?";

pub struct VadAec {
    vad: VadModelConfig,
    speech: Vec<Utterance>,
    /// KIVO's voice at 16 kHz (the render signal).
    kivo: Vec<f32>,
    /// What the microphone picks up of it.
    echo: Vec<f32>,
    stt: OfflineRecognizer,
}

/// A small room: the direct path 40 ms late at −6 dB, two reflections, and a little noise.
fn room(signal: &[f32]) -> Vec<f32> {
    let taps = [(640_usize, 0.5_f32), (960, 0.25), (1760, 0.12)];
    let mut out = vec![0.0; signal.len() + 1760];
    for (delay, gain) in taps {
        for (i, s) in signal.iter().enumerate() {
            out[i + delay] += s * gain;
        }
    }
    // Deterministic noise at about −60 dBFS.
    let mut seed = 0x2545_f491_u32;
    for s in &mut out {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        #[allow(clippy::cast_precision_loss, reason = "noise")]
        let noise = (seed as f32 / u32::MAX as f32 - 0.5) * 0.002;
        *s += noise;
    }
    out
}

fn vad_config() -> Result<VadModelConfig, String> {
    let mut config = VadModelConfig::default();
    let model = data::root()?.join("models").join("silero_vad.onnx");
    if !model.is_file() {
        return Err(format!(
            "Silero VAD is missing: put silero_vad.onnx (v6) at {}",
            model.display()
        ));
    }
    config.silero_vad.model = Some(model.to_string_lossy().into_owned());
    config.silero_vad.threshold = 0.5;
    // VOICE §3: a 250 ms pause ends speech.
    config.silero_vad.min_silence_duration = 0.25;
    config.silero_vad.min_speech_duration = 0.25;
    config.silero_vad.window_size = 512;
    config.silero_vad.max_speech_duration = 20.0;
    config.sample_rate = 16_000;
    config.num_threads = 1;
    config.provider = Some("cpu".into());
    Ok(config)
}

/// The first 10 ms frame whose RMS rises above (or, from the end, stays above) 0.02.
fn energy_bounds(samples: &[f32]) -> Option<(usize, usize)> {
    let loud = |f: &[f32]| (f.iter().map(|s| s * s).sum::<f32>() / f.len() as f32).sqrt() > 0.02;
    let frames: Vec<bool> = samples.chunks(FRAME).map(loud).collect();
    let first = frames.iter().position(|&l| l)?;
    let last = frames.iter().rposition(|&l| l)?;
    Some((first * FRAME, (last + 1) * FRAME))
}

/// What the VAD made of a signal fed in 10 ms frames. Positions are in samples.
struct Detection {
    /// When speech was first detected.
    onset: Option<usize>,
    /// When the VAD first decided that speech was over.
    end: Option<usize>,
    segments: Vec<Vec<f32>>,
}

fn detect(config: &VadModelConfig, mic: &[f32]) -> Result<Detection, String> {
    let vad = VoiceActivityDetector::create(config, 30.0).ok_or("the VAD failed to load")?;
    let (mut onset, mut end) = (None, None);
    let mut segments = Vec::new();
    for (i, frame) in mic.chunks(FRAME).enumerate() {
        vad.accept_waveform(frame);
        let detected = vad.detected();
        if onset.is_none() && detected {
            onset = Some((i + 1) * FRAME);
        } else if onset.is_some() && end.is_none() && !detected {
            end = Some((i + 1) * FRAME);
        }
        while let Some(segment) = vad.front() {
            segments.push(segment.samples().to_vec());
            vad.pop();
        }
    }
    vad.flush();
    while let Some(segment) = vad.front() {
        segments.push(segment.samples().to_vec());
        vad.pop();
    }
    Ok(Detection {
        onset,
        end,
        segments,
    })
}

/// Runs AEC3 over `mic`, with `render` (what KIVO played) as the reference.
fn cancel_echo(render: &[f32], mic: &[f32]) -> Result<Vec<f32>, String> {
    let config = StreamConfig::new(16_000, 1);
    let mut apm = AudioProcessing::builder()
        .config(Config {
            echo_canceller: Some(EchoCanceller::default()),
            ..Default::default()
        })
        .capture_config(config)
        .render_config(config)
        .build();
    let mut out = Vec::with_capacity(mic.len());
    let (mut r_out, mut c_out) = (vec![0.0; FRAME], vec![0.0; FRAME]);
    for (i, frame) in mic.chunks_exact(FRAME).enumerate() {
        let start = i * FRAME;
        let reference: Vec<f32> = (start..start + FRAME)
            .map(|j| render.get(j).copied().unwrap_or(0.0))
            .collect();
        apm.process_render_f32(&[&reference], &mut [&mut r_out])
            .map_err(|e| format!("{e:?}"))?;
        apm.process_capture_f32(&[frame], &mut [&mut c_out])
            .map_err(|e| format!("{e:?}"))?;
        out.extend_from_slice(&c_out);
    }
    Ok(out)
}

fn energy(samples: &[f32]) -> f64 {
    samples.iter().map(|&s| f64::from(s) * f64::from(s)).sum()
}

impl VadAec {
    pub fn start() -> Result<Self, String> {
        let vad = vad_config()?;
        let speech = data::librispeech(12)?;
        let kokoro = data::model("kokoro-int8-en-v0_19")?;
        let mut tts_config = OfflineTtsConfig::default();
        tts_config.model.kokoro.model = Some(data::file(&kokoro, &[".onnx"])?);
        tts_config.model.kokoro.voices = Some(data::file(&kokoro, &["voices"])?);
        tts_config.model.kokoro.tokens = Some(data::file(&kokoro, &["tokens"])?);
        tts_config.model.kokoro.data_dir =
            Some(kokoro.join("espeak-ng-data").to_string_lossy().into_owned());
        tts_config.model.num_threads = 4;
        let tts = OfflineTts::create(&tts_config).ok_or("Kokoro failed to load")?;
        let audio = tts
            .generate_with_config(
                KIVO_SAYS,
                &GenerationConfig::default(),
                None::<fn(&[f32], f32) -> bool>,
            )
            .ok_or("generating KIVO's speech failed")?;
        let resampler =
            LinearResampler::create(tts.sample_rate(), 16_000).ok_or("resampler failed")?;
        let kivo = resampler.resample(audio.samples(), true);
        let echo = room(&kivo);
        let stt = OfflineRecognizer::create(&Engine::Parakeet.config()?)
            .ok_or("Parakeet failed to load")?;
        Ok(Self {
            vad,
            speech,
            kivo,
            echo,
            stt,
        })
    }

    fn words(&self, segments: &[Vec<f32>]) -> usize {
        segments
            .iter()
            .map(|segment| {
                let stream = self.stt.create_stream();
                stream.accept_waveform(16_000, segment);
                self.stt.decode(&stream);
                stream
                    .get_result()
                    .map_or(0, |r| super::wer::normalize(&r.text).len())
            })
            .sum()
    }
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v.get(v.len() / 2).copied().unwrap_or(0.0)
}

impl Suite for VadAec {
    fn name(&self) -> &'static str {
        "vad"
    }

    fn run(&mut self) -> Result<Vec<Sample>, String> {
        let ms = |samples: f64| samples / RATE as f64 * 1000.0;
        // 1. Onset and end latency on clean speech, padded with a second of silence each side.
        let (mut onsets, mut ends) = (Vec::new(), Vec::new());
        for u in &self.speech {
            let mut mic = vec![0.0; RATE];
            mic.extend_from_slice(&u.samples);
            mic.extend(std::iter::repeat_n(0.0, RATE));
            let (start, end) = energy_bounds(&mic).ok_or("silent utterance")?;
            let found = detect(&self.vad, &mic)?;
            #[allow(clippy::cast_precision_loss, reason = "sample positions")]
            if let (Some(onset), Some(ended)) = (found.onset, found.end) {
                onsets.push(ms(onset.saturating_sub(start) as f64));
                ends.push(ms(ended.saturating_sub(end) as f64));
            }
        }

        // 2. Only KIVO's echo is heard: without and with AEC3.
        #[allow(clippy::cast_precision_loss, reason = "sample counts")]
        let minutes = self.kivo.len() as f64 / RATE as f64 / 60.0;
        let raw_segments = detect(&self.vad, &self.echo)?.segments;
        let cleaned = cancel_echo(&self.kivo, &self.echo)?;
        let clean_segments = detect(&self.vad, &cleaned)?.segments;
        // Skip AEC3's first two seconds, while it converges, when measuring what it removed.
        let skip = 2 * RATE;
        let erle = 10.0
            * (energy(&self.echo[skip..cleaned.len()]) / energy(&cleaned[skip..]).max(1e-12))
                .log10();

        // 3. The user talks over KIVO (barge-in), with AEC3: time to detect them.
        let mut barge = Vec::new();
        for u in self.speech.iter().take(6) {
            let at = 5 * RATE; // the user starts talking 5 s into KIVO's answer
            let mut mic = self.echo.clone();
            mic.resize(mic.len().max(at + u.samples.len()), 0.0);
            for (i, s) in u.samples.iter().enumerate() {
                mic[at + i] += s;
            }
            let cleaned = cancel_echo(&self.kivo, &mic)?;
            let (start, _) = energy_bounds(&u.samples).ok_or("silent utterance")?;
            // Onset after the user started: find the first detection at or after their start.
            let vad = VoiceActivityDetector::create(&self.vad, 30.0).ok_or("VAD")?;
            for (i, frame) in cleaned.chunks(FRAME).enumerate() {
                vad.accept_waveform(frame);
                let pos = (i + 1) * FRAME;
                if pos > at + start && vad.detected() {
                    #[allow(clippy::cast_precision_loss, reason = "sample positions")]
                    barge.push(ms((pos - at - start) as f64));
                    break;
                }
            }
        }

        #[allow(clippy::cast_precision_loss, reason = "counts")]
        Ok(vec![
            Sample::cost("VAD onset latency (median)", "ms", median(onsets)),
            Sample::cost(
                "VAD end-of-speech latency (median, incl. 250 ms pause)",
                "ms",
                median(ends),
            ),
            Sample::cost(
                "echo only, no AEC: false VAD triggers",
                "/min",
                raw_segments.len() as f64 / minutes,
            ),
            Sample::cost(
                "echo only, AEC3: false VAD triggers",
                "/min",
                clean_segments.len() as f64 / minutes,
            ),
            Sample::cost(
                "echo only, no AEC: words leaked into STT",
                "/min",
                self.words(&raw_segments) as f64 / minutes,
            ),
            Sample::cost(
                "echo only, AEC3: words leaked into STT",
                "/min",
                self.words(&clean_segments) as f64 / minutes,
            ),
            Sample {
                metric: "AEC3 echo removed (ERLE)".into(),
                unit: "dB",
                lower_is_better: false,
                value: erle,
            },
            Sample::cost(
                "user over KIVO, AEC3: onset latency (median)",
                "ms",
                median(barge),
            ),
        ])
    }

    fn notes(&self) -> Vec<String> {
        #[allow(clippy::cast_precision_loss, reason = "sample counts")]
        let seconds = self.kivo.len() as f64 / RATE as f64;
        vec![
            "Digital loop, not speakers and a microphone: KIVO's speech (Kokoro, resampled to \
             16 kHz) passes through a simulated room (direct path 40 ms late at −6 dB, \
             reflections at 60 ms/−12 dB and 110 ms/−18 dB, noise at about −60 dBFS)."
                .into(),
            format!(
                "KIVO speaks for {seconds:.0} s per run; user speech is LibriSpeech test-clean."
            ),
            "Silero VAD v6 through sherpa-onnx (threshold 0.5, 250 ms pause, 512-sample \
             window); AEC3 from sonora 0.2 (pure-Rust WebRTC); leaked words counted with \
             Parakeet TDT v3. The OS echo canceller (VOICE §3) needs real devices and is \
             measured with VOICE-30 (M2)."
                .into(),
        ]
    }
}
