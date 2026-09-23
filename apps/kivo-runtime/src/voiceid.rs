//! Your voice (VOICE §5, VOICE-20/21/22; SECURITY §5). With the owner's consent KIVO records eight
//! short prompts, turns them into CAM++ voice embeddings and keeps them as the speaker profile.
//! At the end of a spoken request the whole utterance (wake word and request, long enough for a
//! steady voiceprint) tells the owner from someone else: in "Prefer owner" a stranger gets a guest
//! turn, in "Owner only" they are ignored. It is a convenience and a filter, never an
//! authorization (SECURITY §5).
//!
//! Everything biometric is encrypted with DPAPI before it reaches disk: the clips in
//! `%LOCALAPPDATA%\KIVO\data\voice\`, the embeddings in the database. The profile is the
//! embedding of all the prompts joined (30–45 s of speech) plus one per prompt of 2 s or more;
//! a voice is scored against their centroid, because short clips give noisy embeddings. It grows
//! from confident matches up to 40 embeddings, and is rebuilt from the clips if the model changes.
//! "Delete voice data" removes all of it.

use kivo_platform::Secrets;
use kivo_store::Database;
use kivo_store::models::SPEAKER_MODEL;
use kivo_voice::traits::SpeakerVerifier;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

/// VOICE §5: at most this many embeddings in the profile.
pub const MAX_EMBEDDINGS: usize = 40;
/// A turn this far above the threshold adds its embedding to the profile.
const LEARN_MARGIN: f32 = 0.1;
/// The threshold is set from the owner's own clips, within these bounds.
const THRESHOLD_MIN: f32 = 0.5;
const THRESHOLD_MAX: f32 = 0.75;
/// Clips shorter than this are asked again.
const MIN_CLIP_SECONDS: f32 = 0.8;
/// A clip this long gets its own embedding in the profile (shorter ones only join the whole).
const OWN_EMBEDDING_SAMPLES: usize = 16_000 * 2;
/// Less speech than this can't be judged: the verdict is Unknown.
const MIN_CHECK_SAMPLES: usize = 16_000;
/// At least this many good clips make a profile.
pub const MIN_CLIPS: usize = 6;

/// The enrollment prompts (research §2: the wake word ×3, wake word + command ×3, "It's me",
/// a sentence with numbers and names).
pub const PROMPTS: [&str; 8] = [
    "Hey Kivo",
    "Hey Kivo",
    "Hey Kivo",
    "Hey Kivo, what's the weather like today?",
    "Hey Kivo, open my music",
    "Hey Kivo, remind me at seven thirty to call home",
    "It's me, talking to Kivo",
    "My code is four eight one five, and today is the twenty-third",
];

/// Who said the wake word.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Verdict {
    /// No profile, or recognition is off: nobody is told apart.
    Unknown,
    Owner {
        score: f32,
    },
    Stranger {
        score: f32,
    },
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub enrolled: bool,
    /// What each prompt asks the user to say.
    pub prompts: Vec<String>,
    /// Prompts recorded in the enrollment under way.
    pub recorded: Vec<bool>,
    pub embeddings: usize,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Take {
    /// The clip is usable (speech, long enough).
    pub ok: bool,
    pub seconds: f32,
}

struct Profile {
    embeddings: Vec<Vec<f32>>,
    threshold: f32,
}

pub struct VoiceId {
    dir: PathBuf,
    secrets: Arc<dyn Secrets>,
    db: Arc<Mutex<Database>>,
    /// Loads the speaker model, once it has been downloaded.
    loader: Box<dyn Fn() -> Option<Arc<dyn SpeakerVerifier>> + Send + Sync>,
    engine: Mutex<Option<Arc<dyn SpeakerVerifier>>>,
    profile: Mutex<Option<Profile>>,
    /// The enrollment under way: each prompt's clip.
    takes: Mutex<Vec<Option<Vec<f32>>>>,
    waiting: Mutex<HashMap<u64, oneshot::Sender<Vec<f32>>>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn to_bytes(samples: &[f32]) -> Vec<u8> {
    samples.iter().flat_map(|s| s.to_le_bytes()).collect()
}

fn from_bytes(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

#[allow(clippy::cast_precision_loss, reason = "seconds of audio")]
fn seconds(samples: &[f32]) -> f32 {
    samples.len() as f32 / 16_000.0
}

/// The match threshold for this person: a little below how alike their own clips are.
pub fn calibrate(embeddings: &[Vec<f32>]) -> f32 {
    let mut lowest = f32::INFINITY;
    for (i, a) in embeddings.iter().enumerate() {
        // Each clip against the centroid of the rest, as a turn is scored against the profile.
        let rest: Vec<Vec<f32>> = embeddings
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, b)| b.clone())
            .collect();
        if rest.is_empty() {
            continue;
        }
        let score = kivo_voice::speaker::similarity(a, &kivo_voice::speaker::centroid(&rest));
        lowest = lowest.min(score);
    }
    if lowest.is_finite() {
        (lowest - 0.1).clamp(THRESHOLD_MIN, THRESHOLD_MAX)
    } else {
        THRESHOLD_MIN
    }
}

