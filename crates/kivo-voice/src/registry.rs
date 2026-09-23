//! The speech engine registry (VOICE §11, VOICE-42/43): every STT and TTS engine KIVO can use,
//! with what the choosers show about it, and the 2–4 curated profiles per slot that users pick
//! from. The Voice page and onboarding read only this; adding an engine is an entry here plus an
//! adapter in `kivo-infer`.
//!
//! Scores come only from KIVO's own measurements (`Measured`, from `kivo-bench` on this PC);
//! anything unmeasured is shown as "Not benchmarked by KIVO".

use crate::engine::{EngineInfo, EngineKind, EngineSlot};
use crate::{kokoro, moonshine, supertonic, system_tts};
use serde::{Deserialize, Serialize};

/// A curated choice (VOICE §11). STT uses Recommended, Lightweight, HighAccuracy and
/// Multilingual; TTS uses Natural, Lightweight, Multilingual and Expressive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Profile {
    Recommended,
    Lightweight,
    HighAccuracy,
    Multilingual,
    Natural,
    Expressive,
}

impl Profile {
    pub const STT: [Profile; 4] = [
        Profile::Recommended,
        Profile::Lightweight,
        Profile::HighAccuracy,
        Profile::Multilingual,
    ];
    pub const TTS: [Profile; 4] = [
        Profile::Natural,
        Profile::Lightweight,
        Profile::Multilingual,
        Profile::Expressive,
    ];
}

/// Where the audio or text goes (VOICE §11 privacy labels), from the engine's architecture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Privacy {
    /// Audio and text stay on this PC.
    Local,
    /// Audio or text goes to the provider.
    Cloud,
    /// Some of the work happens on the provider's side.
    Hybrid,
}

/// A voice of a TTS engine; the engine and the voice are separate choices (VOICE §11).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceOption {
    pub id: String,
    pub name: String,
    /// "female", "male", or empty when the publisher doesn't say.
    pub style: String,
    /// BCP-47 primary tags; `*` for every language the engine speaks.
    pub languages: Vec<String>,
}

/// KIVO's own measurement of an engine on this PC (`kivo-bench stt`/`tts`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Measured {
    /// Processing time over audio time; below 1 is faster than real time.
    pub real_time_factor: Option<f64>,
    /// End of speech → final transcript (STT) or text → first audio (TTS), median, ms.
    pub latency_ms: Option<f64>,
    /// Word error rate on the benchmark set, 0–1 (STT).
    pub word_error_rate: Option<f64>,
    /// Unix milliseconds of the run.
    pub measured_at: i64,
}

/// One engine in the registry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryEntry {
    pub engine: EngineInfo,
    pub profiles: Vec<Profile>,
    pub privacy: Privacy,
    /// False when the licence forbids commercial use (shown before the download).
    pub commercial_use: bool,
    /// TTS voices; the Windows voices are listed by the runtime from the system instead.
    pub voices: Vec<VoiceOption>,
    /// On this PC, from KIVO's benchmarks; `None` reads "Not benchmarked by KIVO".
    pub measured: Option<Measured>,
    /// The name `kivo-bench` reports this engine under, when it measures it.
    #[serde(skip)]
    pub bench_name: Option<&'static str>,
}

impl RegistryEntry {
    fn new(engine: EngineInfo, profiles: &[Profile]) -> Self {
        let privacy = match engine.kind {
            EngineKind::Local | EngineKind::System => Privacy::Local,
            EngineKind::Cloud => Privacy::Cloud,
        };
        Self {
            engine,
            profiles: profiles.to_vec(),
            privacy,
            commercial_use: true,
            voices: Vec::new(),
            measured: None,
            bench_name: None,
        }
    }

    pub fn id(&self) -> &str {
        &self.engine.id
    }

    pub fn slot(&self) -> EngineSlot {
        self.engine.slot
    }

    pub fn has(&self, profile: Profile) -> bool {
        self.profiles.contains(&profile)
    }
}

