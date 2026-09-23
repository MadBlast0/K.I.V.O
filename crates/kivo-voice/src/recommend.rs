//! The voice recommendation (plan §25, §90; PLAN-01, VOICE-44): from what the PC has and is doing
//! right now, the user's language, privacy choice and priority, what is installed and what KIVO
//! measured, which speech engines to use (with fallbacks) and how many threads they may take.
//! Onboarding shows it as the preselected choice (UX §4) and the runtime uses it until the user
//! picks otherwise. It prefers CPU engines that meet the budgets, keeping the GPU for the user's
//! own work, and it only ever proposes: nothing changes until the user chooses.

use crate::engine::EngineSlot;
use crate::registry::{Privacy, Profile, RegistryEntry};
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

/// What the user said matters most (onboarding's "What matters most?").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Priority {
    #[default]
    Balanced,
    Speed,
    Accuracy,
    /// Use as little of the PC as possible.
    Resources,
    /// The most natural voice.
    Voice,
}

/// Everything the recommendation weighs besides the hardware.
#[derive(Clone, Copy, Debug)]
pub struct Needs<'a> {
    /// Primary language (BCP-47).
    pub language: &'a str,
    /// Only engines that keep audio on this PC (the privacy mode, or offline).
    pub local_only: bool,
    pub priority: Priority,
    /// Engine ids whose models are on this PC.
    pub installed: &'a [String],
    /// The registry, with KIVO's measurements filled in.
    pub registry: &'a [RegistryEntry],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    pub tier: Tier,
    /// Speech-to-text engine id for the language, among those KIVO has.
    pub stt_engine: Option<String>,
    /// Used when the chosen STT engine fails (VOICE-47).
    pub stt_fallback: Option<String>,
    /// Text-to-speech engine id.
    pub tts_engine: String,
    /// The Windows voices, unless they are the choice already.
    pub tts_fallback: Option<String>,
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

