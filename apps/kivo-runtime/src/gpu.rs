//! The GPU policy (plan §91, PLAN-09, VOICE-35, VOICE-50): a speech model that can use the graphics
//! card (whisper.cpp's) runs there first (owner, 2026-09-24), and on the processor when the card
//! isn't available. It isn't when KIVO would compete with GPU-heavy work: not on the Battery or
//! Gaming profiles, not while a fullscreen app (a game, a presentation) is in front, not on
//! battery unless the Performance profile says so, not on a card without the memory for it, and
//! not while something else keeps the GPU busy. Moving off the GPU happens at 60 % load and back
//! only below 30 %, so it doesn't flap.
//!
//! On the chosen card, the backend follows Settings → Performance → Graphics backend: Automatic
//! picks CUDA on NVIDIA once its runtime is installed (and the driver is new enough), Vulkan on
//! AMD and Intel (and NVIDIA until then), Metal on a Mac. A choice the PC can't use falls back to
//! Automatic's; Processor only keeps local models off the card.

use kivo_core::config::{GraphicsBackend, PerformanceProfile};
use kivo_ipc::infer::{GpuBackend, GpuTarget};
use kivo_platform::{GpuInfo, GpuVendor, SystemSnapshot};

/// A card needs this much memory of its own for KIVO's largest GPU model with room to spare.
const MIN_VRAM_MB: u64 = kivo_voice::recommend::GPU_MIN_MB;
/// Busy thresholds: leave the GPU above the first, come back below the second.
const LEAVE_AT: u8 = 60;
const RETURN_BELOW: u8 = 30;
/// CUDA 13's oldest NVIDIA driver; below it CUDA can't start and Vulkan is used.
pub const CUDA_MIN_DRIVER: u32 = 580;

/// Speech models that can run on the graphics card: whisper.cpp's. The ONNX engines run on the
/// processor (DECISIONS "GGML on Vulkan, no DirectML").
pub fn can_use_gpu(engine: &str) -> bool {
    kivo_voice::whisper_cpp::variant(engine).is_some()
}

/// The card to run on, or `None` for the processor. `on_gpu_now` says where the model is, for the
/// busy thresholds.
pub fn choose(
    profile: PerformanceProfile,
    machine: &SystemSnapshot,
    gpu_load: Option<u8>,
    on_gpu_now: bool,
) -> Option<&GpuInfo> {
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
    // The card with the most memory of its own.
    let best = machine.gpus.iter().max_by_key(|g| g.vram_mb)?;
    (best.vram_mb >= MIN_VRAM_MB).then_some(best)
}

/// Whether CUDA can run on `card`: an NVIDIA card with a new enough driver, on a build with a CUDA
/// worker whose runtime is installed (`cuda_ready`).
fn cuda_usable(card: &GpuInfo, cuda_ready: bool) -> bool {
    cfg!(all(windows, target_arch = "x86_64"))
        && cuda_ready
        && card.vendor == GpuVendor::Nvidia
        && card
            .nvidia_driver()
            .is_some_and(|(major, _)| major >= CUDA_MIN_DRIVER)
}

/// The backend for `card` under the `setting`; `None` keeps the model on the processor.
pub fn backend_for(
    setting: GraphicsBackend,
    card: &GpuInfo,
    cuda_ready: bool,
) -> Option<GpuBackend> {
    let cuda = cuda_usable(card, cuda_ready);
    let vulkan = cfg!(all(windows, target_arch = "x86_64"));
    let metal = cfg!(target_os = "macos");
    match setting {
        GraphicsBackend::Processor => None,
        GraphicsBackend::Cuda if cuda => Some(GpuBackend::Cuda),
        GraphicsBackend::Vulkan if vulkan => Some(GpuBackend::Vulkan),
        GraphicsBackend::Metal if metal => Some(GpuBackend::Metal),
        // Automatic, or a choice this PC can't use.
        _ if metal => Some(GpuBackend::Metal),
        _ if cuda => Some(GpuBackend::Cuda),
        _ if vulkan => Some(GpuBackend::Vulkan),
        _ => None,
    }
}