/// Every STT and TTS engine KIVO can use.
pub fn registry() -> Vec<RegistryEntry> {
    let mut all: Vec<RegistryEntry> = moonshine::VARIANTS
        .iter()
        .map(|v| {
            let profiles: &[Profile] = match (v.language, v.tiny) {
                ("en", false) => &[Profile::Recommended],
                ("en", true) => &[Profile::Lightweight],
                _ => &[Profile::Multilingual],
            };
            let mut entry = RegistryEntry::new(moonshine::info_for(v), profiles);
            entry.commercial_use = v.commercial;
            entry.bench_name = (v.id == moonshine::MODEL_ID).then_some("moonshine");
            entry
        })
        .collect();

    all.push(RegistryEntry::new(
        system_tts::info(),
        &[Profile::Lightweight],
    ));

    let mut kokoro = RegistryEntry::new(kokoro::info(), &[Profile::Natural]);
    kokoro.voices = kokoro::VOICES
        .iter()
        .map(|id| VoiceOption {
            id: (*id).to_owned(),
            name: kokoro::voice_name(id),
            style: style_of(id.chars().nth(1)),
            languages: vec!["en".into()],
        })
        .collect();
    all.push(kokoro);

    let mut supertonic = RegistryEntry::new(supertonic::info(), &[Profile::Multilingual]);
    supertonic.voices = supertonic::VOICES
        .iter()
        .map(|id| VoiceOption {
            id: (*id).to_owned(),
            name: supertonic::voice_name(id),
            style: style_of(id.chars().next().map(|c| c.to_ascii_lowercase())),
            languages: vec!["*".into()],
        })
        .collect();
    all.push(supertonic);
    all
}

fn style_of(letter: Option<char>) -> String {
    match letter {
        Some('f') => "female",
        Some('m') => "male",
        _ => "",
    }
    .into()
}

/// The entry for engine `id`.
pub fn entry(id: &str) -> Option<RegistryEntry> {
    registry().into_iter().find(|e| e.id() == id)
}

/// A profile card: what the chooser shows for one profile, in one language.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileCard {
    pub slot: String,
    pub profile: Profile,
    /// The engine behind it for this language; `None` when KIVO has none yet ("Not available
    /// yet") or none that speaks the language.
    pub engine: Option<String>,
    /// True when the profile has engines, but none for this language (VOICE-48).
    pub other_languages_only: bool,
}

/// The profiles of `slot` for `language`, each mapped to its engine (VOICE-43).
pub fn profiles(slot: EngineSlot, language: &str) -> Vec<ProfileCard> {
    let all = registry();
    let list: &[Profile] = match slot {
        EngineSlot::Stt => &Profile::STT,
        EngineSlot::Tts => &Profile::TTS,
        _ => &[],
    };
    list.iter()
        .map(|&profile| {
            let with: Vec<&RegistryEntry> = all
                .iter()
                .filter(|e| e.slot() == slot && e.has(profile))
                .collect();
            let fits = |e: &&&RegistryEntry| e.engine.supports(language);
            // Base before Tiny when several fit (Multilingual has both for Japanese).
            let engine = with
                .iter()
                .filter(fits)
                .find(|e| !e.engine.id.contains("-tiny-"))
                .or_else(|| with.iter().find(fits))
                .map(|e| e.engine.id.clone());
            ProfileCard {
                slot: if slot == EngineSlot::Stt {
                    "stt"
                } else {
                    "tts"
                }
                .into(),
                profile,
                other_languages_only: engine.is_none() && !with.is_empty(),
                engine,
            }
        })
        .collect()
}

/// The engines of `slot` that handle `language`, for explaining an incompatible choice with
/// compatible alternatives (VOICE-48).
pub fn compatible(slot: EngineSlot, language: &str) -> Vec<RegistryEntry> {
    registry()
        .into_iter()
        .filter(|e| e.slot() == slot && e.engine.supports(language))
        .collect()
}