impl VoiceId {
    pub fn new(
        dir: PathBuf,
        secrets: Arc<dyn Secrets>,
        db: Arc<Mutex<Database>>,
        loader: impl Fn() -> Option<Arc<dyn SpeakerVerifier>> + Send + Sync + 'static,
    ) -> Self {
        Self {
            dir,
            secrets,
            db,
            loader: Box::new(loader),
            engine: Mutex::new(None),
            profile: Mutex::new(None),
            takes: Mutex::new(vec![None; PROMPTS.len()]),
            waiting: Mutex::new(HashMap::new()),
        }
    }

    fn engine(&self) -> Option<Arc<dyn SpeakerVerifier>> {
        let mut engine = lock(&self.engine);
        if engine.is_none() {
            *engine = (self.loader)();
        }
        engine.clone()
    }

    /// The profile's embeddings from the enrollment clips: all of them joined, and each clip long
    /// enough to stand on its own.
    fn embeddings_from(
        engine: &dyn SpeakerVerifier,
        clips: &[Vec<f32>],
    ) -> Result<Vec<Vec<f32>>, String> {
        let joined: Vec<f32> = clips.iter().flatten().copied().collect();
        let mut embeddings = vec![engine.embed(&joined).map_err(|e| e.detail())?];
        for clip in clips.iter().filter(|c| c.len() >= OWN_EMBEDDING_SAMPLES) {
            embeddings.push(engine.embed(clip).map_err(|e| e.detail())?);
        }
        Ok(embeddings)
    }

    /// The stored profile, decrypted (and rebuilt from the clips if it was made with another
    /// model).
    fn load_profile(&self) -> Option<()> {
        if lock(&self.profile).is_some() {
            return Some(());
        }
        let stored = lock(&self.db).speaker_profile().ok()??;
        if stored.model_id != SPEAKER_MODEL {
            tracing::info!("the voice model changed; rebuilding the profile from the clips");
            self.rebuild().ok()?;
            return lock(&self.profile).is_some().then_some(());
        }
        let plain = self.secrets.unprotect(&stored.embeddings).ok()?;
        let embeddings: Vec<Vec<f32>> = serde_json::from_slice(&plain).ok()?;
        *lock(&self.profile) = Some(Profile {
            embeddings,
            threshold: stored.threshold,
        });
        Some(())
    }

    fn save_profile(&self, profile: &Profile) -> Result<(), String> {
        let json = serde_json::to_vec(&profile.embeddings).map_err(|e| e.to_string())?;
        let sealed = self.secrets.protect(&json).map_err(|e| e.to_string())?;
        lock(&self.db)
            .save_speaker_profile(SPEAKER_MODEL, profile.threshold, &sealed)
            .map_err(|e| e.to_string())
    }

    /// Rebuilds the profile from the stored clips (a new model, VOICE-21).
    pub fn rebuild(&self) -> Result<(), String> {
        let engine = self.engine().ok_or("the voice model isn't downloaded")?;
        let clips = lock(&self.db).voice_clips().map_err(|e| e.to_string())?;
        let mut audio = Vec::new();
        for clip in clips {
            let sealed = std::fs::read(self.dir.join(&clip.file)).map_err(|e| e.to_string())?;
            audio.push(from_bytes(
                &self.secrets.unprotect(&sealed).map_err(|e| e.to_string())?,
            ));
        }
        let embeddings = Self::embeddings_from(engine.as_ref(), &audio)?;
        let profile = Profile {
            threshold: calibrate(&embeddings),
            embeddings,
        };
        self.save_profile(&profile)?;
        *lock(&self.profile) = Some(profile);
        Ok(())
    }

