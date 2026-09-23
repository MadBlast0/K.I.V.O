//! Recognizing the owner's voice with CAM++ (3D-Speaker, Apache-2.0; VOICE §5): a clip becomes a
//! 512-number embedding, and a voice is compared with the enrolled profile by cosine similarity.
//! It decides whose preferences and memory to use and filters out other voices; it never
//! authorizes a risky action (SECURITY §5). The model is a download the user chooses when they
//! enroll their voice.

use crate::engine::{Accel, EngineInfo, EngineKind, EngineSlot, ResourceEstimate};
use crate::error::{VoiceError, VoiceResult};
use crate::fbank::{self, BINS};
use crate::traits::{Embedding, SpeakerVerifier};
use ort::session::Session;
use ort::value::Tensor;
use std::path::Path;
use std::sync::Mutex;

pub const ENGINE_ID: &str = "campplus-voxceleb-en";
/// Shorter clips give unreliable embeddings.
pub const MIN_SAMPLES: usize = fbank::SAMPLE_RATE / 2;

pub fn info() -> EngineInfo {
    EngineInfo {
        id: ENGINE_ID.into(),
        name: "Voice recognition (CAM++)".into(),
        slot: EngineSlot::Speaker,
        kind: EngineKind::Local,
        license: "Apache-2.0".into(),
        languages: vec!["*".into()],
        streaming: false,
        accel: vec![Accel::Cpu],
        resources: ResourceEstimate {
            ram_mb: 60,
            vram_mb: 0,
            disk_mb: 29,
        },
        model: Some(ENGINE_ID.into()),
    }
}

pub struct CamPlusPlus {
    info: EngineInfo,
    session: Mutex<Session>,
}

impl CamPlusPlus {
    /// Loads `campplus.onnx` from the model folder.
    pub fn load(dir: &Path) -> VoiceResult<Self> {
        let path = dir.join("campplus.onnx");
        if !path.is_file() {
            return Err(VoiceError::ModelMissing("voice recognition".into()));
        }
        let session = Session::builder()?
            .with_intra_threads(1)?
            .with_inter_threads(1)?
            .commit_from_file(path)?;
        Ok(Self {
            info: info(),
            session: Mutex::new(session),
        })
    }
}

/// The profile's centroid: the mean of its embeddings, each normalized first. Scoring against
/// it is steadier than against any one clip (short clips give noisy embeddings).
pub fn centroid(embeddings: &[Embedding]) -> Embedding {
    let Some(dim) = embeddings.first().map(Vec::len) else {
        return Vec::new();
    };
    let mut sum = vec![0.0_f32; dim];
    for e in embeddings {
        let norm = e.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
        for (s, x) in sum.iter_mut().zip(e) {
            *s += x / norm;
        }
    }
    sum
}

/// Cosine similarity, with opposite directions counted as 0 (the scale speaker-verification
/// thresholds are published on).
pub fn similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm = |v: &[f32]| v.iter().map(|x| x * x).sum::<f32>().sqrt();
    let (na, nb) = (norm(a), norm(b));
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    (dot / (na * nb)).clamp(0.0, 1.0)
}

impl SpeakerVerifier for CamPlusPlus {
    fn info(&self) -> &EngineInfo {
        &self.info
    }

    fn embed(&self, audio: &[f32]) -> VoiceResult<Embedding> {
        if audio.len() < MIN_SAMPLES {
            return Err(VoiceError::Engine(
                "the clip is too short to recognize".into(),
            ));
        }
        let mut frames = fbank::compute(audio);
        // The model expects each bin's mean over the clip removed ("global-mean").
        let mut mean = [0.0_f32; BINS];
        for frame in &frames {
            for (m, v) in mean.iter_mut().zip(frame) {
                *m += v;
            }
        }
        let count = frames.len().max(1) as f32;
        mean.iter_mut().for_each(|m| *m /= count);
        for frame in &mut frames {
            for (v, m) in frame.iter_mut().zip(&mean) {
                *v -= m;
            }
        }
        let t = frames.len();
        let x: Vec<f32> = frames.into_iter().flatten().collect();
        let mut session = self
            .session
            .lock()
            .map_err(|_| VoiceError::Engine("the voice model is unavailable".into()))?;
        let outputs =
            session.run(ort::inputs!["x" => Tensor::from_array(([1usize, t, BINS], x))?])?;
        let (_, embedding) = outputs["embedding"].try_extract_tensor::<f32>()?;
        Ok(embedding.to_vec())
    }

    /// Similarity to the profile's centroid.
    fn score(&self, embedding: &Embedding, profile: &[Embedding]) -> f32 {
        similarity(embedding, &centroid(profile))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn similarity_is_the_cosine_with_opposites_at_zero() {
        assert!((similarity(&[1.0, 2.0], &[2.0, 4.0]) - 1.0).abs() < 1e-6);
        assert!(similarity(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
        assert!(
            (similarity(&[1.0, 1.0], &[1.0, 0.0]) - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-5
        );
        assert!(similarity(&[1.0, 0.0], &[-1.0, 0.0]).abs() < 1e-6);
        assert_eq!(similarity(&[0.0, 0.0], &[1.0, 0.0]), 0.0);
    }

    fn wav(path: &Path) -> Vec<f32> {
        let data = std::fs::read(path).unwrap();
        data[44..]
            .chunks_exact(2)
            .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32_768.0)
            .collect()
    }

    /// `KIVO_CAMPPLUS_DIR` holds `campplus.onnx`; `KIVO_KWS_DIR` the two test recordings (two
    /// different speakers from LibriSpeech).
    #[test]
    fn the_same_voice_scores_higher_than_another() {
        let (Some(model), Some(kws)) = (
            std::env::var_os("KIVO_CAMPPLUS_DIR").map(std::path::PathBuf::from),
            std::env::var_os("KIVO_KWS_DIR").map(std::path::PathBuf::from),
        ) else {
            eprintln!("KIVO_CAMPPLUS_DIR / KIVO_KWS_DIR not set; skipping");
            return;
        };
        let cam = CamPlusPlus::load(&model).unwrap();
        let one = wav(&kws.join("test_wavs/1.wav"));
        let other = wav(&kws.join("test_wavs/0.wav"));
        let (first, second) = one.split_at(one.len() / 2);
        let a = cam.embed(first).unwrap();
        let b = cam.embed(second).unwrap();
        let c = cam.embed(&other).unwrap();
        assert_eq!(a.len(), 512);
        let same = cam.score(&a, &[b]);
        let different = cam.score(&a, std::slice::from_ref(&c));
        eprintln!("same speaker {same:.3}, different speakers {different:.3}");
        // sherpa-onnx's own extractor gives 0.956 and 0.754 on these clips.
        assert!((same - 0.956).abs() < 0.02, "{same}");
        assert!((different - 0.754).abs() < 0.02, "{different}");
    }
}
