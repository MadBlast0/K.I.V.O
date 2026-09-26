//! The GPU policy (plan §91, PLAN-09, VOICE-35): a speech model that can use the graphics card
//! (whisper.cpp's) runs there first (owner, 2026-09-24), and on the processor when the card
//! isn't available. It isn't when KIVO would compete with GPU-heavy work: not
//! on the Battery or Gaming profiles, not while a fullscreen app (a game, a presentation) is in
//! front, not on battery unless the Performance profile says so, not on a card without the memory
//! for it, and not while something else keeps the GPU busy. Moving off the GPU happens at 60 %
//! load and back only below 30 %, so it doesn't flap.

use kivo_core::config::PerformanceProfile;
use kivo_platform::SystemSnapshot;

/// A card needs this much memory of its own for KIVO's largest GPU model with room to spare.
const MIN_VRAM_MB: u64 = kivo_voice::recommend::GPU_MIN_MB;
/// Busy thresholds: leave the GPU above the first, come back below the second.
const LEAVE_AT: u8 = 60;
const RETURN_BELOW: u8 = 30;

/// Speech models that can run on the graphics card: whisper.cpp's (Vulkan). The ONNX engines run
/// on the processor (DECISIONS "GGML on Vulkan, no DirectML").
pub fn can_use_gpu(engine: &str) -> bool {
    kivo_voice::whisper_cpp::variant(engine).is_some()
}

/// The DXGI adapter to run on, or `None` for the processor. `on_gpu_now` says where the model is,
/// for the busy thresholds.
pub fn choose(
    profile: PerformanceProfile,
    machine: &SystemSnapshot,
    gpu_load: Option<u8>,
    on_gpu_now: bool,
) -> Option<u32> {
    if matches!(
        profile,
        PerformanceProfile::Battery | PerformanceProfile::Gaming
    ) {
        return None;
    }
    if machine.fullscreen_app {
        return None;
    }
    if machine.on_battery && profile != PerformanceProfile::Performance {
        return None;
    }
    let busy_at = if on_gpu_now { LEAVE_AT } else { RETURN_BELOW };
    if gpu_load.is_some_and(|l| l >= busy_at) {
        return None;
    }
    // The card with the most memory of its own (adapters are listed in DXGI order).
    let (index, best) = machine
        .gpus
        .iter()
        .enumerate()
        .max_by_key(|(_, g)| g.vram_mb)?;
    if best.vram_mb < MIN_VRAM_MB {
        return None;
    }
    u32::try_from(index).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kivo_platform::GpuInfo;

    fn pc(vram_gb: &[u64]) -> SystemSnapshot {
        SystemSnapshot {
            cpu_name: "CPU".into(),
            logical_cpus: 16,
            ram_mb: 16_384,
            ram_free_mb: 8_000,
            cpu_load_percent: 5,
            gpus: vram_gb
                .iter()
                .map(|g| GpuInfo {
                    name: "GPU".into(),
                    vram_mb: g * 1024,
                })
                .collect(),
            on_battery: false,
            battery_percent: None,
            fullscreen_app: false,
            focus_mode: false,
        }
    }

    #[test]
    fn the_largest_card_when_nothing_else_needs_it() {
        // An integrated GPU first, the discrete one second: the discrete one.
        assert_eq!(
            choose(PerformanceProfile::Auto, &pc(&[1, 6]), Some(5), false),
            Some(1)
        );
        assert_eq!(
            choose(PerformanceProfile::Performance, &pc(&[8]), None, false),
            Some(0)
        );
        assert_eq!(
            choose(PerformanceProfile::Auto, &pc(&[2]), Some(0), false),
            None,
            "too small"
        );
        assert_eq!(
            choose(PerformanceProfile::Auto, &pc(&[]), Some(0), false),
            None
        );
    }

    #[test]
    fn never_competes_with_games_battery_or_busy_gpus() {
        let machine = pc(&[6]);
        assert_eq!(
            choose(PerformanceProfile::Gaming, &machine, Some(0), true),
            None
        );
        assert_eq!(
            choose(PerformanceProfile::Battery, &machine, Some(0), true),
            None
        );
        let mut game = machine.clone();
        game.fullscreen_app = true;
        assert_eq!(choose(PerformanceProfile::Auto, &game, Some(0), true), None);
        let mut battery = machine.clone();
        battery.on_battery = true;
        assert_eq!(
            choose(PerformanceProfile::Auto, &battery, Some(0), false),
            None
        );
        assert_eq!(
            choose(PerformanceProfile::Performance, &battery, Some(0), false),
            Some(0),
            "the user asked for performance"
        );
        // Busy: leave at 60 %, come back only below 30 %.
        assert_eq!(
            choose(PerformanceProfile::Auto, &machine, Some(50), true),
            Some(0)
        );
        assert_eq!(
            choose(PerformanceProfile::Auto, &machine, Some(65), true),
            None
        );
        assert_eq!(
            choose(PerformanceProfile::Auto, &machine, Some(50), false),
            None
        );
        assert_eq!(
            choose(PerformanceProfile::Auto, &machine, Some(20), false),
            Some(0)
        );
    }

    #[test]
    fn only_the_models_that_gain_from_it() {
        assert!(can_use_gpu("whisper-cpp-small") && can_use_gpu("whisper-cpp-turbo"));
        // ONNX engines stay on the processor (DECISIONS "GGML on Vulkan, no DirectML").
        assert!(!can_use_gpu("parakeet-tdt-v3") && !can_use_gpu("whisper-large-v3-turbo"));
        assert!(!can_use_gpu("moonshine-base-en"));
    }
}