    /// Who said `clip` (a whole spoken request). Confident owner matches of 2 s or more grow the
    /// profile.
    pub fn check(&self, clip: &[f32]) -> Verdict {
        if clip.len() < MIN_CHECK_SAMPLES || self.load_profile().is_none() {
            return Verdict::Unknown;
        }
        let Some(engine) = self.engine() else {
            return Verdict::Unknown;
        };
        let Ok(embedding) = engine.embed(clip) else {
            return Verdict::Unknown;
        };
        let mut guard = lock(&self.profile);
        let Some(profile) = guard.as_mut() else {
            return Verdict::Unknown;
        };
        let score = engine.score(&embedding, &profile.embeddings);
        if score < profile.threshold {
            return Verdict::Stranger { score };
        }
        if score >= profile.threshold + LEARN_MARGIN
            && clip.len() >= OWN_EMBEDDING_SAMPLES
            && profile.embeddings.len() < MAX_EMBEDDINGS
        {
            profile.embeddings.push(embedding);
            if let Err(e) = self.save_profile(profile) {
                tracing::warn!(e, "couldn't save the voice profile");
            }
        }
        Verdict::Owner { score }
    }

    pub fn status(&self) -> Status {
        let enrolled = lock(&self.db).speaker_profile().ok().flatten().is_some();
        let embeddings = if enrolled && self.load_profile().is_some() {
            lock(&self.profile)
                .as_ref()
                .map_or(0, |p| p.embeddings.len())
        } else {
            0
        };
        Status {
            enrolled,
            prompts: PROMPTS.iter().map(|p| (*p).to_owned()).collect(),
            recorded: lock(&self.takes).iter().map(Option::is_some).collect(),
            embeddings,
        }
    }

    /// Waits for the listener to return clip `id` (see `recorded`).
    pub fn expect(&self, id: u64) -> oneshot::Receiver<Vec<f32>> {
        let (tx, rx) = oneshot::channel();
        lock(&self.waiting).insert(id, tx);
        rx
    }

    /// The listener finished recording clip `id`.
    pub fn recorded(&self, id: u64, audio: Vec<f32>) {
        if let Some(tx) = lock(&self.waiting).remove(&id) {
            let _ = tx.send(audio);
        }
    }

    /// Keeps a recorded prompt (or says it needs doing again).
    pub fn keep_take(&self, prompt: usize, audio: Vec<f32>) -> Take {
        let seconds = seconds(&audio);
        let ok = seconds >= MIN_CLIP_SECONDS;
        if let Some(slot) = lock(&self.takes).get_mut(prompt) {
            *slot = ok.then_some(audio);
        }
        Take { ok, seconds }
    }

    /// Drops an unfinished enrollment.
    pub fn cancel_enrollment(&self) {
        *lock(&self.takes) = vec![None; PROMPTS.len()];
    }

    /// Builds the profile from the recorded prompts and stores it all encrypted.
    pub fn finish_enrollment(&self) -> Result<Status, String> {
        let takes: Vec<(usize, Vec<f32>)> = lock(&self.takes)
            .iter()
            .enumerate()
            .filter_map(|(i, t)| t.clone().map(|a| (i, a)))
            .collect();
        if takes.len() < MIN_CLIPS {
            return Err(kivo_core::text::tf(
                "voiceId.needMore",
                &[("count", &MIN_CLIPS)],
            ));
        }
        let engine = self
            .engine()
            .ok_or_else(|| kivo_core::text::t("voiceId.modelMissing"))?;
        let clips: Vec<Vec<f32>> = takes.iter().map(|(_, a)| a.clone()).collect();
        let embeddings = Self::embeddings_from(engine.as_ref(), &clips)?;
        // Replace any earlier enrollment completely.
        self.delete()?;
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        for (prompt, audio) in &takes {
            let file = format!("clip-{prompt}-{}.bin", uuid::Uuid::now_v7().simple());
            let sealed = self
                .secrets
                .protect(&to_bytes(audio))
                .map_err(|e| e.to_string())?;
            std::fs::write(self.dir.join(&file), sealed).map_err(|e| e.to_string())?;
            lock(&self.db)
                .add_voice_clip(PROMPTS[*prompt], &file)
                .map_err(|e| e.to_string())?;
        }
        let profile = Profile {
            threshold: calibrate(&embeddings),
            embeddings,
        };
        self.save_profile(&profile)?;
        *lock(&self.profile) = Some(profile);
        self.cancel_enrollment();
        Ok(self.status())
    }

