//! Performance profiles (plan §90, PLAN-08): Low Resource, Balanced, Performance, Battery, Gaming
//! and Custom. Auto (the default) picks one from what the PC is doing — a fullscreen app in front
//! means Gaming, running on battery means Battery, a small PC means Low Resource, anything else
//! Balanced — and the user can pick one outright. A profile decides how many threads speech
//! models may take, how long they stay loaded after use, and whether they may use the GPU (the GPU
//! policy still has the last word on a busy card).

use kivo_core::config::{KivoConfig, PerformanceProfile};
use kivo_platform::SystemSnapshot;

/// What a profile means in practice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resolved {
    /// The profile in effect (Auto resolved to one of the others).
    pub profile: PerformanceProfile,
    /// Most threads speech models may use.
    pub threads: usize,
    /// Minutes models stay loaded after use (0: unloaded right away).
    pub warm_minutes: u64,
    /// Whether a speech model may go on the GPU at all.
    pub gpu: bool,
}

/// A PC this small gets Low Resource from Auto.
const SMALL_RAM_MB: u64 = 8 * 1024;
const SMALL_CPUS: u32 = 4;

/// The profile in effect for `config` on `machine` (`None` when KIVO can't read the machine).
pub fn resolve(config: &KivoConfig, machine: Option<&SystemSnapshot>) -> Resolved {
    let cores = machine.map_or_else(
        || std::thread::available_parallelism().map_or(2, std::num::NonZero::get),
        |m| usize::try_from(m.logical_cpus).unwrap_or(2),
    );
    let half = (cores / 2).max(1);
    let chosen = config.performance.profile;
    let profile = match (chosen, machine) {
        (PerformanceProfile::Auto, Some(m)) if m.fullscreen_app => PerformanceProfile::Gaming,
        (PerformanceProfile::Auto, Some(m)) if m.on_battery => PerformanceProfile::Battery,
        (PerformanceProfile::Auto, Some(m))
            if m.ram_mb < SMALL_RAM_MB || m.logical_cpus <= SMALL_CPUS =>
        {
            PerformanceProfile::LowResource
        }
        (PerformanceProfile::Auto, _) => PerformanceProfile::Balanced,
        (p, _) => p,
    };
    let own_warm = u64::from(
        config
            .performance
            .stt_warm_minutes
            .max(config.performance.tts_warm_minutes),
    );
    let mut resolved = match profile {
        PerformanceProfile::LowResource => Resolved {
            profile,
            threads: half.min(2),
            warm_minutes: 0,
            gpu: false,
        },
        PerformanceProfile::Battery => Resolved {
            profile,
            threads: half.min(2),
            warm_minutes: own_warm.min(2),
            gpu: false,
        },
        PerformanceProfile::Gaming => Resolved {
            profile,
            threads: half.min(2),
            warm_minutes: own_warm.min(5),
            gpu: false,
        },
        PerformanceProfile::Performance => Resolved {
            profile,
            threads: half.min(8),
            warm_minutes: own_warm.max(30),
            gpu: true,
        },
        // Balanced, and Custom's starting point.
        _ => Resolved {
            profile,
            threads: half.min(4),
            warm_minutes: own_warm,
            gpu: true,
        },
    };
    // Custom: the user's own numbers where they set them.
    if profile == PerformanceProfile::Custom {
        if config.performance.speech_threads > 0 {
            resolved.threads = usize::from(config.performance.speech_threads).min(cores);
        }
        resolved.warm_minutes = own_warm;
    } else if config.performance.speech_threads > 0 {
        // An explicit thread limit (VOICE-49) holds in every profile.
        resolved.threads = usize::from(config.performance.speech_threads).min(cores);
    }
    // Low-memory mode unloads models right after use, whatever the profile (UX §5).
    if config.general.low_memory_mode {
        resolved.warm_minutes = 0;
    }
    resolved
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pc(cpus: u32, ram_gb: u64) -> SystemSnapshot {
        SystemSnapshot {
            cpu_name: "CPU".into(),
            logical_cpus: cpus,
            ram_mb: ram_gb * 1024,
            ram_free_mb: 1_000,
            cpu_load_percent: 5,
            gpus: Vec::new(),
            on_battery: false,
            battery_percent: None,
            fullscreen_app: false,
            focus_mode: false,
        }
    }

    #[test]
    fn auto_follows_what_the_pc_is_doing() {
        let config = KivoConfig::default();
        let big = pc(16, 32);
        assert_eq!(
            resolve(&config, Some(&big)).profile,
            PerformanceProfile::Balanced
        );
        let mut game = big.clone();
        game.fullscreen_app = true;
        let gaming = resolve(&config, Some(&game));
        assert_eq!(gaming.profile, PerformanceProfile::Gaming);
        assert!(!gaming.gpu && gaming.threads == 2);
        let mut battery = big.clone();
        battery.on_battery = true;
        assert_eq!(
            resolve(&config, Some(&battery)).profile,
            PerformanceProfile::Battery
        );
        let small = resolve(&config, Some(&pc(4, 8)));
        assert_eq!(small.profile, PerformanceProfile::LowResource);
        assert_eq!(
            (small.warm_minutes, small.gpu),
            (0, false),
            "unloaded right after use"
        );
        assert_eq!(resolve(&config, None).profile, PerformanceProfile::Balanced);
    }

    #[test]
    fn a_chosen_profile_is_kept_and_custom_uses_the_users_numbers() {
        let mut config = KivoConfig::default();
        config.performance.profile = PerformanceProfile::Performance;
        let mut game = pc(16, 32);
        game.fullscreen_app = true;
        let perf = resolve(&config, Some(&game));
        assert_eq!(
            perf.profile,
            PerformanceProfile::Performance,
            "no auto switch"
        );
        assert_eq!((perf.threads, perf.gpu), (8, true));
        assert!(perf.warm_minutes >= 30);
        config.performance.profile = PerformanceProfile::Custom;
        config.performance.speech_threads = 3;
        config.performance.stt_warm_minutes = 45;
        let custom = resolve(&config, Some(&pc(16, 32)));
        assert_eq!((custom.threads, custom.warm_minutes), (3, 45));
        config.general.low_memory_mode = true;
        assert_eq!(resolve(&config, Some(&pc(16, 32))).warm_minutes, 0);
    }
}
