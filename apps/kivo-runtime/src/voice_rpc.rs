//! The Control Center's voice requests (VOICE §4–6): wake words, enrollment of the owner's voice
//! and sound previews. Like every request they carry no authority beyond a click in the UI.

use crate::activity::Recorder;
use crate::core::Core;
use crate::engine::Engine;
use crate::models::Models;
use crate::switch::Switcher;
use crate::wake::Wake;
use kivo_core::config::{SoundCue, SoundSet};
use kivo_core::text;
use kivo_ipc::infer::InferSlot;
use kivo_ipc::protocol::{
    MeasuredItem, ProfileItem, RecommendationItem, SpeechChoices, SpeechEngineItem, VoiceItem,
};
use kivo_ipc::{RpcError, method};
use kivo_platform::Secrets;
use kivo_store::Database;
use kivo_store::models::{KEYWORD_SPOTTER, SPEAKER_MODEL};
use kivo_store::wake::{FalseAlarmTest, Quality as StoredQuality, WakeWord};
use kivo_voice::kws::{Keyword, KeywordSpotter, KwsModel};
use kivo_voice::recommend::Priority;
use kivo_voice::wakeword::{self, Assessment, Quality};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How long a single recording may take (the listener ends it on silence well before).
const RECORD_LIMIT: Duration = Duration::from_secs(20);
/// A sample must be at least this sure to count (with the listener at its most sensitive).
const SAMPLE_MIN_SCORE: f32 = 0.10;

/// Everyday speech for the false-alarm test: none of it says a wake word on purpose, all of it is
/// the kind of talk a wake word must sleep through.
const BACKGROUND: &[&str] = &[
    "I think we should leave a little earlier tomorrow, the traffic was terrible this morning.",
    "Could you pass me the salt? Thanks. This soup needs a bit more flavour.",
    "The meeting has moved to three o'clock, and they want the slides before lunch.",
    "My sister is coming over on Saturday with the kids, so we'll need the spare room.",
    "Did you see the game last night? They were two goals down and still won.",
    "The printer is out of paper again. There's a new pack in the cupboard by the door.",
    "Let's order something in tonight. Pizza, noodles, or that curry place you liked?",
    "I've been reading a book about the history of maps, it's surprisingly good.",
    "Remember to water the plants while I'm away. The big one needs it every two days.",
    "The weather says rain in the afternoon, so take an umbrella just in case.",
    "We could paint the kitchen a light green. What do you think about the colour?",
    "He called to say the parcel will arrive between nine and twelve on Monday.",
    "This recipe says to bake it for forty minutes, but my oven runs a little hot.",
    "I keep forgetting my password for that site. I should write it down somewhere safe.",
    "The train was cancelled, so I took the bus and got home an hour late.",
    "Can we talk about the budget for the trip? Flights have gone up quite a lot.",
    "Every Tuesday there's a quiz at the pub on the corner. We came third last week.",
    "She's learning the piano now, and she practises every evening after dinner.",
    "The garage called. The car is ready, and it wasn't as expensive as we feared.",
    "I'll make coffee. Do you want milk in yours, or are you still off dairy?",
    "The new neighbours seem nice. They brought over a cake when they moved in.",
    "Could you turn the music down a little? I'm trying to finish this report.",
    "We need eggs, bread, some apples and washing-up liquid from the shop.",
    "The film was far too long, but the ending almost made up for it.",
];