    /// "Delete voice data": the clips, their rows and the profile (VOICE-20).
    pub fn delete(&self) -> Result<(), String> {
        let files = lock(&self.db)
            .delete_voice_data()
            .map_err(|e| e.to_string())?;
        for file in files {
            let path = self.dir.join(&file);
            if path.exists() {
                std::fs::remove_file(&path).map_err(|e| e.to_string())?;
            }
        }
        *lock(&self.profile) = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in voice model: a clip's "voice" is its dominant pitch, found from its zero
    /// crossings, as a direction on a circle; the same pitch is the same voice.
    struct PitchVoice;

    impl SpeakerVerifier for PitchVoice {
        fn info(&self) -> &kivo_voice::EngineInfo {
            static INFO: std::sync::OnceLock<kivo_voice::EngineInfo> = std::sync::OnceLock::new();
            INFO.get_or_init(kivo_voice::speaker::info)
        }
        fn embed(&self, audio: &[f32]) -> kivo_voice::VoiceResult<Vec<f32>> {
            let crossings = audio
                .windows(2)
                .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
                .count();
            #[allow(clippy::cast_precision_loss)]
            let rate = crossings as f32 / audio.len().max(1) as f32;
            let angle = rate * 40.0;
            Ok(vec![angle.cos(), angle.sin(), 0.2])
        }
        fn score(&self, embedding: &Vec<f32>, profile: &[Vec<f32>]) -> f32 {
            kivo_voice::speaker::similarity(embedding, &kivo_voice::speaker::centroid(profile))
        }
    }

    /// `seconds` of a voice at `hz`.
    fn voice(hz: f32, seconds: f32) -> Vec<f32> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = (16_000.0 * seconds) as usize;
        #[allow(clippy::cast_precision_loss)]
        (0..n)
            .map(|i| (i as f32 / 16_000.0 * hz * std::f32::consts::TAU).sin() * 0.3)
            .collect()
    }

    fn voice_id(dir: &std::path::Path) -> VoiceId {
        VoiceId::new(
            dir.to_path_buf(),
            Arc::new(kivo_testkit::FakeSecrets::default()),
            Arc::new(Mutex::new(Database::in_memory().unwrap())),
            || None,
        )
    }

    fn with_model(dir: &std::path::Path) -> VoiceId {
        VoiceId::new(
            dir.to_path_buf(),
            Arc::new(kivo_testkit::FakeSecrets::default()),
            Arc::new(Mutex::new(Database::in_memory().unwrap())),
            || Some(Arc::new(PitchVoice) as Arc<dyn SpeakerVerifier>),
        )
    }

    #[test]
    fn enrollment_is_encrypted_the_owner_recognized_and_deletion_complete() {
        let tmp = tempfile::tempdir().unwrap();
        let id = with_model(&tmp.path().join("voice"));
        // Eight prompts in the owner's voice (short wake words and longer sentences).
        for p in 0..8 {
            let seconds = if p < 3 { 1.0 } else { 2.5 };
            assert!(id.keep_take(p, voice(180.0, seconds)).ok);
        }
        let status = id.finish_enrollment().unwrap();
        assert!(status.enrolled);
        // The joined prompts, plus the five prompts of 2 s or more.
        assert_eq!(status.embeddings, 6);
        let files: Vec<_> = std::fs::read_dir(tmp.path().join("voice"))
            .unwrap()
            .map(|f| f.unwrap().path())
            .collect();
        assert_eq!(files.len(), 8);
        // FakeSecrets scrambles: no file holds the raw samples.
        let raw: Vec<u8> = voice(180.0, 1.0)
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        for f in &files {
            let data = std::fs::read(f).unwrap();
            assert!(
                !data.windows(64).any(|w| w == &raw[..64]),
                "{f:?} is plain audio"
            );
        }
        assert!(matches!(
            id.check(&voice(180.0, 2.0)),
            Verdict::Owner { .. }
        ));
        assert!(matches!(
            id.check(&voice(620.0, 2.0)),
            Verdict::Stranger { .. }
        ));
        // Too little speech to judge.
        assert_eq!(id.check(&voice(180.0, 0.5)), Verdict::Unknown);
        id.delete().unwrap();
        assert!(!id.status().enrolled);
        assert_eq!(
            std::fs::read_dir(tmp.path().join("voice")).unwrap().count(),
            0
        );
        assert_eq!(id.check(&voice(180.0, 2.0)), Verdict::Unknown);
    }

