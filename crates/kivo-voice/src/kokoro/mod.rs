//! Kokoro-82M (Apache-2.0), a local neural voice (VOICE §3, VOICE-09): the ONNX model on `ort`,
//! fed phonemes from KIVO's own English phonemizer (`g2p`, no espeak). Speech streams sentence by
//! sentence at 24 kHz, like the system voices.
//!
//! The model folder (downloaded by the model manager) holds `model_fp16.onnx` (or the older
//! `model_quantized.onnx`), the voices as
//! `voices/<id>.bin` (510 × 256 style vectors each, one per phoneme count) and misaki's US
//! dictionaries `us_gold.json` and `us_silver.json`.

pub mod g2p;
pub mod numbers;
pub mod rules;

use crate::engine::{Accel, EngineInfo, EngineKind, EngineSlot, ResourceEstimate};
use crate::error::{VoiceError, VoiceResult};
use crate::system_tts::sentences;
use crate::traits::{AudioSink, TtsEngine, VoiceInfo};
use ort::session::Session;
use ort::value::Tensor;
use std::path::Path;
use tokio_util::sync::CancellationToken;

pub const ENGINE_ID: &str = "kokoro-82m";
/// Kokoro speaks at 24 kHz.
pub const SAMPLE_RATE: u32 = 24_000;
/// Style vectors per voice file (one per phoneme count) and their width.
const STYLES: usize = 510;
const STYLE_WIDTH: usize = 256;
/// The most phonemes one pass takes (the model's context, less the two pads).
const MAX_PHONEMES: usize = 510;
pub const DEFAULT_VOICE: &str = "af_heart";
/// The voices KIVO downloads with the model: six English ones, then the Anime ones (Kokoro's
/// Japanese voices reading English through KIVO's English phonemizer, so with a Japanese accent).
pub const VOICES: [&str; 9] = [
    "af_heart",
    "af_bella",
    "am_michael",
    "am_fenrir",
    "bf_emma",
    "bm_george",
    "jf_alpha",
    "jf_tebukuro",
    "jm_kumo",
];

/// One of the Anime voices (a Japanese voice).
pub fn is_anime(id: &str) -> bool {
    id.starts_with('j')
}

pub fn info() -> EngineInfo {
    EngineInfo {
        id: ENGINE_ID.into(),
        name: "Kokoro".into(),
        slot: EngineSlot::Tts,
        kind: EngineKind::Local,
        license: "Apache-2.0".into(),
        languages: vec!["en".into()],
        streaming: true,
        accel: vec![Accel::Cpu],
        resources: ResourceEstimate {
            ram_mb: 400,
            vram_mb: 0,
            disk_mb: 175,
        },
        model: Some(ENGINE_ID.into()),
    }
}

/// Voice names as the picker shows them: "af_heart" → "Heart (American, female)".
pub fn voice_name(id: &str) -> String {
    let (prefix, name) = id.split_once('_').unwrap_or(("", id));
    let accent = match prefix.chars().next() {
        Some('a') => "American",
        Some('b') => "British",
        Some('j') => "Japanese accent",
        _ => "",
    };
    let gender = match prefix.chars().nth(1) {
        Some('f') => "female",
        Some('m') => "male",
        _ => "",
    };
    let mut title = name.to_owned();
    if let Some(first) = title.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    format!("{title} ({accent}, {gender})")
}

pub struct Kokoro {
    info: EngineInfo,
    session: Session,
    lexicon: g2p::Lexicon,
    /// id → 510 × 256 style vectors.
    voices: Vec<(String, Vec<f32>)>,
    /// Speaking speed (the model's own `speed` input).
    speed: f32,
}

