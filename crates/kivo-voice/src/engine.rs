//! What every speech engine declares about itself (VOICE §2). The Control Center shows it in the
//! engine pickers, the language router uses `languages`, and the privacy check uses `kind`.

use serde::{Deserialize, Serialize};

/// The engine slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EngineSlot {
    Vad,
    Wake,
    WakeVerifier,
    Speaker,
    Stt,
    Tts,
    TurnDetector,
}

/// Where the engine runs. Cloud engines send audio or text off the device, so they must pass the
/// privacy check first (VOICE-07).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EngineKind {
    Local,
    Cloud,
    /// Provided by the operating system (Windows voices, Windows AI Speech).
    System,
}

/// Hardware an engine can run on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Accel {
    Cpu,
    /// whisper.cpp's GPU backends (VOICE-50): Vulkan on any vendor, CUDA on NVIDIA, Metal on a Mac.
    Vulkan,
    Cuda,
    Metal,
    Npu,
}

impl Accel {
    /// A graphics-card backend (not the processor or an NPU).
    pub fn is_gpu(self) -> bool {
        matches!(self, Self::Vulkan | Self::Cuda | Self::Metal)
    }
}

/// Rough cost of keeping the engine loaded and running it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceEstimate {
    pub ram_mb: u32,
    pub vram_mb: u32,
    /// Download size; 0 for bundled or system engines.
    pub disk_mb: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineInfo {
    /// Stable id, e.g. "moonshine-base-en".
    pub id: String,
    pub name: String,
    pub slot: EngineSlot,
    pub kind: EngineKind,
    /// SPDX id or a named licence ("CC-BY-4.0", "OpenRAIL-M").
    pub license: String,
    /// BCP-47 primary tags; `*` means any language.
    pub languages: Vec<String>,
    /// Produces results while audio or text is still arriving.
    pub streaming: bool,
    pub accel: Vec<Accel>,
    pub resources: ResourceEstimate,
    /// The model this engine loads through the model manager (DIST-12), if any.
    pub model: Option<String>,
}

impl EngineInfo {
    /// True when the engine handles `language` (a BCP-47 tag like "en" or "en-US").
    pub fn supports(&self, language: &str) -> bool {
        let primary = language.split('-').next().unwrap_or(language);
        self.languages
            .iter()
            .any(|l| l == "*" || l.eq_ignore_ascii_case(primary))
    }

    /// Data leaves the device when this engine runs (VOICE-07, SECURITY §6).
    pub fn sends_data_off_device(&self) -> bool {
        self.kind == EngineKind::Cloud
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(languages: &[&str], kind: EngineKind) -> EngineInfo {
        EngineInfo {
            id: "x".into(),
            name: "X".into(),
            slot: EngineSlot::Stt,
            kind,
            license: "MIT".into(),
            languages: languages.iter().map(|l| (*l).to_owned()).collect(),
            streaming: false,
            accel: vec![Accel::Cpu],
            resources: ResourceEstimate {
                ram_mb: 1,
                vram_mb: 0,
                disk_mb: 1,
            },
            model: None,
        }
    }

    #[test]
    fn languages_match_on_the_primary_tag() {
        let e = info(&["en"], EngineKind::Local);
        assert!(e.supports("en") && e.supports("en-US") && e.supports("EN-gb"));
        assert!(!e.supports("hi"));
        assert!(info(&["*"], EngineKind::System).supports("pa"));
    }

    #[test]
    fn only_cloud_engines_send_data_off_the_device() {
        assert!(info(&["en"], EngineKind::Cloud).sends_data_off_device());
        assert!(!info(&["en"], EngineKind::Local).sends_data_off_device());
        assert!(!info(&["en"], EngineKind::System).sends_data_off_device());
    }
}