/// Recommends speech engines for this PC and these needs (VOICE-44).
pub fn recommend(s: &SystemSnapshot, needs: &Needs<'_>) -> Recommendation {
    let tier = tier(s);
    // Speech models never take the whole machine (plan §128): fewer threads on small PCs, on
    // battery, and when the PC is already busy.
    let mut threads = match tier {
        Tier::Low => 2,
        Tier::Mid | Tier::High => 4,
    };
    let mut why = vec![match tier {
        Tier::Low => "a smaller PC, so KIVO keeps its speech models light",
        Tier::Mid => "a mid-range PC, and its processor handles speech well",
        Tier::High => {
            "a fast PC; speech runs on the processor so the graphics card stays free for your own work"
        }
    }];
    if s.on_battery {
        threads = threads.min(2);
        why.push("on battery, so it uses fewer cores");
    }
    if s.cpu_load_percent >= 70 {
        threads = threads.min(2);
        why.push("the CPU is busy right now");
    }
    let light =
        tier == Tier::Low || matches!(needs.priority, Priority::Resources | Priority::Speed);
    let usable = |e: &RegistryEntry, slot: EngineSlot| {
        e.slot() == slot
            && e.engine.supports(needs.language)
            && (!needs.local_only || e.privacy == Privacy::Local)
    };
    let installed = |e: &RegistryEntry| needs.installed.contains(&e.engine.id);
    // Engines KIVO measured slower than real time on this PC are passed over.
    let fast_enough = |e: &RegistryEntry| {
        e.measured
            .as_ref()
            .and_then(|m| m.real_time_factor)
            .is_none_or(|rtf| rtf < 1.0)
    };

    // Speech to text: the lighter model when the PC or the user asks for it, else the larger
    // one; installed engines win ties, then the smaller download.
    let mut stt: Vec<&RegistryEntry> = needs
        .registry
        .iter()
        .filter(|e| usable(e, EngineSlot::Stt) && fast_enough(e))
        .collect();
    stt.sort_by_key(|e| {
        let tiny = e.engine.id.contains("-tiny-");
        (tiny != light, !installed(e), e.engine.resources.disk_mb)
    });
    let stt_engine = stt.first().map(|e| e.engine.id.clone());
    let stt_fallback = stt.get(1).map(|e| e.engine.id.clone());

    // Text to speech: the Windows voices are always there. A natural local voice when the PC
    // has room and it speaks the language, else the multilingual one.
    let system = crate::system_tts::ENGINE_ID;
    let voice = |profile: Profile| {
        needs.registry.iter().find(|e| {
            usable(e, EngineSlot::Tts) && fast_enough(e) && e.engine.id != system && e.has(profile)
        })
    };
    let tts_engine = if light && needs.priority != Priority::Voice {
        system
    } else if let Some(natural) = voice(Profile::Natural) {
        natural.id()
    } else if let Some(multilingual) = voice(Profile::Multilingual) {
        why.push("its multilingual voice speaks your language");
        multilingual.id()
    } else {
        system
    };
    let cores = usize::try_from(s.logical_cpus).unwrap_or(1).max(1);
    Recommendation {
        tier,
        stt_engine,
        stt_fallback,
        tts_engine: tts_engine.to_owned(),
        tts_fallback: (tts_engine != system).then(|| system.to_owned()),
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

    fn advise(
        pc: &SystemSnapshot,
        language: &str,
        priority: Priority,
        installed: &[&str],
    ) -> Recommendation {
        let registry = crate::registry::registry();
        let installed: Vec<String> = installed.iter().map(|s| (*s).to_owned()).collect();
        recommend(
            pc,
            &Needs {
                language,
                local_only: true,
                priority,
                installed: &installed,
                registry: &registry,
            },
        )
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
        let fast = advise(&pc(16, 16, 6), "en", Priority::Balanced, &[]);
        assert_eq!(fast.threads, 4);
        let mut busy = pc(16, 16, 6);
        busy.on_battery = true;
        busy.cpu_load_percent = 90;
        let slow = advise(&busy, "en", Priority::Balanced, &[]);
        assert_eq!(slow.threads, 2);
        assert!(slow.reason.contains("battery") && slow.reason.contains("busy"));
        assert_eq!(
            advise(&pc(2, 4, 0), "en", Priority::Balanced, &[]).threads,
            1,
            "never more than half the cores"
        );
    }

    #[test]
    fn a_capable_pc_gets_the_larger_models_and_a_small_one_the_light_ones() {
        let fast = advise(&pc(16, 16, 6), "en-US", Priority::Balanced, &[]);
        assert_eq!(fast.stt_engine.as_deref(), Some("moonshine-base-en"));
        assert_eq!(fast.stt_fallback.as_deref(), Some("moonshine-tiny-en"));
        assert_eq!(
            (fast.tts_engine.as_str(), fast.tts_fallback.as_deref()),
            ("kokoro-82m", Some("system"))
        );
        assert!(
            fast.reason.contains("graphics card stays free"),
            "{}",
            fast.reason
        );

        let small = advise(&pc(4, 8, 0), "en", Priority::Balanced, &[]);
        assert_eq!(small.stt_engine.as_deref(), Some("moonshine-tiny-en"));
        assert_eq!(
            (small.tts_engine.as_str(), small.tts_fallback),
            ("system", None)
        );

        let frugal = advise(&pc(16, 16, 6), "en", Priority::Resources, &[]);
        assert_eq!(frugal.stt_engine.as_deref(), Some("moonshine-tiny-en"));
        let voice = advise(&pc(4, 8, 0), "en", Priority::Voice, &[]);
        assert_eq!(
            voice.tts_engine, "kokoro-82m",
            "the user asked for the best voice"
        );
    }

    #[test]
    fn other_languages_get_their_own_models_and_the_multilingual_voice() {
        let es = advise(&pc(16, 16, 6), "es", Priority::Balanced, &[]);
        assert_eq!(es.stt_engine.as_deref(), Some("moonshine-base-es"));
        assert_eq!(es.tts_engine, "supertonic-3");
        assert!(es.reason.contains("multilingual"));
        let fr = advise(&pc(16, 16, 6), "fr", Priority::Balanced, &[]);
        assert_eq!(fr.stt_engine, None, "no recognizer for French yet");
        let ja = advise(
            &pc(16, 16, 6),
            "ja",
            Priority::Speed,
            &["moonshine-base-ja"],
        );
        assert_eq!(ja.stt_engine.as_deref(), Some("moonshine-tiny-ja"));
        assert_eq!(ja.stt_fallback.as_deref(), Some("moonshine-base-ja"));
        let hi = advise(&pc(8, 16, 0), "hi", Priority::Balanced, &[]);
        assert_eq!(hi.stt_engine, None, "no recognizer for Hindi yet");
    }

    #[test]
    fn engines_measured_slower_than_real_time_are_passed_over() {
        let mut registry = crate::registry::registry();
        crate::registry::apply_measurements(
            &mut registry,
            &[("moonshine: real-time factor".into(), 1.4)],
            1,
        );
        let r = recommend(
            &pc(16, 16, 6),
            &Needs {
                language: "en",
                local_only: true,
                priority: Priority::Balanced,
                installed: &[],
                registry: &registry,
            },
        );
        assert_eq!(r.stt_engine.as_deref(), Some("moonshine-tiny-en"));
    }
}
