//! `wake` (BENCHMARKS §1, BENCH-04): "Hey Kivo" with sherpa-onnx open-vocabulary keyword
//! spotting (VOICE §3, the custom-word engine). No recordings from the owner are needed: the
//! positives are synthetic ("Hey Kivo" in every Kokoro voice at three speeds, through the
//! simulated room, clean and over background speech), as wake-word models are trained; the
//! negatives are LibriSpeech test-clean speech. Measured: false rejects, false accepts per hour,
//! and the spotter's CPU cost (real-time factor on one thread).
//!
//! `KIVO_BENCH_WAKE_NEGATIVE_MINUTES` limits the negative audio per run (default: all of it).

use super::data::{self, Entry};
use crate::harness::{Sample, Suite};
use sherpa_onnx::{
    GenerationConfig, KeywordSpotter, KeywordSpotterConfig, LinearResampler, OfflineTts,
    OfflineTtsConfig,
};
use std::time::Instant;

const RATE: usize = 16_000;
const PHRASE: &str = "HEY KIVO";
const SPEEDS: [f32; 3] = [0.85, 1.0, 1.15];

pub struct Wake {
    config: KeywordSpotterConfig,
    /// The phrase as the model's sub-word tokens.
    tokens: String,
    positives: Vec<Vec<f32>>,
    /// Decoded one at a time during the run (5 h of audio would not fit comfortably in memory).
    negatives: Vec<Entry>,
    negative_seconds: f64,
}