pub struct VoiceRpc {
    core: Arc<Core>,
    engine: Arc<Engine>,
    models: Arc<Models>,
    db: Arc<Mutex<Database>>,
    wake: Arc<Wake>,
    recorder: Recorder,
    secrets: Arc<dyn Secrets>,
    voice_dir: PathBuf,
    switcher: Arc<Switcher>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RpcError> {
    serde_json::from_value(params).map_err(RpcError::invalid_params)
}

fn ok<T: Serialize>(value: &T) -> Result<Value, RpcError> {
    serde_json::to_value(value).map_err(|e| RpcError::new(RpcError::INTERNAL, e.to_string()))
}

/// A unit enum's JSON name ("highAccuracy").
fn json_name(value: &impl Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn refuse(message: impl Into<String>) -> RpcError {
    RpcError::new(RpcError::REFUSED, message.into())
}

fn stored(quality: Quality) -> StoredQuality {
    match quality {
        Quality::Good => StoredQuality::Good,
        Quality::Fair => StoredQuality::Fair,
        Quality::Risky => StoredQuality::Risky,
    }
}

/// The spotter's best score for `phrase` in `audio`, listening as sensitively as it can.
fn best_score(model_dir: &Path, phrase: &str, audio: &[f32]) -> Result<Option<f32>, String> {
    let model = KwsModel::load(model_dir).map_err(|e| e.detail())?;
    let keyword = Keyword::new("try", phrase).with_sensitivity(1.0);
    let (mut spotter, refused) = KeywordSpotter::new(model, vec![keyword]);
    if !refused.is_empty() {
        return Ok(None);
    }
    let mut best: Option<f32> = None;
    for chunk in audio
        .chunks(1_280)
        .chain(std::iter::once(&[0.0; 16_000][..]))
    {
        for hit in spotter.accept(chunk).map_err(|e| e.detail())? {
            best = Some(best.map_or(hit.score, |b: f32| b.max(hit.score)));
        }
    }
    Ok(best)
}

/// The threshold a sensitivity gives (as the listener uses it).
fn threshold(sensitivity: f32) -> f32 {
    Keyword::new("", "").with_sensitivity(sensitivity).threshold
}

/// The sensitivity whose threshold sits just under the weakest sample, so every sample would
/// have woken KIVO (VOICE-16 "tune").
pub fn tuned_sensitivity(scores: &[f32]) -> f32 {
    let weakest = scores.iter().copied().fold(f32::INFINITY, f32::min);
    if !weakest.is_finite() {
        return 0.5;
    }
    // threshold = 0.45 − 0.35 · s  ⇒  s = (0.45 − threshold) / 0.35
    let wanted = (weakest - 0.05).clamp(0.10, 0.45);
    ((0.45 - wanted) / 0.35).clamp(0.2, 0.9)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WakeList {
    words: Vec<WakeWord>,
    /// The keyword model is on this PC.
    model_installed: bool,
    /// "Microphone listening" is on.
    listening: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Tried {
    heard: bool,
    score: Option<f32>,
    seconds: f32,
}

impl VoiceRpc {
    #[allow(clippy::too_many_arguments, reason = "the parts it talks to")]
    pub fn new(
        core: Arc<Core>,
        engine: Arc<Engine>,
        models: Arc<Models>,
        db: Arc<Mutex<Database>>,
        wake: Arc<Wake>,
        recorder: Recorder,
        secrets: Arc<dyn Secrets>,
        voice_dir: PathBuf,
    ) -> Self {
        let switcher = Arc::new(Switcher::new(
            Arc::clone(&core),
            Arc::clone(&engine),
            Arc::clone(&models),
        ));
        Self {
            core,
            engine,
            models,
            db,
            wake,
            recorder,
            secrets,
            voice_dir,
            switcher,
        }
    }

    /// The registry, profiles and current choice for the speech choosers (VOICE-42/43/48).
    fn speech_choices(&self) -> SpeechChoices {
        let config = self.core.config();
        let language = config.general.language.clone();
        let ready = self.models.ready_engines();

        // The Windows voices are whatever this PC has installed.
        let windows_voices: Vec<VoiceItem> = self
            .engine
            .system_voice()
            .and_then(|synth| synth.voices().ok())
            .unwrap_or_default()
            .into_iter()
            .map(|v| VoiceItem {
                id: v.id,
                name: v.name,
                style: String::new(),
                languages: vec![v.language],
            })
            .collect();
        let engines = self
            .models
            .registry()
            .into_iter()
            .map(|e| SpeechEngineItem {
                slot: json_name(&e.engine.slot),
                profiles: e.profiles.iter().map(json_name).collect(),
                privacy: json_name(&e.privacy),
                commercial_use: e.commercial_use,
                streaming: e.engine.streaming,
                devices: e.engine.accel.iter().map(json_name).collect(),
                download_mb: e.engine.resources.disk_mb,
                ram_mb: e.engine.resources.ram_mb,
                ready: ready.contains(&e.engine.id),
                fits_language: e.engine.supports(&language),
                voices: if e.engine.id == kivo_voice::system_tts::ENGINE_ID {
                    windows_voices.clone()
                } else {
                    e.voices
                        .into_iter()
                        .map(|v| VoiceItem {
                            id: v.id,
                            name: v.name,
                            style: v.style,
                            languages: v.languages,
                        })
                        .collect()
                },
                measured: e.measured.map(|m| MeasuredItem {
                    real_time_factor: m.real_time_factor,
                    latency_ms: m.latency_ms,
                    word_error_rate: m.word_error_rate,
                    measured_at: m.measured_at,
                }),
                model: e.engine.model,
                license: e.engine.license,
                languages: e.engine.languages,
                name: e.engine.name,
                id: e.engine.id,
            })
            .collect();
        let profiles = [kivo_voice::EngineSlot::Stt, kivo_voice::EngineSlot::Tts]
            .into_iter()
            .flat_map(|slot| kivo_voice::registry::profiles(slot, &language))
            .map(|c| ProfileItem {
                slot: c.slot,
                profile: json_name(&c.profile),
                engine: c.engine,
                other_languages_only: c.other_languages_only,
            })
            .collect();
        SpeechChoices {
            stt: self.models.listening_engine(&config),
            tts: config.voice.tts_engine.clone(),
            tts_voice: config.voice.tts_voice.clone(),
            language,
            engines,
            profiles,
        }
    }

    fn model_dir(&self) -> Result<PathBuf, RpcError> {
        self.models
            .installed_dir(KEYWORD_SPOTTER)
            .ok_or_else(|| refuse(text::t("wake.noModel")))
    }

    fn word(&self, id: &str) -> Result<WakeWord, RpcError> {
        lock(&self.db)
            .wake_word(id)
            .map_err(|e| refuse(e.to_string()))?
            .ok_or_else(|| refuse(text::t("wake.unknown")))
    }

    fn check(&self, phrase: &str, except: Option<&str>) -> Assessment {
        let others: Vec<String> = lock(&self.db)
            .wake_words()
            .unwrap_or_default()
            .into_iter()
            .filter(|w| w.enabled && Some(w.id.as_str()) != except)
            .map(|w| w.spoken().to_owned())
            .collect();
        let commands = kivo_intent::Grammar::bundled(&self.core.config().general.language)
            .map(|g| g.phrases())
            .unwrap_or_default();
        let spellable = self
            .models
            .installed_dir(KEYWORD_SPOTTER)
            .and_then(|dir| KwsModel::load(&dir).ok())
            .map(|m| m.tokens(phrase).is_some());
        wakeword::assess(phrase, &others, &commands, spellable)
    }

    /// Records one clip through the listener (it ends on silence).
    async fn record(&self) -> Result<Vec<f32>, RpcError> {
        let voice_id = self
            .engine
            .voice_id()
            .ok_or_else(|| refuse(text::t("voiceId.busy")))?;
        let listener = self
            .engine
            .listener_handle()
            .ok_or_else(|| refuse(text::t("voiceId.busy")))?;
        let id = self.engine.infer.next_utterance();
        let clip = voice_id.expect(id);
        listener.record(id);
        let audio = tokio::time::timeout(RECORD_LIMIT, clip)
            .await
            .map_err(|_| refuse(text::t("voiceId.notHeard")))?
            .map_err(|_| refuse(text::t("voiceId.notHeard")))?;
        if audio.is_empty() {
            return Err(refuse(text::t("voiceId.notHeard")));
        }
        Ok(audio)
    }

    async fn spot(&self, phrase: &str, audio: Vec<f32>) -> Result<Option<f32>, RpcError> {
        let dir = self.model_dir()?;
        let phrase = phrase.to_owned();
        tokio::task::spawn_blocking(move || best_score(&dir, &phrase, &audio))
            .await
            .map_err(|e| refuse(e.to_string()))?
            .map_err(refuse)
    }

    fn save(&self, word: &WakeWord) -> Result<(), RpcError> {
        lock(&self.db)
            .save_wake_word(word)
            .map_err(|e| refuse(e.to_string()))?;
        self.wake.words_changed();
        Ok(())
    }

    fn sample_path(&self, file: &str) -> PathBuf {
        self.voice_dir.join(file)
    }

    fn load_sample(&self, file: &str) -> Option<Vec<f32>> {
        let sealed = std::fs::read(self.sample_path(file)).ok()?;
        let plain = self.secrets.unprotect(&sealed).ok()?;
        Some(
            plain
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect(),
        )
    }

    /// Background speech for the false-alarm test, from Windows' voices (up to two of them).
    fn background_speech(&self) -> Vec<f32> {
        let Some(synth) = self.engine.system_voice() else {
            return Vec::new();
        };
        let voices: Vec<Option<String>> = synth
            .voices()
            .map(|v| v.into_iter().take(2).map(|v| Some(v.id)).collect())
            .unwrap_or_default();
        let voices = if voices.is_empty() {
            vec![None]
        } else {
            voices
        };
        let mut audio = Vec::new();
        for voice in &voices {
            for sentence in BACKGROUND {
                if let Ok(spoken) = synth.synthesize(sentence, voice.as_deref()) {
                    audio.extend(kivo_audio::RateConverter::convert_all(
                        spoken.rate,
                        16_000,
                        &spoken.samples,
                    ));
                    audio.extend(std::iter::repeat_n(0.0, 4_000));
                }
            }
        }
        audio
    }

    /// Handles a voice request, or returns `None` if `name` isn't one.
    #[allow(clippy::too_many_lines, reason = "one table of the voice requests")]
    pub async fn call(&self, name: &str, params: Value) -> Option<Result<Value, RpcError>> {
        let result = match name {
            method::VOICE_ENGINES => ok(&self.speech_choices()),
            method::VOICE_RECOMMEND => {
                #[derive(Deserialize, Default)]
                #[serde(deny_unknown_fields)]
                struct Params {
                    #[serde(default)]
                    priority: Priority,
                }
                let p: Params = if params.is_null() {
                    Params::default()
                } else {
                    match parse(params) {
                        Ok(p) => p,
                        Err(e) => return Some(Err(e)),
                    }
                };
                self.models
                    .recommend_now(&self.core.config(), p.priority)
                    .ok_or_else(|| refuse(text::t("voice.noHardware")))
                    .and_then(|r| {
                        ok(&RecommendationItem {
                            tier: json_name(&r.tier),
                            stt_engine: r.stt_engine,
                            stt_fallback: r.stt_fallback,
                            tts_engine: r.tts_engine,
                            tts_fallback: r.tts_fallback,
                            threads: u32::try_from(r.threads).unwrap_or(1),
                            reason: r.reason,
                        })
                    })
            }
            method::VOICE_SWITCH => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Params {
                    slot: InferSlot,
                    engine: String,
                    #[serde(default)]
                    voice: Option<String>,
                }
                match parse::<Params>(params) {
                    Ok(p) => self
                        .switcher
                        .start(p.slot, &p.engine, p.voice)
                        .map(|()| Value::Null)
                        .map_err(refuse),
                    Err(e) => Err(e),
                }
            }
            method::VOICE_PREVIEW => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Params {
                    engine: String,
                    #[serde(default)]
                    voice: Option<String>,
                }
                match parse::<Params>(params) {
                    Ok(p) => self
                        .switcher
                        .preview(&p.engine, p.voice.as_deref())
                        .await
                        .map(|()| Value::Null)
                        .map_err(refuse),
                    Err(e) => Err(e),
                }
            }
            method::VOICE_MIC_CHECK => {
                #[derive(Serialize)]
                #[serde(rename_all = "camelCase")]
                struct Checked {
                    heard: bool,
                    /// The loudest moment, in dB below full scale.
                    peak_db: f32,
                    seconds: f32,
                }
                self.record().await.and_then(|audio| {
                    let peak = audio.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
                    #[allow(clippy::cast_precision_loss, reason = "seconds of audio")]
                    let seconds = audio.len() as f32 / 16_000.0;
                    let peak_db = 20.0 * peak.max(1e-6).log10();
                    // Speech was found (the recording keeps only speech) and it is loud enough.
                    ok(&Checked {
                        heard: seconds >= 0.3 && peak_db > -40.0,
                        peak_db,
                        seconds,
                    })
                })
            }
            method::VOICE_TRY_SAMPLE => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Params {
                    engine: String,
                }
                match parse::<Params>(params) {
                    Ok(p) => self
                        .switcher
                        .try_sample(&p.engine)
                        .await
                        .map(|(said, heard)| serde_json::json!({ "said": said, "heard": heard }))
                        .map_err(refuse),
                    Err(e) => Err(e),
                }
            }
            method::WAKE_LIST => {
                let words = lock(&self.db)
                    .wake_words()
                    .map_err(|e| refuse(e.to_string()));
                words.and_then(|words| {
                    ok(&WakeList {
                        words,
                        model_installed: self.models.installed_dir(KEYWORD_SPOTTER).is_some(),
                        listening: self
                            .core
                            .config()
                            .capabilities
                            .enabled(kivo_core::Capability::MicListening),
                    })
                })
            }
            method::WAKE_CHECK => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Params {
                    phrase: String,
                    #[serde(default)]
                    id: Option<String>,
                }
                match parse::<Params>(params) {
                    Ok(p) => ok(&self.check(&p.phrase, p.id.as_deref())),
                    Err(e) => Err(e),
                }
            }
            method::WAKE_SAVE => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase", deny_unknown_fields)]
                struct Params {
                    #[serde(default)]
                    id: Option<String>,
                    phrase: String,
                    #[serde(default)]
                    phonetic: Option<String>,
                    #[serde(default)]
                    sensitivity: Option<f32>,
                    #[serde(default)]
                    enabled: Option<bool>,
                }
                let p: Params = match parse(params) {
                    Ok(p) => p,
                    Err(e) => return Some(Err(e)),
                };
                let existing =
                    p.id.as_deref()
                        .and_then(|id| lock(&self.db).wake_word(id).ok().flatten());
                let spoken = p
                    .phonetic
                    .clone()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| p.phrase.clone());
                let assessment = self.check(&spoken, p.id.as_deref());
                let is_new = existing.is_none();
                let mut word = existing.unwrap_or_else(|| WakeWord {
                    id: format!("word-{}", uuid::Uuid::now_v7().simple()),
                    phrase: String::new(),
                    phonetic: None,
                    enabled: true,
                    built_in: false,
                    sensitivity: 0.5,
                    samples: Vec::new(),
                    quality: StoredQuality::Good,
                    false_alarm_test: None,
                    created_at: 0,
                });
                if !word.built_in {
                    p.phrase.trim().clone_into(&mut word.phrase);
                    word.quality = stored(assessment.quality);
                }
                word.phonetic = p.phonetic.filter(|s| !s.trim().is_empty());
                if let Some(s) = p.sensitivity {
                    word.sensitivity = s.clamp(0.0, 1.0);
                }
                if let Some(e) = p.enabled {
                    word.enabled = e;
                }
                self.save(&word).and_then(|()| {
                    if is_new {
                        self.recorder.user_action(
                            "wake.save",
                            &text::tf("wake.added", &[("phrase", &word.phrase)]),
                        );
                    }
                    ok(&word)
                })
            }
            method::WAKE_DELETE => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Params {
                    id: String,
                }
                let p: Params = match parse(params) {
                    Ok(p) => p,
                    Err(e) => return Some(Err(e)),
                };
                let phrase = self.word(&p.id).map(|w| w.phrase).unwrap_or_default();
                let deleted = lock(&self.db).delete_wake_word(&p.id);
                match deleted {
                    Ok(files) => {
                        for file in files {
                            let _ = std::fs::remove_file(self.sample_path(&file));
                        }
                        self.wake.words_changed();
                        self.recorder.user_action(
                            "wake.delete",
                            &text::tf("wake.deleted", &[("phrase", &phrase)]),
                        );
                        Ok(Value::Null)
                    }
                    Err(e) => Err(refuse(e.to_string())),
                }
            }
            method::WAKE_SET => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Params {
                    id: String,
                    #[serde(default)]
                    enabled: Option<bool>,
                    #[serde(default)]
                    sensitivity: Option<f32>,
                }
                let p: Params = match parse(params) {
                    Ok(p) => p,
                    Err(e) => return Some(Err(e)),
                };
                let db = lock(&self.db);
                let result = p
                    .enabled
                    .map_or(Ok(()), |on| db.set_wake_word_enabled(&p.id, on))
                    .and_then(|()| {
                        p.sensitivity
                            .map_or(Ok(()), |s| db.set_wake_word_sensitivity(&p.id, s))
                    });
                drop(db);
                self.wake.words_changed();
                result
                    .map(|()| Value::Null)
                    .map_err(|e| refuse(e.to_string()))
            }
            method::WAKE_HEAR => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Params {
                    phrase: String,
                }
                match parse::<Params>(params) {
                    Ok(p) => self
                        .engine
                        .preview(&p.phrase, None)
                        .await
                        .map(|()| Value::Null)
                        .map_err(refuse),
                    Err(e) => Err(e),
                }
            }
            method::WAKE_TRY => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Params {
                    phrase: String,
                    #[serde(default = "half")]
                    sensitivity: f32,
                }
                let p: Params = match parse(params) {
                    Ok(p) => p,
                    Err(e) => return Some(Err(e)),
                };
                match self.record().await {
                    Ok(audio) => {
                        #[allow(clippy::cast_precision_loss, reason = "seconds")]
                        let seconds = audio.len() as f32 / 16_000.0;
                        self.spot(&p.phrase, audio).await.and_then(|score| {
                            ok(&Tried {
                                heard: score.is_some_and(|s| s >= threshold(p.sensitivity)),
                                score,
                                seconds,
                            })
                        })
                    }
                    Err(e) => Err(e),
                }
            }
            method::WAKE_SAMPLE => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Params {
                    id: String,
                }
                let p: Params = match parse(params) {
                    Ok(p) => p,
                    Err(e) => return Some(Err(e)),
                };
                let mut word = match self.word(&p.id) {
                    Ok(w) => w,
                    Err(e) => return Some(Err(e)),
                };
                let audio = match self.record().await {
                    Ok(a) => a,
                    Err(e) => return Some(Err(e)),
                };
                let score = match self.spot(word.spoken(), audio.clone()).await {
                    Ok(s) => s,
                    Err(e) => return Some(Err(e)),
                };
                #[allow(clippy::cast_precision_loss, reason = "seconds")]
                let seconds = audio.len() as f32 / 16_000.0;
                if score.is_some_and(|s| s >= SAMPLE_MIN_SCORE) {
                    let file = format!("wake-{}-{}.bin", word.id, word.samples.len() + 1);
                    let bytes: Vec<u8> = audio.iter().flat_map(|s| s.to_le_bytes()).collect();
                    let written = self
                        .secrets
                        .protect(&bytes)
                        .map_err(|e| refuse(e.to_string()))
                        .and_then(|sealed| {
                            std::fs::create_dir_all(&self.voice_dir)
                                .and_then(|()| std::fs::write(self.sample_path(&file), sealed))
                                .map_err(|e| refuse(e.to_string()))
                        });
                    if let Err(e) = written {
                        return Some(Err(e));
                    }
                    word.samples.push(file);
                    if let Err(e) = self.save(&word) {
                        return Some(Err(e));
                    }
                }
                ok(&Tried {
                    heard: score.is_some_and(|s| s >= SAMPLE_MIN_SCORE),
                    score,
                    seconds,
                })
            }
            method::WAKE_TUNE => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Params {
                    id: String,
                }
                let p: Params = match parse(params) {
                    Ok(p) => p,
                    Err(e) => return Some(Err(e)),
                };
                let mut word = match self.word(&p.id) {
                    Ok(w) => w,
                    Err(e) => return Some(Err(e)),
                };
                if word.samples.is_empty() {
                    return Some(Err(refuse(text::t("wake.noSamples"))));
                }
                let mut scores = Vec::new();
                for file in word.samples.clone() {
                    if let Some(audio) = self.load_sample(&file)
                        && let Ok(Some(score)) = self.spot(word.spoken(), audio).await
                    {
                        scores.push(score);
                    }
                }
                word.sensitivity = tuned_sensitivity(&scores);
                self.save(&word).and_then(|()| ok(&word))
            }
            method::WAKE_FALSE_ALARMS => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Params {
                    id: String,
                }
                let p: Params = match parse(params) {
                    Ok(p) => p,
                    Err(e) => return Some(Err(e)),
                };
                let mut word = match self.word(&p.id) {
                    Ok(w) => w,
                    Err(e) => return Some(Err(e)),
                };
                let dir = match self.model_dir() {
                    Ok(d) => d,
                    Err(e) => return Some(Err(e)),
                };
                let speech = self.background_speech();
                let phrase = word.spoken().to_owned();
                let sensitivity = word.sensitivity;
                let counted = tokio::task::spawn_blocking(move || -> Result<(u32, f32), String> {
                    let model = KwsModel::load(&dir).map_err(|e| e.detail())?;
                    let keyword = Keyword::new("test", &phrase).with_sensitivity(sensitivity);
                    let (mut spotter, _) = KeywordSpotter::new(model, vec![keyword]);
                    let mut hits = 0_u32;
                    for chunk in speech.chunks(1_280) {
                        hits += u32::try_from(spotter.accept(chunk).map_err(|e| e.detail())?.len())
                            .unwrap_or(0);
                    }
                    #[allow(clippy::cast_precision_loss, reason = "minutes of audio")]
                    let minutes = speech.len() as f32 / 16_000.0 / 60.0;
                    Ok((hits, minutes))
                })
                .await;
                match counted {
                    Ok(Ok((false_alarms, minutes))) => {
                        word.false_alarm_test = Some(FalseAlarmTest {
                            minutes,
                            false_alarms,
                        });
                        self.save(&word).and_then(|()| ok(&word.false_alarm_test))
                    }
                    Ok(Err(e)) => Err(refuse(e)),
                    Err(e) => Err(refuse(e.to_string())),
                }
            }
            method::VOICE_ID_STATUS => match self.engine.voice_id() {
                Some(v) => ok(&v.status()),
                None => Err(refuse(text::t("voiceId.busy"))),
            },
            method::VOICE_ID_START => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Params {
                    consent: bool,
                }
                let p: Params = match parse(params) {
                    Ok(p) => p,
                    Err(e) => return Some(Err(e)),
                };
                if !p.consent {
                    return Some(Err(RpcError::invalid_params("consent is required")));
                }
                let Some(voice_id) = self.engine.voice_id() else {
                    return Some(Err(refuse(text::t("voiceId.busy"))));
                };
                voice_id.cancel_enrollment();
                // The user chose to enroll: the voice model downloads now if it isn't here.
                if self.models.installed_dir(SPEAKER_MODEL).is_none()
                    && let Err(e) = self.models.install(SPEAKER_MODEL)
                {
                    return Some(Err(refuse(e)));
                }
                self.recorder
                    .user_action("voiceId.start", &text::t("voiceId.consent"));
                ok(&serde_json::json!({ "prompts": crate::voiceid::PROMPTS }))
            }
            method::VOICE_ID_RECORD => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Params {
                    prompt: usize,
                }
                let p: Params = match parse(params) {
                    Ok(p) => p,
                    Err(e) => return Some(Err(e)),
                };
                let Some(voice_id) = self.engine.voice_id() else {
                    return Some(Err(refuse(text::t("voiceId.busy"))));
                };
                match self.record().await {
                    Ok(audio) => ok(&voice_id.keep_take(p.prompt, audio)),
                    Err(e) => Err(e),
                }
            }
            method::VOICE_ID_FINISH => {
                let Some(voice_id) = self.engine.voice_id() else {
                    return Some(Err(refuse(text::t("voiceId.busy"))));
                };
                let finished = tokio::task::spawn_blocking(move || voice_id.finish_enrollment())
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r);
                match finished {
                    Ok(status) => {
                        // Enrolled: recognition turns on, preferring the owner (VOICE §5).
                        self.core.update_config(|c| {
                            c.capabilities
                                .set(kivo_core::Capability::SpeakerRecognition, true);
                            if c.voice.speaker_mode == kivo_core::config::SpeakerMode::Off {
                                c.voice.speaker_mode = kivo_core::config::SpeakerMode::PreferOwner;
                            }
                        });
                        self.recorder
                            .user_action("voiceId.finish", &text::t("voiceId.saved"));
                        ok(&status)
                    }
                    Err(e) => Err(refuse(e)),
                }
            }
            method::VOICE_ID_CANCEL => {
                if let Some(v) = self.engine.voice_id() {
                    v.cancel_enrollment();
                }
                Ok(Value::Null)
            }
            method::VOICE_ID_DELETE => {
                let Some(voice_id) = self.engine.voice_id() else {
                    return Some(Err(refuse(text::t("voiceId.busy"))));
                };
                match voice_id.delete() {
                    Ok(()) => {
                        voice_id.cancel_enrollment();
                        self.core.update_config(|c| {
                            c.capabilities
                                .set(kivo_core::Capability::SpeakerRecognition, false);
                            c.voice.speaker_mode = kivo_core::config::SpeakerMode::Off;
                        });
                        self.recorder
                            .user_action("voiceId.delete", &text::t("voiceId.deleted"));
                        ok(&voice_id.status())
                    }
                    Err(e) => Err(refuse(e)),
                }
            }
            method::SOUNDS_PREVIEW => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Params {
                    set: SoundSet,
                    #[serde(default)]
                    cue: Option<SoundCue>,
                }
                let p: Params = match parse(params) {
                    Ok(p) => p,
                    Err(e) => return Some(Err(e)),
                };
                let speaker = Arc::clone(&self.engine.speaker);
                match p.cue {
                    Some(cue) => speaker.preview(p.set, cue),
                    // The whole set: the cues people hear most, one after another.
                    None => {
                        tokio::spawn(async move {
                            for cue in [SoundCue::ListenStart, SoundCue::Done, SoundCue::Question] {
                                speaker.preview(p.set, cue);
                                tokio::time::sleep(Duration::from_millis(450)).await;
                            }
                        });
                    }
                }
                Ok(Value::Null)
            }
            _ => return None,
        };
        Some(result)
    }
}

fn half() -> f32 {
    0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tuning_puts_the_threshold_just_under_the_weakest_sample() {
        let s = tuned_sensitivity(&[0.6, 0.35, 0.5]);
        let t = threshold(s);
        assert!(
            (0.25..=0.35 - 0.04).contains(&t),
            "sensitivity {s} → threshold {t}"
        );
        // Very strong samples keep a sensible, not-too-eager setting.
        assert!((tuned_sensitivity(&[0.95, 0.9]) - 0.2).abs() < 1e-6);
        // Very weak samples can't push it past the top.
        assert!((tuned_sensitivity(&[0.05]) - 0.9).abs() < 1e-6);
        assert!((tuned_sensitivity(&[]) - 0.5).abs() < 1e-6);
    }
}