    #[test]
    fn confident_long_matches_grow_the_profile_up_to_forty() {
        let tmp = tempfile::tempdir().unwrap();
        let id = with_model(&tmp.path().join("voice"));
        for p in 0..8 {
            id.keep_take(p, voice(180.0, 1.0));
        }
        assert_eq!(id.finish_enrollment().unwrap().embeddings, 1);
        for _ in 0..50 {
            let _ = id.check(&voice(180.0, 2.5));
        }
        assert_eq!(id.status().embeddings, MAX_EMBEDDINGS);
        // Short requests are judged but not learned from.
        let before = id.status().embeddings;
        let _ = id.check(&voice(180.0, 1.2));
        assert_eq!(id.status().embeddings, before);
    }

    #[test]
    fn the_threshold_follows_how_alike_the_owners_clips_are() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.9, 0.1, 0.0];
        let c = vec![0.8, 0.2, 0.1];
        let t = calibrate(&[a.clone(), b, c]);
        assert!((THRESHOLD_MIN..=THRESHOLD_MAX).contains(&t), "{t}");
        // Clips that don't match each other at all still get the floor, never below it.
        assert_eq!(calibrate(&[a, vec![0.0, 1.0, 0.0]]), THRESHOLD_MIN);
    }

    #[test]
    fn short_or_silent_takes_are_asked_again() {
        let tmp = tempfile::tempdir().unwrap();
        let id = voice_id(tmp.path());
        assert!(!id.keep_take(0, vec![0.1; 4_000]).ok, "a quarter second");
        assert!(id.keep_take(0, vec![0.1; 20_000]).ok);
        assert!(id.status().recorded[0]);
        id.cancel_enrollment();
        assert!(id.status().recorded.iter().all(|r| !r));
    }

    #[test]
    fn without_a_profile_nobody_is_told_apart() {
        let tmp = tempfile::tempdir().unwrap();
        let id = voice_id(tmp.path());
        assert_eq!(id.check(&vec![0.1; 32_000]), Verdict::Unknown);
        assert!(!id.status().enrolled);
    }

    #[test]
    fn enrollment_needs_enough_clips_and_the_model() {
        let tmp = tempfile::tempdir().unwrap();
        let id = voice_id(tmp.path());
        for p in 0..3 {
            id.keep_take(p, vec![0.1; 20_000]);
        }
        assert!(id.finish_enrollment().is_err(), "three clips aren't enough");
        for p in 3..8 {
            id.keep_take(p, vec![0.1; 20_000]);
        }
        // Enough clips, but the voice model isn't downloaded here.
        assert!(id.finish_enrollment().is_err());
    }

    /// With the real model (`KIVO_CAMPPLUS_DIR`) and a real recording (`KIVO_KWS_DIR`): the
    /// enrollment runs end to end on CAM++ and a check gives a score. (Telling voices apart is
    /// verified against sherpa-onnx in `kivo-voice`; these two LibriSpeech readers are too alike
    /// to judge the threshold by.)
    #[test]
    fn enrollment_runs_on_the_real_model() {
        let (Some(model), Some(kws)) = (
            std::env::var_os("KIVO_CAMPPLUS_DIR").map(PathBuf::from),
            std::env::var_os("KIVO_KWS_DIR").map(PathBuf::from),
        ) else {
            eprintln!("KIVO_CAMPPLUS_DIR / KIVO_KWS_DIR not set; skipping");
            return;
        };
        let data = std::fs::read(kws.join("test_wavs/1.wav")).unwrap();
        let owner: Vec<f32> = data[44..]
            .chunks_exact(2)
            .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32_768.0)
            .collect();
        let tmp = tempfile::tempdir().unwrap();
        let id = VoiceId::new(
            tmp.path().join("voice"),
            Arc::new(kivo_testkit::FakeSecrets::default()),
            Arc::new(Mutex::new(Database::in_memory().unwrap())),
            move || {
                kivo_voice::speaker::CamPlusPlus::load(&model)
                    .ok()
                    .map(|c| Arc::new(c) as Arc<dyn SpeakerVerifier>)
            },
        );
        for (p, chunk) in owner.chunks(owner.len() / 8).take(8).enumerate() {
            assert!(id.keep_take(p, chunk.to_vec()).ok);
        }
        let status = id.finish_enrollment().unwrap();
        assert!(status.enrolled && status.embeddings >= 1);
        let verdict = id.check(&owner[..48_000]);
        eprintln!("{verdict:?}");
        assert!(matches!(
            verdict,
            Verdict::Owner { .. } | Verdict::Stranger { .. }
        ));
    }
}