/// Splits `phrase` into the model's word-piece tokens (longest match from `vocabulary`), the
/// form sherpa-onnx keyword spotting expects: "▁HE Y ▁KI VO".
pub fn tokenize(phrase: &str, vocabulary: &[String]) -> Result<String, String> {
    let mut out = Vec::new();
    for word in phrase.split_whitespace() {
        let mut rest = format!("\u{2581}{word}");
        while !rest.is_empty() {
            let piece = (1..=rest.chars().count())
                .rev()
                .map(|n| rest.chars().take(n).collect::<String>())
                .find(|p| vocabulary.contains(p))
                .ok_or_else(|| format!("no token for \"{rest}\" in the keyword model"))?;
            rest = rest[piece.len()..].to_owned();
            out.push(piece);
        }
    }
    Ok(out.join(" "))
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

impl Wake {
    pub fn start() -> Result<Self, String> {
        let dir = data::model("sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01")?;
        let mut config = KeywordSpotterConfig::default();
        let model = &mut config.model_config;
        model.transducer.encoder = Some(
            data::file(&dir, &["encoder", "int8", "chunk-16-left-64"])
                .or_else(|_| data::file(&dir, &["encoder", "chunk-16-left-64"]))?,
        );
        model.transducer.decoder = Some(data::file(&dir, &["decoder", "chunk-16-left-64"])?);
        model.transducer.joiner = Some(
            data::file(&dir, &["joiner", "int8", "chunk-16-left-64"])
                .or_else(|_| data::file(&dir, &["joiner", "chunk-16-left-64"]))?,
        );
        let tokens_file = data::file(&dir, &["tokens"])?;
        model.tokens = Some(tokens_file.clone());
        model.num_threads = 1;
        model.provider = Some("cpu".into());
        config.keywords_threshold = 0.25;
        config.keywords_score = 1.0;
        let vocabulary: Vec<String> = std::fs::read_to_string(&tokens_file)
            .map_err(|e| e.to_string())?
            .lines()
            .filter_map(|l| l.split_whitespace().next().map(str::to_owned))
            .collect();
        let tokens = tokenize(PHRASE, &vocabulary)?;
        config.keywords_buf = Some(format!("{tokens} @{PHRASE}"));

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
        let mut positives = Vec::new();
        for sid in 0..tts.num_speakers() {
            for speed in SPEEDS {
                let generation = GenerationConfig {
                    sid,
                    speed,
                    ..GenerationConfig::default()
                };
                let audio = tts
                    .generate_with_config("Hey Kivo.", &generation, None::<fn(&[f32], f32) -> bool>)
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
        for entry in data::librispeech_index()? {
            let seconds = entry.seconds()?;
            if minutes.is_some_and(|m| negative_seconds + seconds > m * 60.0) {
                break;
            }
            negative_seconds += seconds;
            negatives.push(entry);
        }
        Ok(Self {
            config,
            tokens,
            positives,
            negatives,
            negative_seconds,
        })
    }

    /// How many times the phrase is spotted in `audio`.
    fn spot(spotter: &KeywordSpotter, audio: &[f32]) -> usize {
        let stream = spotter.create_stream();
        let mut hits = 0;
        for chunk in audio.chunks(RATE / 10) {
            stream.accept_waveform(16_000, chunk);
            while spotter.is_ready(&stream) {
                spotter.decode(&stream);
                if spotter
                    .get_result(&stream)
                    .is_some_and(|r| !r.keyword.is_empty())
                {
                    hits += 1;
                    spotter.reset(&stream);
                }
            }
        }
        // A little silence lets the last frames through.
        stream.accept_waveform(16_000, &vec![0.0; RATE / 2]);
        stream.input_finished();
        while spotter.is_ready(&stream) {
            spotter.decode(&stream);
            if spotter
                .get_result(&stream)
                .is_some_and(|r| !r.keyword.is_empty())
            {
                hits += 1;
                spotter.reset(&stream);
            }
        }
        hits
    }
}

impl Suite for Wake {
    fn name(&self) -> &'static str {
        "wake"
    }

    fn run(&mut self) -> Result<Vec<Sample>, String> {
        let spotter =
            KeywordSpotter::create(&self.config).ok_or("the keyword spotter failed to load")?;
        let missed = self
            .positives
            .iter()
            .filter(|p| Self::spot(&spotter, p) == 0)
            .count();
        let mut false_accepts = 0;
        let mut busy = std::time::Duration::ZERO;
        for entry in &self.negatives {
            let audio = entry.load()?;
            let t = Instant::now();
            false_accepts += Self::spot(&spotter, &audio.samples);
            busy += t.elapsed();
        }
        let busy = busy.as_secs_f64();
        #[allow(clippy::cast_precision_loss, reason = "counts")]
        Ok(vec![
            Sample::cost(
                "false rejects (synthetic positives)",
                "%",
                missed as f64 / self.positives.len() as f64 * 100.0,
            ),
            Sample::cost(
                "false accepts",
                "/hour",
                false_accepts as f64 / (self.negative_seconds / 3600.0),
            ),
            Sample::cost(
                "spotter CPU (one thread, share of real time)",
                "%",
                busy / self.negative_seconds * 100.0,
            ),
        ])
    }

    fn notes(&self) -> Vec<String> {
        vec![
            format!(
                "sherpa-onnx keyword spotting, zipformer GigaSpeech 3.3M (int8, 1 thread), \
                 keyword \"{PHRASE}\" as tokens \"{}\", threshold 0.25.",
                self.tokens
            ),
            format!(
                "Positives: {} synthetic clips (every Kokoro EN voice × speeds 0.85/1.0/1.15, \
                 through a room echo, clean and over background speech at −10 dB). No owner \
                 recordings are needed; the built-in \"Hey Kivo\" model (VOICE-13, M2) is \
                 trained the same way.",
                self.positives.len()
            ),
            format!(
                "Negatives: {:.1} h of LibriSpeech test-clean read speech. The spec asks for ≥ 10 h \
                 of mixed speech, podcasts and TV; this is the first measurement, not the \
                 acceptance figure.",
                self.negative_seconds / 3600.0
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phrases_become_longest_matching_word_pieces() {
        let vocab: Vec<String> = ["\u{2581}HE", "\u{2581}H", "Y", "\u{2581}KI", "VO", "V", "O"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        assert_eq!(
            tokenize("HEY KIVO", &vocab).unwrap(),
            "\u{2581}HE Y \u{2581}KI VO"
        );
        assert!(tokenize("HEY ZED", &vocab).is_err());
    }
}