/// Where a GPU-capable model runs now: the card `choose` picks, through the setting's backend.
pub fn target(
    setting: GraphicsBackend,
    profile: PerformanceProfile,
    machine: &SystemSnapshot,
    gpu_load: Option<u8>,
    on_gpu_now: bool,
    cuda_ready: bool,
) -> Option<GpuTarget> {
    let card = choose(profile, machine, gpu_load, on_gpu_now)?;
    Some(GpuTarget {
        backend: backend_for(setting, card, cuda_ready)?,
        device: card.name.clone(),
    })
}

/// The Graphics backend choices this PC has (the setting shows only these): Automatic and
/// Processor only always, CUDA with an NVIDIA card, Vulkan with any card where KIVO has a Vulkan
/// worker, Metal on a Mac.
pub fn options(gpus: &[GpuInfo]) -> Vec<GraphicsBackend> {
    let windows = cfg!(all(windows, target_arch = "x86_64"));
    let mut out = vec![GraphicsBackend::Auto];
    if windows && gpus.iter().any(|g| g.vendor == GpuVendor::Nvidia) {
        out.push(GraphicsBackend::Cuda);
    }
    if windows && !gpus.is_empty() {
        out.push(GraphicsBackend::Vulkan);
    }
    if cfg!(target_os = "macos") {
        out.push(GraphicsBackend::Metal);
    }
    out.push(GraphicsBackend::Processor);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(name: &str, vram_gb: u64, vendor: GpuVendor, driver: &str) -> GpuInfo {
        GpuInfo {
            name: name.into(),
            vram_mb: vram_gb * 1024,
            vendor,
            driver_version: Some(driver.into()),
        }
    }

    fn pc(gpus: Vec<GpuInfo>) -> SystemSnapshot {
        SystemSnapshot {
            cpu_name: "CPU".into(),
            logical_cpus: 16,
            ram_mb: 16_384,
            ram_free_mb: 8_000,
            cpu_load_percent: 5,
            gpus,
            on_battery: false,
            battery_percent: None,
            fullscreen_app: false,
            focus_mode: false,
        }
    }

    fn sizes(vram_gb: &[u64]) -> SystemSnapshot {
        pc(vram_gb
            .iter()
            .map(|g| card("GPU", *g, GpuVendor::Other, "1.0.0.0"))
            .collect())
    }

    /// A laptop's integrated GPU listed first, its NVIDIA card second.
    fn laptop() -> SystemSnapshot {
        pc(vec![
            card("AMD Radeon(TM) Graphics", 0, GpuVendor::Amd, "31.0.21.1"),
            card(
                "NVIDIA GeForce RTX 3060 Laptop GPU",
                6,
                GpuVendor::Nvidia,
                "32.0.16.1692",
            ),
        ])
    }

    #[test]
    fn the_largest_card_when_nothing_else_needs_it() {
        let two = pc(vec![
            card("small", 1, GpuVendor::Intel, "1.0.0.0"),
            card("big", 6, GpuVendor::Amd, "1.0.0.0"),
        ]);
        assert_eq!(
            choose(PerformanceProfile::Auto, &two, Some(5), false).map(|g| g.name.as_str()),
            Some("big")
        );
        assert!(choose(PerformanceProfile::Performance, &sizes(&[8]), None, false).is_some());
        assert!(
            choose(PerformanceProfile::Auto, &sizes(&[2]), Some(0), false).is_none(),
            "too small"
        );
        assert!(choose(PerformanceProfile::Auto, &sizes(&[]), Some(0), false).is_none());
    }

    #[test]
    fn never_competes_with_games_battery_or_busy_gpus() {
        let machine = sizes(&[6]);
        let on = |profile, m: &SystemSnapshot, load, now| choose(profile, m, load, now).is_some();
        assert!(!on(PerformanceProfile::Gaming, &machine, Some(0), true));
        assert!(!on(PerformanceProfile::Battery, &machine, Some(0), true));
        let mut game = machine.clone();
        game.fullscreen_app = true;
        assert!(!on(PerformanceProfile::Auto, &game, Some(0), true));
        let mut battery = machine.clone();
        battery.on_battery = true;
        assert!(!on(PerformanceProfile::Auto, &battery, Some(0), false));
        assert!(
            on(PerformanceProfile::Performance, &battery, Some(0), false),
            "the user asked for performance"
        );
        // Busy: leave at 60 %, come back only below 30 %.
        assert!(on(PerformanceProfile::Auto, &machine, Some(50), true));
        assert!(!on(PerformanceProfile::Auto, &machine, Some(65), true));
        assert!(!on(PerformanceProfile::Auto, &machine, Some(50), false));
        assert!(on(PerformanceProfile::Auto, &machine, Some(20), false));
    }

    #[test]
    fn only_the_models_that_gain_from_it() {
        assert!(can_use_gpu("whisper-cpp-small") && can_use_gpu("whisper-cpp-turbo"));
        // ONNX engines stay on the processor (DECISIONS "GGML on Vulkan, no DirectML").
        assert!(!can_use_gpu("parakeet-tdt-v3") && !can_use_gpu("whisper-large-v3-turbo"));
        assert!(!can_use_gpu("moonshine-base-en"));
    }

    /// The wrong-GPU bug (VOICE-50): the target names the card the policy chose, not "the GPU",
    /// so the worker can't fall back to the first one it lists (the integrated GPU here).
    #[test]
    #[cfg(all(windows, target_arch = "x86_64"))]
    fn the_target_names_the_chosen_card_and_its_backend() {
        let t = |setting, cuda_ready| {
            target(
                setting,
                PerformanceProfile::Auto,
                &laptop(),
                Some(0),
                false,
                cuda_ready,
            )
        };
        let rtx = "NVIDIA GeForce RTX 3060 Laptop GPU".to_owned();
        let vulkan = Some(GpuTarget {
            backend: GpuBackend::Vulkan,
            device: rtx.clone(),
        });
        let cuda = Some(GpuTarget {
            backend: GpuBackend::Cuda,
            device: rtx,
        });
        // Automatic: Vulkan until the CUDA runtime is installed, CUDA after.
        assert_eq!(t(GraphicsBackend::Auto, false), vulkan);
        assert_eq!(t(GraphicsBackend::Auto, true), cuda);
        // Asking for CUDA before it's installed falls back to Automatic's choice.
        assert_eq!(t(GraphicsBackend::Cuda, false), vulkan);
        assert_eq!(t(GraphicsBackend::Vulkan, true), vulkan);
        assert_eq!(t(GraphicsBackend::Processor, true), None);
        // Metal isn't on Windows: Automatic's choice.
        assert_eq!(t(GraphicsBackend::Metal, true), cuda);
    }

    #[test]
    #[cfg(all(windows, target_arch = "x86_64"))]
    fn cuda_needs_nvidia_and_a_new_enough_driver() {
        let old = card(
            "NVIDIA GeForce GTX 1660",
            6,
            GpuVendor::Nvidia,
            "31.0.15.3713",
        );
        assert_eq!(
            backend_for(GraphicsBackend::Cuda, &old, true),
            Some(GpuBackend::Vulkan),
            "driver 537 is older than CUDA 13 needs"
        );
        let amd = card("AMD Radeon RX 7800 XT", 16, GpuVendor::Amd, "31.0.24.1");
        assert_eq!(
            backend_for(GraphicsBackend::Auto, &amd, true),
            Some(GpuBackend::Vulkan)
        );
        assert_eq!(
            backend_for(GraphicsBackend::Cuda, &amd, true),
            Some(GpuBackend::Vulkan)
        );
    }

    #[test]
    #[cfg(all(windows, target_arch = "x86_64"))]
    fn the_setting_offers_what_this_pc_has() {
        use GraphicsBackend::{Auto, Cuda, Processor, Vulkan};
        assert_eq!(options(&laptop().gpus), [Auto, Cuda, Vulkan, Processor]);
        let amd = [card("AMD", 8, GpuVendor::Amd, "1.0.0.0")];
        assert_eq!(options(&amd), [Auto, Vulkan, Processor]);
        assert_eq!(options(&[]), [Auto, Processor]);
    }
}