/// An engine that doesn't handle the chosen language (VOICE-48).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageProblem {
    /// The engine's display name.
    pub engine: String,
    /// The language's English name ("Spanish"), or its code when KIVO doesn't know it.
    pub language: String,
    /// Engines of the same slot that do handle it, by display name.
    pub alternatives: Vec<String>,
}

/// Why `engine` can't serve `language`, or `None` when it can (VOICE-48). It names the engines
/// that can, and nothing is switched.
pub fn language_problem(engine: &str, language: &str) -> Option<LanguageProblem> {
    let entry = entry(engine)?;
    if entry.engine.supports(language) {
        return None;
    }
    Some(LanguageProblem {
        alternatives: compatible(entry.slot(), language)
            .into_iter()
            .map(|e| e.engine.name)
            .collect(),
        engine: entry.engine.name,
        language: language_name(language),
    })
}

/// The English name of a language KIVO's engines cover ("es" → "Spanish").
pub fn language_name(language: &str) -> String {
    const NAMES: [(&str, &str); 33] = [
        ("en", "English"),
        ("ar", "Arabic"),
        ("bg", "Bulgarian"),
        ("cs", "Czech"),
        ("da", "Danish"),
        ("de", "German"),
        ("el", "Greek"),
        ("es", "Spanish"),
        ("et", "Estonian"),
        ("fi", "Finnish"),
        ("fr", "French"),
        ("hi", "Hindi"),
        ("hr", "Croatian"),
        ("hu", "Hungarian"),
        ("id", "Indonesian"),
        ("it", "Italian"),
        ("ja", "Japanese"),
        ("ko", "Korean"),
        ("lt", "Lithuanian"),
        ("lv", "Latvian"),
        ("nl", "Dutch"),
        ("pa", "Punjabi"),
        ("pl", "Polish"),
        ("pt", "Portuguese"),
        ("ro", "Romanian"),
        ("ru", "Russian"),
        ("sk", "Slovak"),
        ("sl", "Slovenian"),
        ("sv", "Swedish"),
        ("tr", "Turkish"),
        ("uk", "Ukrainian"),
        ("vi", "Vietnamese"),
        ("zh", "Chinese"),
    ];
    let primary = language.split('-').next().unwrap_or(language);
    NAMES
        .iter()
        .find(|(code, _)| code.eq_ignore_ascii_case(primary))
        .map_or_else(|| language.to_owned(), |(_, name)| (*name).to_owned())
}

