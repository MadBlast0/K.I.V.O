//! The voice recommendation (plan §25, §90; PLAN-01): from what the PC has and is doing right now,
//! which speech engines to use and how many threads they may take. Onboarding shows it as the
//! preselected choice (UX §4) and the runtime uses it until the user picks otherwise.

use crate::engine::EngineInfo;
use kivo_platform::SystemSnapshot;
use serde::{Deserialize, Serialize};

/// BENCHMARKS §2's reference tiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Tier {
    /// 4 cores or fewer, or under 8 GB of memory, no useful GPU.
    Low,
    Mid,
    /// 12+ threads, 16 GB+, and a GPU with 6 GB+ of its own memory.
    High,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    pub tier: Tier,
    /// Speech-to-text engine id for the language, among those KIVO has.
    pub stt_engine: Option<String>,
    /// Text-to-speech engine id.
    pub tts_engine: String,
    /// Threads the speech models may use.
    pub threads: usize,
    /// Why, in a sentence the UI can show.
    pub reason: String,
}

pub fn tier(s: &SystemSnapshot) -> Tier {
    let gpu_mb = s.gpus.iter().map(|g| g.vram_mb).max().unwrap_or(0);
    if s.logical_cpus <= 4 || s.ram_mb < 8 * 1024 {
        Tier::Low
    } else if s.logical_cpus >= 12 && s.ram_mb >= 16 * 1000 && gpu_mb >= 6 * 1000 {
        Tier::High
    } else {
        Tier::Mid
    }
}

/// Recommends engines for `language` among `stt_engines` (what can run on this PC).
pub fn recommend(s: &SystemSnapshot, language: &str, stt_engines: &[EngineInfo]) -> Recommendation {
    let tier = tier(s);
    let stt_engine = crate::language::pick_stt(language, stt_engines).map(|e| e.id.clone());
    // Speech models never take the whole machine (plan §128): fewer threads on small PCs, on
    // battery, and when the PC is already busy.
    let mut threads = match tier {
        Tier::Low => 2,
        Tier::Mid | Tier::High => 4,
    };
    let mut why = vec![match tier {
        Tier::Low => "a smaller PC, so KIVO keeps its speech models light",
        Tier::Mid => "a mid-range PC",
        Tier::High => "a fast PC with a capable graphics card",
    }];
    if s.on_battery {
        threads = threads.min(2);
        why.push("on battery, so it uses fewer cores");
    }
    if s.cpu_load_percent >= 70 {
        threads = threads.min(2);
        why.push("the CPU is busy right now");
    }
    let cores = usize::try_from(s.logical_cpus).unwrap_or(1).max(1);
    Recommendation {
        tier,
        stt_engine,
        // The Windows voice works everywhere; Kokoro joins as its own choice (VOICE-09).
        tts_engine: crate::system_tts::ENGINE_ID.to_owned(),
        threads: threads.min(cores / 2).max(1),
        reason: format!("This is {}.", why.join("; ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kivo_platform::GpuInfo;

    fn pc(cpus: u32, ram_gb: u64, vram_gb: u64) -> SystemSnapshot {
        SystemSnapshot {
            cpu_name: "Test".into(),
            logical_cpus: cpus,
            ram_mb: ram_gb * 1024,
            ram_free_mb: ram_gb * 512,
            cpu_load_percent: 5,
            gpus: (vram_gb > 0)
                .then(|| GpuInfo {
                    name: "GPU".into(),
                    vram_mb: vram_gb * 1024,
                })
                .into_iter()
                .collect(),
            on_battery: false,
            battery_percent: None,
            fullscreen_app: false,
            focus_mode: false,
        }
    }

    #[test]
    fn machines_fall_into_the_reference_tiers() {
        assert_eq!(tier(&pc(4, 8, 0)), Tier::Low);
        assert_eq!(tier(&pc(8, 4, 0)), Tier::Low, "not enough memory");
        assert_eq!(tier(&pc(8, 16, 0)), Tier::Mid);
        assert_eq!(tier(&pc(16, 16, 6)), Tier::High, "the owner's PC");
    }

    #[test]
    fn battery_and_load_hold_the_models_back() {
        let engines = [crate::moonshine::info()];
        let fast = recommend(&pc(16, 16, 6), "en", &engines);
        assert_eq!(
            (fast.threads, fast.stt_engine.as_deref()),
            (4, Some("moonshine-base-en"))
        );
        let mut busy = pc(16, 16, 6);
        busy.on_battery = true;
        busy.cpu_load_percent = 90;
        let slow = recommend(&busy, "en", &engines);
        assert_eq!(slow.threads, 2);
        assert!(slow.reason.contains("battery") && slow.reason.contains("busy"));
        assert_eq!(
            recommend(&pc(2, 4, 0), "en", &engines).threads,
            1,
            "never more than half the cores"
        );
        assert_eq!(
            recommend(&pc(8, 16, 0), "hi", &engines).stt_engine,
            None,
            "no engine for Hindi yet"
        );
    }
}
