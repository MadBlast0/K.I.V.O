//! Language packs (VOICE §9): which engines, voices, keyword model and command grammar serve each
//! language. Languages ship one at a time (English first); the rest are designed for but listed as
//! Planned until they are tested.

use crate::engine::EngineInfo;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LanguageStatus {
    Planned,
    Alpha,
    Supported,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguagePack {
    /// BCP-47 primary tag.
    pub code: String,
    /// Native name, as the language picker shows it.
    pub name: String,
    /// STT engine ids, best first.
    pub stt_engines: Vec<String>,
    /// TTS voice ids (`engine:voice`), best first.
    pub tts_voices: Vec<String>,
    /// Keyword-spotting model for custom wake words, where one exists.
    pub kws_model: Option<String>,
    /// The fast-path grammar folder (`grammar/<code>/`).
    pub grammar: String,
    /// Words the engines should favour (names of apps and commands).
    pub vocabulary: Vec<String>,
    pub status: LanguageStatus,
}

/// Every language KIVO knows about, in shipping order (DECISIONS: Languages).
pub fn packs() -> Vec<LanguagePack> {
    let planned = |code: &str, name: &str| LanguagePack {
        code: code.into(),
        name: name.into(),
        stt_engines: Vec::new(),
        tts_voices: vec!["system:default".into()],
        kws_model: None,
        grammar: code.into(),
        vocabulary: Vec::new(),
        status: LanguageStatus::Planned,
    };
    vec![
        LanguagePack {
            code: "en".into(),
            name: "English".into(),
            stt_engines: vec![crate::moonshine::MODEL_ID.into()],
            tts_voices: vec!["system:default".into()],
            kws_model: None,
            grammar: "en".into(),
            vocabulary: ["KIVO", "Chrome", "Spotify", "screenshot"]
                .map(String::from)
                .to_vec(),
            status: LanguageStatus::Alpha,
        },
        planned("hi", "हिन्दी"),
        planned("pa", "ਪੰਜਾਬੀ"),
    ]
}

/// The pack for `language` (a BCP-47 tag), if KIVO has one.
pub fn pack(language: &str) -> Option<LanguagePack> {
    let primary = language.split('-').next().unwrap_or(language);
    packs()
        .into_iter()
        .find(|p| p.code.eq_ignore_ascii_case(primary))
}

/// Picks the STT engine for `language`: the pack's preference order, among the engines that are
/// available and declare the language; else any available engine that declares it.
pub fn pick_stt<'a>(language: &str, available: &'a [EngineInfo]) -> Option<&'a EngineInfo> {
    let usable = |e: &&EngineInfo| e.supports(language);
    pack(language)
        .and_then(|p| {
            p.stt_engines
                .iter()
                .find_map(|id| available.iter().filter(usable).find(|e| &e.id == id))
        })
        .or_else(|| available.iter().find(usable))
}

/// The language to use: the primary, unless the engine can identify the language itself and the
/// user speaks more than one (VOICE-37). Only engines that measured reliable language ID would
/// return `Some` from `detected`; Moonshine doesn't identify languages.
pub fn active_language<'a>(
    primary: &'a str,
    secondary: &'a [String],
    detected: Option<&'a str>,
) -> &'a str {
    match detected {
        Some(d) if secondary.iter().any(|s| s.eq_ignore_ascii_case(d)) => d,
        _ => primary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{EngineKind, EngineSlot, ResourceEstimate};

    fn engine(id: &str, languages: &[&str]) -> EngineInfo {
        EngineInfo {
            id: id.into(),
            name: id.into(),
            slot: EngineSlot::Stt,
            kind: EngineKind::Local,
            license: "MIT".into(),
            languages: languages.iter().map(|l| (*l).to_owned()).collect(),
            streaming: false,
            accel: Vec::new(),
            resources: ResourceEstimate {
                ram_mb: 0,
                vram_mb: 0,
                disk_mb: 0,
            },
            model: None,
        }
    }

    #[test]
    fn english_ships_first_and_the_rest_are_planned() {
        let en = pack("en-US").unwrap();
        assert_eq!(en.status, LanguageStatus::Alpha);
        assert_eq!(pack("hi").unwrap().status, LanguageStatus::Planned);
        assert!(pack("xx").is_none());
    }

    #[test]
    fn the_router_prefers_the_packs_order_then_anything_that_fits() {
        let engines = [
            engine("whisper", &["*"]),
            engine(crate::moonshine::MODEL_ID, &["en"]),
        ];
        assert_eq!(
            pick_stt("en", &engines).unwrap().id,
            crate::moonshine::MODEL_ID
        );
        assert_eq!(pick_stt("hi", &engines).unwrap().id, "whisper");
        assert!(pick_stt("hi", &engines[1..]).is_none());
    }

    #[test]
    fn detected_languages_count_only_when_the_user_speaks_them() {
        let secondary = vec!["hi".to_owned()];
        assert_eq!(active_language("en", &secondary, Some("hi")), "hi");
        assert_eq!(active_language("en", &secondary, Some("fr")), "en");
        assert_eq!(active_language("en", &secondary, None), "en");
    }
}