impl Kokoro {
    /// Loads the model from `dir`, running on `threads` CPU threads.
    pub fn load(dir: &Path, threads: usize) -> VoiceResult<Self> {
        // fp16 is about three times faster on a CPU than the 8-bit file (0.28 vs 0.87 × real
        // time, DECISIONS "Kokoro in fp16"); downloads from before the switch keep working.
        let model = ["model_fp16.onnx", "model_quantized.onnx"]
            .iter()
            .map(|name| dir.join(name))
            .find(|p| p.is_file())
            .ok_or_else(|| VoiceError::ModelMissing(ENGINE_ID.into()))?;
        let session = crate::onnx::session(&model, threads)?;
        let mut voices = Vec::new();
        let listing = std::fs::read_dir(dir.join("voices"))
            .map_err(|e| VoiceError::Engine(format!("voices: {e}")))?;
        for entry in listing.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "bin")
                && let Some(id) = path.file_stem().and_then(|s| s.to_str())
            {
                let bytes =
                    std::fs::read(&path).map_err(|e| VoiceError::Engine(format!("{id}: {e}")))?;
                if bytes.len() != STYLES * STYLE_WIDTH * 4 {
                    return Err(VoiceError::Engine(format!("{id}: unexpected size")));
                }
                let styles = bytes
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .collect();
                voices.push((id.to_owned(), styles));
            }
        }
        if voices.is_empty() {
            return Err(VoiceError::ModelMissing(ENGINE_ID.into()));
        }
        voices.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(Self {
            info: info(),
            session,
            lexicon: g2p::Lexicon::load(dir)?,
            voices,
            speed: 1.0,
        })
    }

    fn style(&self, voice: Option<&str>, phonemes: usize) -> &[f32] {
        let styles = self
            .voices
            .iter()
            .find(|(id, _)| Some(id.as_str()) == voice)
            .or_else(|| self.voices.iter().find(|(id, _)| id == DEFAULT_VOICE))
            .unwrap_or(&self.voices[0]);
        let row = phonemes.min(STYLES - 1);
        &styles.1[row * STYLE_WIDTH..(row + 1) * STYLE_WIDTH]
    }

    /// One pass of the model: phoneme ids (at most `MAX_PHONEMES`) → 24 kHz audio.
    /// A run stops as soon as `cancel` fires (a long sentence takes seconds).
    fn synthesize(
        &mut self,
        ids: &[i64],
        voice: Option<&str>,
        cancel: &CancellationToken,
    ) -> VoiceResult<Vec<f32>> {
        let mut input = Vec::with_capacity(ids.len() + 2);
        input.push(0);
        input.extend_from_slice(ids);
        input.push(0);
        let style = self.style(voice, ids.len()).to_vec();
        let speed = self.speed;
        let session = &mut self.session;
        crate::interrupt::interruptible(cancel, |options| {
            let outputs = session.run_with_options(
                ort::inputs![
                    "input_ids" => Tensor::from_array(([1usize, input.len()], input))?,
                    "style" => Tensor::from_array(([1usize, STYLE_WIDTH], style))?,
                    "speed" => Tensor::from_array(([1usize], vec![speed]))?,
                ],
                options,
            )?;
            let (_, audio) = outputs[0].try_extract_tensor::<f32>()?;
            Ok(audio.to_vec())
        })
    }

    /// The phonemes KIVO would give Kokoro for `text` (diagnostics and tests).
    pub fn phonemes(&self, text: &str) -> String {
        self.lexicon.phonemize(text)
    }
}

/// Phoneme ids for Kokoro's vocabulary; symbols it doesn't know are dropped.
pub fn token_ids(phonemes: &str) -> Vec<i64> {
    phonemes.chars().filter_map(vocab).collect()
}

/// Splits a long phoneme string at spaces so each piece fits one pass.
fn chunks(ids: &[i64]) -> Vec<&[i64]> {
    const SPACE: i64 = 16;
    let mut out = Vec::new();
    let mut rest = ids;
    while rest.len() > MAX_PHONEMES {
        let cut = rest[..MAX_PHONEMES]
            .iter()
            .rposition(|&t| t == SPACE)
            .filter(|&i| i > 0)
            .unwrap_or(MAX_PHONEMES);
        out.push(&rest[..cut]);
        rest = &rest[cut..];
    }
    if !rest.is_empty() {
        out.push(rest);
    }
    out
}

impl TtsEngine for Kokoro {
    fn info(&self) -> &EngineInfo {
        &self.info
    }

    fn set_speed(&mut self, speed: f32) {
        self.speed = speed.clamp(0.5, 2.0);
    }

    fn voices(&self) -> Vec<VoiceInfo> {
        self.voices
            .iter()
            .map(|(id, _)| VoiceInfo {
                id: id.clone(),
                name: voice_name(id),
                language: if id.starts_with('b') {
                    "en-GB"
                } else {
                    "en-US"
                }
                .into(),
            })
            .collect()
    }

