//! The voice recommendation (plan §25, §90; PLAN-01, VOICE-44): from what the PC has and is doing
//! right now, the user's language, privacy choice and priority, what is installed and what KIVO
//! measured, which speech engines to use (with fallbacks) and how many threads they may take.
//! Onboarding shows it as the preselected choice (UX §4) and the runtime uses it until the user
//! picks otherwise. With "Speech recognition on the graphics card" on (the default) and a usable
//! GPU it recommends the recognizer that runs there (DECISIONS "Local models on the GPU first");
//! off, the processor's recognizers. It only ever proposes: nothing changes until the user chooses.

use crate::engine::EngineSlot;
use crate::registry::{Privacy, Profile, RegistryEntry};
use kivo_platform::SystemSnapshot;
use serde::{Deserialize, Serialize};

/// BENCHMARKS §2's reference tiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Tier {
    /// 4 cores or fewer, or under 8 GB of memory (as reported: under 7 GiB), no useful GPU.
    Low,
    Mid,
    /// 12+ threads, 16 GB+ (as reported: 15 GiB+), and a GPU with 6 GB+ of its own memory
    /// (5,500 MB+ reported).
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
    /// The user let speech recognition use the graphics card.
    pub gpu_speech: bool,
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

/// Memory as Windows reports it is a little under the size on the box (what the firmware and an
/// integrated GPU keep): a "16 GB" laptop reports ~15,500 MB and a "6 GB" RTX 3060 5,994 MB. The
/// tiers compare against these, not the nominal sizes.
const LOW_RAM_MB: u64 = 7 * 1024;
const HIGH_RAM_MB: u64 = 15 * 1024;
const HIGH_VRAM_MB: u64 = 5_500;

pub fn tier(s: &SystemSnapshot) -> Tier {
    let gpu_mb = s.gpus.iter().map(|g| g.vram_mb).max().unwrap_or(0);
    if s.logical_cpus <= 4 || s.ram_mb < LOW_RAM_MB {
        Tier::Low
    } else if s.logical_cpus >= 12 && s.ram_mb >= HIGH_RAM_MB && gpu_mb >= HIGH_VRAM_MB {
        Tier::High
    } else {
        Tier::Mid
    }
}

/// A recognizer needing more memory than this is "heavy" (VOICE §3's Balanced and Accurate tiers).
const HEAVY_RAM_MB: u32 = 600;
/// A graphics card with this much memory of its own can hold KIVO's GPU recognizers.
pub const GPU_MIN_MB: u64 = 3_000;