/// Fills in `measured` from `kivo-bench` metrics: `(metric name, p50)` pairs from the latest
/// `stt` and `tts` suite runs, named "<engine>: real-time factor" and so on.
pub fn apply_measurements(entries: &mut [RegistryEntry], metrics: &[(String, f64)], at: i64) {
    for entry in entries {
        let Some(name) = entry.bench_name else {
            continue;
        };
        let find = |what: &str| {
            metrics
                .iter()
                .find(|(m, _)| {
                    m.strip_prefix(name).and_then(|r| r.strip_prefix(": ")) == Some(what)
                })
                .map(|(_, v)| *v)
        };
        let measured = Measured {
            real_time_factor: find("real-time factor"),
            latency_ms: find("end of speech → final (median utterance)")
                .or_else(|| find("first audio")),
            word_error_rate: find("WER").map(|percent| percent / 100.0),
            measured_at: at,
        };
        if measured.real_time_factor.is_some()
            || measured.latency_ms.is_some()
            || measured.word_error_rate.is_some()
        {
            entry.measured = Some(measured);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_engine_kivo_ships_is_in_the_registry() {
        let ids: Vec<String> = registry().into_iter().map(|e| e.engine.id).collect();
        for engine in crate::engines() {
            assert!(ids.contains(&engine.id), "{} is missing", engine.id);
        }
        assert!(
            registry().iter().all(|e| e.privacy == Privacy::Local),
            "every engine KIVO ships today runs on this PC"
        );
    }

    #[test]
    fn profiles_map_to_engines_and_say_when_there_is_none() {
        let stt = profiles(EngineSlot::Stt, "en-US");
        let engine = |cards: &[ProfileCard], p: Profile| {
            cards
                .iter()
                .find(|c| c.profile == p)
                .unwrap()
                .engine
                .clone()
        };
        assert_eq!(
            engine(&stt, Profile::Recommended).as_deref(),
            Some("moonshine-base-en")
        );
        assert_eq!(
            engine(&stt, Profile::Lightweight).as_deref(),
            Some("moonshine-tiny-en")
        );
        assert_eq!(
            engine(&stt, Profile::HighAccuracy),
            None,
            "not available yet"
        );
        let multi = stt
            .iter()
            .find(|c| c.profile == Profile::Multilingual)
            .unwrap();
        assert!(multi.engine.is_none() && multi.other_languages_only);
        let ja = profiles(EngineSlot::Stt, "ja");
        assert_eq!(
            engine(&ja, Profile::Multilingual).as_deref(),
            Some("moonshine-base-ja"),
            "Base before Tiny"
        );
        let tts = profiles(EngineSlot::Tts, "en");
        assert_eq!(
            engine(&tts, Profile::Natural).as_deref(),
            Some("kokoro-82m")
        );
        assert_eq!(
            engine(&tts, Profile::Lightweight).as_deref(),
            Some("system")
        );
        assert_eq!(
            engine(&tts, Profile::Multilingual).as_deref(),
            Some("supertonic-3")
        );
        assert_eq!(engine(&tts, Profile::Expressive), None);
        let hi = profiles(EngineSlot::Tts, "hi");
        assert_eq!(
            engine(&hi, Profile::Natural),
            None,
            "Kokoro speaks English only"
        );
        assert_eq!(
            engine(&hi, Profile::Multilingual).as_deref(),
            Some("supertonic-3")
        );
    }

    #[test]
    fn non_commercial_models_are_marked() {
        assert!(entry("moonshine-base-en").unwrap().commercial_use);
        assert!(!entry("moonshine-base-es").unwrap().commercial_use);
    }

    #[test]
    fn incompatible_languages_are_explained_with_alternatives() {
        assert_eq!(language_problem("moonshine-base-en", "en-GB"), None);
        let es = language_problem("moonshine-base-en", "es").unwrap();
        assert_eq!(
            (es.engine.as_str(), es.language.as_str()),
            ("Moonshine Base", "Spanish")
        );
        assert_eq!(es.alternatives, ["Moonshine Base (Spanish)"]);
        let fr = language_problem("moonshine-base-en", "fr-FR").unwrap();
        assert!(fr.alternatives.is_empty(), "no recognizer for French yet");
        let kokoro = language_problem("kokoro-82m", "hi").unwrap();
        assert_eq!(kokoro.alternatives, ["Windows voices", "Supertonic 3"]);
        assert_eq!(language_name("xx"), "xx");
    }

    #[test]
    fn measurements_come_only_from_kivos_benchmarks() {
        let mut entries = registry();
        apply_measurements(
            &mut entries,
            &[
                ("moonshine: real-time factor".into(), 0.05),
                (
                    "moonshine: end of speech → final (median utterance)".into(),
                    120.0,
                ),
                ("moonshine: WER".into(), 4.2),
                ("parakeet: real-time factor".into(), 0.1),
            ],
            7,
        );
        let base = entries
            .iter()
            .find(|e| e.id() == "moonshine-base-en")
            .unwrap();
        let m = base.measured.as_ref().unwrap();
        assert_eq!(
            (
                m.real_time_factor,
                m.latency_ms,
                m.word_error_rate,
                m.measured_at
            ),
            (Some(0.05), Some(120.0), Some(0.042), 7)
        );
        assert!(
            entries
                .iter()
                .filter(|e| e.id() != "moonshine-base-en")
                .all(|e| e.measured.is_none()),
            "the rest read Not benchmarked by KIVO"
        );
    }
}