    fn speak(
        &mut self,
        text: &str,
        voice: Option<&str>,
        cancel: &CancellationToken,
        sink: AudioSink<'_>,
    ) -> VoiceResult<()> {
        for sentence in sentences(text) {
            let ids = token_ids(&self.lexicon.phonemize(sentence));
            for piece in chunks(&ids) {
                if cancel.is_cancelled() {
                    return Err(VoiceError::Cancelled);
                }
                let audio = self.synthesize(piece, voice, cancel)?;
                if cancel.is_cancelled() {
                    return Err(VoiceError::Cancelled);
                }
                sink(&audio, SAMPLE_RATE)?;
            }
        }
        Ok(())
    }
}

/// Kokoro 1.0's phoneme vocabulary (its `tokenizer.json`).
#[allow(clippy::match_same_arms, reason = "a table")]
fn vocab(c: char) -> Option<i64> {
    Some(match c {
        ';' => 1,
        ':' => 2,
        ',' => 3,
        '.' => 4,
        '!' => 5,
        '?' => 6,
        '—' => 9,
        '…' => 10,
        '"' => 11,
        '(' => 12,
        ')' => 13,
        '“' => 14,
        '”' => 15,
        ' ' => 16,
        '\u{0303}' => 17,
        'ʣ' => 18,
        'ʥ' => 19,
        'ʦ' => 20,
        'ʨ' => 21,
        'ᵝ' => 22,
        'ꭧ' => 23,
        'A' => 24,
        'I' => 25,
        'O' => 31,
        'Q' => 33,
        'S' => 35,
        'T' => 36,
        'W' => 39,
        'Y' => 41,
        'ᵊ' => 42,
        'a' => 43,
        'b' => 44,
        'c' => 45,
        'd' => 46,
        'e' => 47,
        'f' => 48,
        'h' => 50,
        'i' => 51,
        'j' => 52,
        'k' => 53,
        'l' => 54,
        'm' => 55,
        'n' => 56,
        'o' => 57,
        'p' => 58,
        'q' => 59,
        'r' => 60,
        's' => 61,
        't' => 62,
        'u' => 63,
        'v' => 64,
        'w' => 65,
        'x' => 66,
        'y' => 67,
        'z' => 68,
        'ɑ' => 69,
        'ɐ' => 70,
        'ɒ' => 71,
        'æ' => 72,
        'β' => 75,
        'ɔ' => 76,
        'ɕ' => 77,
        'ç' => 78,
        'ɖ' => 80,
        'ð' => 81,
        'ʤ' => 82,
        'ə' => 83,
        'ɚ' => 85,
        'ɛ' => 86,
        'ɜ' => 87,
        'ɟ' => 90,
        'ɡ' => 92,
        'ɥ' => 99,
        'ɨ' => 101,
        'ɪ' => 102,
        'ʝ' => 103,
        'ɯ' => 110,
        'ɰ' => 111,
        'ŋ' => 112,
        'ɳ' => 113,
        'ɲ' => 114,
        'ɴ' => 115,
        'ø' => 116,
        'ɸ' => 118,
        'θ' => 119,
        'œ' => 120,
        'ɹ' => 123,
        'ɾ' => 125,
        'ɻ' => 126,
        'ʁ' => 128,
        'ɽ' => 129,
        'ʂ' => 130,
        'ʃ' => 131,
        'ʈ' => 132,
        'ʧ' => 133,
        'ʊ' => 135,
        'ʋ' => 136,
        'ʌ' => 138,
        'ɣ' => 139,
        'ɤ' => 140,
        'χ' => 142,
        'ʎ' => 143,
        'ʒ' => 147,
        'ʔ' => 148,
        'ˈ' => 156,
        'ˌ' => 157,
        'ː' => 158,
        'ʰ' => 162,
        'ʲ' => 164,
        '↓' => 169,
        '→' => 171,
        '↗' => 172,
        '↘' => 173,
        'ᵻ' => 177,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phonemes_map_to_kokoros_vocabulary() {
        assert_eq!(
            token_ids("həlˈO, wˈɜɹld!"),
            [50, 83, 54, 156, 31, 3, 16, 65, 156, 87, 123, 54, 46, 5]
        );
        assert!(token_ids("❓").is_empty(), "unknown symbols are dropped");
    }

    #[test]
    fn long_input_is_cut_at_spaces() {
        let mut ids = Vec::new();
        for _ in 0..200 {
            ids.extend_from_slice(&[50, 83, 16]);
        }
        let pieces = chunks(&ids);
        assert!(pieces.len() >= 2);
        assert!(pieces.iter().all(|p| p.len() <= MAX_PHONEMES));
        assert_eq!(pieces.iter().map(|p| p.len()).sum::<usize>(), ids.len());
    }

    #[test]
    fn voice_names_read_well() {
        assert_eq!(voice_name("af_heart"), "Heart (American, female)");
        assert_eq!(voice_name("bm_george"), "George (British, male)");
        assert_eq!(voice_name("jf_alpha"), "Alpha (Japanese accent, female)");
        assert!(is_anime("jm_kumo") && !is_anime("am_michael"));
    }

    /// Runs the real model when the Kokoro download is on this PC (like the Moonshine test).
    #[test]
    fn speaks_a_sentence_when_the_model_is_installed() {
        let Some(dir) = installed() else {
            eprintln!("Kokoro isn't installed on this machine; skipping");
            return;
        };
        let mut kokoro = Kokoro::load(&dir, 4).unwrap();
        let mut audio = Vec::new();
        let started = std::time::Instant::now();
        kokoro
            .speak(
                "Opening Google Chrome.",
                None,
                &CancellationToken::new(),
                &mut |pcm: &[f32], rate: u32| {
                    assert_eq!(rate, SAMPLE_RATE);
                    audio.extend_from_slice(pcm);
                    Ok(())
                },
            )
            .unwrap();
        #[allow(clippy::cast_precision_loss, reason = "a short clip")]
        let seconds = audio.len() as f32 / SAMPLE_RATE as f32;
        eprintln!("{seconds:.2} s of speech in {:?}", started.elapsed());
        assert!((0.6..4.0).contains(&seconds), "{seconds} s");
        let peak = audio.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.05, "audible: peak {peak}");
    }

    fn installed() -> Option<std::path::PathBuf> {
        let candidates = [
            std::env::var_os("KIVO_KOKORO_DIR").map(std::path::PathBuf::from),
            std::env::var_os("LOCALAPPDATA").map(|d| {
                std::path::PathBuf::from(d)
                    .join("KIVO")
                    .join("models")
                    .join(ENGINE_ID)
            }),
        ];
        candidates.into_iter().flatten().find(|dir| {
            dir.join("model_fp16.onnx").is_file() || dir.join("model_quantized.onnx").is_file()
        })
    }
}

#[cfg(test)]
mod listen {
    /// Writes a sample to `KIVO_KOKORO_WAV` (a manual check; skipped unless both variables are set).
    #[test]
    fn write_a_sample_to_listen_to() {
        let (Some(dir), Some(out)) = (
            std::env::var_os("KIVO_KOKORO_DIR"),
            std::env::var_os("KIVO_KOKORO_WAV"),
        ) else {
            return;
        };
        use crate::traits::TtsEngine as _;
        let mut kokoro = super::Kokoro::load(std::path::Path::new(&dir), 4).unwrap();
        for text in [
            "Opening Google Chrome.",
            "The volume is at 50%. I'll remind you to call Maya at 5 PM.",
            "Kivo reads the news: Spotify's playing Lo-fi Focus, 2026 edition.",
        ] {
            eprintln!("{text}\n  → {}", kokoro.phonemes(text));
        }
        let mut audio = Vec::new();
        kokoro
            .speak(
                "Hi, I'm Kivo. The volume is at 50%. I'll remind you to call Maya at 5 PM.",
                None,
                &tokio_util::sync::CancellationToken::new(),
                &mut |pcm: &[f32], _| {
                    audio.extend_from_slice(pcm);
                    Ok(())
                },
            )
            .unwrap();
        let mut wav = Vec::new();
        let data = u32::try_from(audio.len() * 2).unwrap();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&super::SAMPLE_RATE.to_le_bytes());
        wav.extend_from_slice(&(super::SAMPLE_RATE * 2).to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data.to_le_bytes());
        for s in audio {
            #[allow(clippy::cast_possible_truncation, reason = "16-bit PCM")]
            wav.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
        }
        std::fs::write(out, wav).unwrap();
    }
}