/// Whether speech recognition should run on this PC's graphics card: it has one with room for the
/// models, the PC isn't a small one, and it isn't on battery.
pub fn gpu_ready(s: &SystemSnapshot) -> bool {
    tier(s) != Tier::Low && !s.on_battery && s.gpus.iter().any(|g| g.vram_mb >= GPU_MIN_MB)
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
        Tier::High => "a fast PC",
    }];
    // Local models run on the graphics card first; the processor stands in when it can't.
    let prefer_gpu = needs.gpu_speech && gpu_ready(s) && needs.priority != Priority::Resources;
    if prefer_gpu {
        why.push(
            "speech recognition runs on its graphics card, and on the processor when the card is busy or a game is in front",
        );
    }
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
        // The graphics card's engines only when speech may use it.
        .filter(|e| prefer_gpu || !crate::registry::gpu_first(e))
        .collect();
    // With a usable GPU, the recognizers that run on it (whisper.cpp's) come first. Without one,
    // heavy recognizers come first only when accuracy is what the user asked for on a PC with room
    // for them; otherwise they're for languages nothing lighter covers.
    let on_gpu = crate::registry::gpu_first;
    // Asked for speed: the high-accuracy GPU model (Whisper) is the slow one, so it doesn't count.
    let speed = needs.priority == Priority::Speed;
    let gpu_pick = |e: &RegistryEntry| on_gpu(e) && !(speed && e.has(Profile::HighAccuracy));
    let accurate = needs.priority == Priority::Accuracy && tier != Tier::Low;
    let wants_heavy = accurate || (prefer_gpu && !speed);
    stt.sort_by_key(|e| {
        let tiny = e.engine.id.contains("-tiny-");
        let heavy = e.engine.resources.ram_mb > HEAVY_RAM_MB;
        (
            prefer_gpu && !gpu_pick(e),
            // The graphics card's engines left out above (its slow one, when speed was asked) still
            // before the processor's.
            prefer_gpu && !on_gpu(e),
            // Among the graphics card's engines, the one whose profile the priority asks for.
            prefer_gpu
                && !e.has(if accurate {
                    Profile::HighAccuracy
                } else if light {
                    Profile::Lightweight
                } else {
                    Profile::Recommended
                }),
            heavy != wants_heavy,
            // Asked for accuracy: the High-accuracy engine (Whisper) before the balanced one.
            accurate && !e.has(Profile::HighAccuracy),
            tiny != light,
            !installed(e),
            e.engine.resources.disk_mb,
        )
    });
    // How well each recognizer heard the owner's enrollment (VOICE-23): one clearly better on
    // this voice (5 points of WER or more) goes first.
    let voice_wer = |e: &RegistryEntry| e.measured.as_ref().and_then(|m| m.voice_word_error_rate);
    let best = stt
        .iter()
        .filter_map(|e| voice_wer(e).map(|w| (w, e.engine.id.clone())))
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((best_wer, best_id)) = best {
        let beats_first = stt
            .first()
            .and_then(|e| voice_wer(e))
            .is_some_and(|first| first - best_wer >= 0.05);
        if beats_first && let Some(i) = stt.iter().position(|e| e.engine.id == best_id) {
            let chosen = stt.remove(i);
            stt.insert(0, chosen);
            why.push("it heard your voice best when you set up voice recognition");
        }
    }
    let stt_engine = stt.first().map(|e| e.engine.id.clone());
    // A GPU recognizer's own fallback is the processor; if it can't load at all, a light
    // processor one stands in rather than the other heavy model.
    // A GPU recognizer's fallback is outside whisper.cpp (if it can't load, its siblings can't
    // either): a light processor one, or for accuracy the best of the rest.
    let stt_fallback = if prefer_gpu {
        stt.iter()
            .skip(1)
            .find(|e| {
                !on_gpu(e)
                    && !crate::registry::gpu_first(e)
                    && (accurate || e.engine.resources.ram_mb <= HEAVY_RAM_MB)
            })
            .or_else(|| stt.iter().skip(1).find(|e| !crate::registry::gpu_first(e)))
            .map(|e| e.engine.id.clone())
    } else {
        stt.get(1).map(|e| e.engine.id.clone())
    };

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
                    ..GpuInfo::default()
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
        advise_with(pc, language, priority, installed, false)
    }

    /// As `advise`, with "Speech recognition on the graphics card" on.
    fn advise_gpu(pc: &SystemSnapshot, language: &str, priority: Priority) -> Recommendation {
        advise_with(pc, language, priority, &[], true)
    }

    fn advise_with(
        pc: &SystemSnapshot,
        language: &str,
        priority: Priority,
        installed: &[&str],
        gpu_speech: bool,
    ) -> Recommendation {
        let registry = crate::registry::registry();
        let installed: Vec<String> = installed.iter().map(|s| (*s).to_owned()).collect();
        recommend(
            pc,
            &Needs {
                gpu_speech,
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
        // What Windows really reports: a "16 GB" laptop with a "6 GB" RTX 3060 (the owner's), and
        // an "8 GB" laptop. Nominal sizes are never fully reported.
        let mut real = pc(16, 0, 0);
        real.ram_mb = 15_556;
        real.gpus = vec![GpuInfo {
            name: "NVIDIA GeForce RTX 3060 Laptop GPU".into(),
            vram_mb: 5_994,
            ..GpuInfo::default()
        }];
        assert_eq!(
            tier(&real),
            Tier::High,
            "the owner's PC as Windows reports it"
        );
        let mut eight = pc(8, 0, 0);
        eight.ram_mb = 7_790;
        assert_eq!(tier(&eight), Tier::Mid, "an 8 GB laptop isn't a small PC");
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
        // By default the processor's recognizers, graphics card or not.
        let fast = advise(&pc(16, 16, 6), "en-US", Priority::Balanced, &[]);
        assert_eq!(fast.stt_engine.as_deref(), Some("moonshine-base-en"));
        assert_eq!(fast.stt_fallback.as_deref(), Some("moonshine-tiny-en"));
        assert_eq!(
            (fast.tts_engine.as_str(), fast.tts_fallback.as_deref()),
            ("kokoro-82m", Some("system"))
        );
        // With the graphics card allowed: the recognizer that runs on it, with a light processor
        // one behind it; on battery, still the processor.
        if crate::whisper_cpp::AVAILABLE {
            let gpu = advise_gpu(&pc(16, 16, 6), "en-US", Priority::Balanced);
            assert_eq!(gpu.stt_engine.as_deref(), Some("whisper-cpp-small"));
            assert_eq!(gpu.stt_fallback.as_deref(), Some("moonshine-base-en"));
            assert!(gpu.reason.contains("graphics card"), "{}", gpu.reason);
        }
        let mut unplugged = pc(16, 16, 6);
        unplugged.on_battery = true;
        let unplugged = advise_gpu(&unplugged, "en-US", Priority::Balanced);
        assert_eq!(unplugged.stt_engine.as_deref(), Some("moonshine-base-en"));

        let small = advise(&pc(4, 8, 0), "en", Priority::Balanced, &[]);
        assert_eq!(small.stt_engine.as_deref(), Some("moonshine-tiny-en"));
        assert_eq!(
            (small.tts_engine.as_str(), small.tts_fallback),
            ("system", None)
        );

        // Asking for accuracy with a graphics card: Whisper turbo on it, with the processor's
        // Whisper behind it.
        if crate::whisper_cpp::AVAILABLE {
            let accurate_gpu = advise_gpu(&pc(16, 16, 6), "en", Priority::Accuracy);
            assert_eq!(
                accurate_gpu.stt_engine.as_deref(),
                Some("whisper-cpp-turbo")
            );
            assert_eq!(
                accurate_gpu.stt_fallback.as_deref(),
                Some("whisper-large-v3-turbo")
            );
        }
        // Without one, the heavier recognizer first.
        let accurate = advise(&pc(16, 16, 0), "en", Priority::Accuracy, &[]);
        assert_eq!(
            accurate.stt_engine.as_deref(),
            Some("whisper-large-v3-turbo")
        );
        assert_eq!(accurate.stt_fallback.as_deref(), Some("parakeet-tdt-v3"));
        let small_accurate = advise(&pc(4, 8, 0), "en", Priority::Accuracy, &[]);
        assert!(
            !small_accurate
                .stt_engine
                .as_deref()
                .is_some_and(|e| e.starts_with("parakeet") || e.starts_with("whisper")),
            "a small PC keeps the light ones"
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
        let es = advise(&pc(16, 16, 0), "es", Priority::Balanced, &[]);
        assert_eq!(es.stt_engine.as_deref(), Some("moonshine-base-es"));
        if crate::whisper_cpp::AVAILABLE {
            let es_gpu = advise_gpu(&pc(16, 16, 6), "es", Priority::Balanced);
            assert_eq!(es_gpu.stt_engine.as_deref(), Some("whisper-cpp-small"));
        }
        assert_eq!(es.tts_engine, "supertonic-3");
        assert!(es.reason.contains("multilingual"));
        // French: only Parakeet has it, so it's the one, heavy or not.
        let fr = advise(&pc(16, 16, 6), "fr", Priority::Balanced, &[]);
        assert_eq!(fr.stt_engine.as_deref(), Some("parakeet-tdt-v3"));
        // Hindi: Whisper hears it; Swahili nothing on this PC does yet.
        let hi = advise(&pc(16, 16, 6), "hi", Priority::Balanced, &[]);
        assert_eq!(hi.stt_engine.as_deref(), Some("whisper-large-v3-turbo"));
        let sw = advise(&pc(16, 16, 6), "sw", Priority::Balanced, &[]);
        assert_eq!(sw.stt_engine, None, "no recognizer for Swahili yet");
        let ja = advise(
            &pc(16, 16, 6),
            "ja",
            Priority::Speed,
            &["moonshine-base-ja"],
        );
        assert_eq!(ja.stt_engine.as_deref(), Some("moonshine-tiny-ja"));
        assert_eq!(ja.stt_fallback.as_deref(), Some("moonshine-base-ja"));
        let sw = advise(&pc(8, 16, 0), "sw", Priority::Balanced, &[]);
        assert_eq!(sw.stt_engine, None, "no recognizer for Swahili yet");
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
            &pc(16, 16, 0),
            &Needs {
                gpu_speech: false,
                language: "en",
                local_only: true,
                priority: Priority::Balanced,
                installed: &[],
                registry: &registry,
            },
        );
        assert_eq!(r.stt_engine.as_deref(), Some("moonshine-tiny-en"));
    }

    #[test]
    fn the_recognizer_that_hears_the_owner_best_is_recommended() {
        fn needs(registry: &[RegistryEntry]) -> Needs<'_> {
            Needs {
                gpu_speech: false,
                language: "en",
                local_only: true,
                priority: Priority::Balanced,
                installed: &[],
                registry,
            }
        }
        let mut registry = crate::registry::registry();
        let base = recommend(&pc(16, 16, 0), &needs(&registry));
        assert_eq!(base.stt_engine.as_deref(), Some("moonshine-base-en"));
        // Tiny heard the enrollment clearly better than Base: it's recommended, with the reason.
        let wers = [
            ("moonshine-base-en".to_owned(), 0.22),
            ("moonshine-tiny-en".to_owned(), 0.08),
        ]
        .into_iter()
        .collect();
        crate::registry::apply_voice_wer(&mut registry, &wers, 1);
        let r = recommend(&pc(16, 16, 0), &needs(&registry));
        assert_eq!(r.stt_engine.as_deref(), Some("moonshine-tiny-en"));
        assert!(r.reason.contains("heard your voice best"), "{}", r.reason);
        // A small difference doesn't override the hardware choice.
        let close = [
            ("moonshine-base-en".to_owned(), 0.10),
            ("moonshine-tiny-en".to_owned(), 0.08),
        ]
        .into_iter()
        .collect();
        let mut registry = crate::registry::registry();
        crate::registry::apply_voice_wer(&mut registry, &close, 1);
        let r = recommend(&pc(16, 16, 0), &needs(&registry));
        assert_eq!(r.stt_engine.as_deref(), Some("moonshine-base-en"));
    }
}
