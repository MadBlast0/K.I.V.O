//! Hardware and system state (plan §25, §90–91).

use crate::error::PlatformResult;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuInfo {
    pub name: String,
    pub vram_mb: u64,
}

/// A point-in-time view of the machine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSnapshot {
    pub cpu_name: String,
    pub logical_cpus: u32,
    pub ram_mb: u64,
    /// Memory free right now.
    pub ram_free_mb: u64,
    /// How busy the CPU is right now, 0–100 (over a short sample).
    pub cpu_load_percent: u8,
    pub gpus: Vec<GpuInfo>,
    pub on_battery: bool,
    pub battery_percent: Option<u8>,
    /// A fullscreen app or game is in front (UX §2: suppress the Island).
    pub fullscreen_app: bool,
    /// Windows Focus / Do not disturb (UX §7).
    pub focus_mode: bool,
}

/// Whether the user is presenting, gaming or in Focus right now (UX §2, §7).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attention {
    pub fullscreen_app: bool,
    pub focus_mode: bool,
}

/// Whether the user is here and free to be spoken to (UX §7).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Presence {
    /// Seconds since the last keyboard or mouse input.
    pub idle_seconds: u32,
    pub locked: bool,
    /// Another app has the microphone open: probably a call.
    pub mic_in_use_elsewhere: bool,
}

pub trait SystemInfo: Send + Sync {
    /// Everything, including a short CPU-load sample (~100 ms).
    fn snapshot(&self) -> PlatformResult<SystemSnapshot>;
    /// Only fullscreen and Focus: cheap enough for every request.
    fn attention(&self) -> PlatformResult<Attention>;
    /// Idle time, the lock screen and a call, for proactive speech (UX-40).
    fn presence(&self) -> Presence {
        Presence::default()
    }
    /// A process's CPU time so far and private memory (Settings → Performance). `None` when it
    /// can't be read (gone, or not ours to read).
    fn process_usage(&self, _pid: u32) -> Option<ProcessUsage> {
        None
    }
    /// Dedicated graphics memory the processes `pids` use, in bytes (PLAN-07). `None` when it
    /// can't be read.
    fn gpu_memory(&self, _pids: &[u32]) -> Option<u64> {
        None
    }
    /// How busy the graphics cards' 3D engines are right now, 0–100 (the busiest adapter), over a
    /// short sample (PLAN-09). `None` when it can't be read.
    fn gpu_load(&self) -> Option<u8> {
        None
    }
    /// Whether the PC reaches the internet and whether the connection is metered (plan §130).
    /// `None` when it can't be told.
    fn network(&self) -> Option<Network> {
        None
    }
    /// The names of the networks the PC is connected to (routines' network trigger, ROUT-11).
    fn networks(&self) -> Vec<String> {
        Vec::new()
    }
    /// The names of the USB devices plugged in (routines' USB trigger, ROUT-11).
    fn usb_devices(&self) -> Vec<String> {
        Vec::new()
    }
}

/// The network as Windows sees it (plan §130: recommendations).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Network {
    pub online: bool,
    /// Pay-per-use or capped (a phone hotspot): large downloads wait for the user.
    pub metered: bool,
}

/// What one process has used.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessUsage {
    /// CPU time (user + kernel) since it started, in milliseconds.
    pub cpu_ms: u64,
    /// Private working set, in bytes.
    pub memory_bytes: u64,
}

/// The speaker or microphone level (TOOLS_AND_CONTROL §3, "Audio").
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeState {
    /// 0–1.
    pub level: f32,
    pub muted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MediaAction {
    PlayPause,
    Next,
    Previous,
}

/// What the system's media controls report as playing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    /// The app playing it, as the OS names it.
    pub app: String,
    pub playing: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PowerAction {
    Lock,
    Sleep,
    Restart,
    Shutdown,
}

/// Volume, media keys, power and the default browser (TOOLS_AND_CONTROL §3, §5).
pub trait SystemControl: Send + Sync {
    fn volume(&self) -> PlatformResult<VolumeState>;
    fn set_volume(&self, level: f32) -> PlatformResult<()>;
    fn set_muted(&self, muted: bool) -> PlatformResult<()>;
    fn microphone(&self) -> PlatformResult<VolumeState>;
    fn set_mic_muted(&self, muted: bool) -> PlatformResult<()>;
    /// Sends a transport command to the app currently playing media.
    fn media(&self, action: MediaAction) -> PlatformResult<()>;
    fn now_playing(&self) -> PlatformResult<Option<NowPlaying>>;
    fn power(&self, action: PowerAction) -> PlatformResult<()>;
    /// Opens `url` (http or https only) in the default browser.
    fn open_url(&self, url: &str) -> PlatformResult<()>;
    /// Opens a page of the system's own settings (`privacy-microphone`, `sound`), for the
    /// "Open Windows settings" buttons KIVO offers when something is blocked (UX-57).
    fn open_system_settings(&self, page: &str) -> PlatformResult<()>;
    /// Shows a folder in the file manager ("Open folder" for crash reports, screenshots).
    fn reveal(&self, folder: &std::path::Path) -> PlatformResult<()>;
}

/// How the OS should run the calling thread (VOICE-03): efficiency mode (EcoQoS on Windows) for
/// a thread that only waits, full speed while it works on audio.
pub trait ThreadQos: Send + Sync {
    fn efficiency_mode(&self, on: bool);
}

/// Starting KIVO when the user signs in (ARCH-06, "Open KIVO when Windows starts").
pub trait Autostart: Send + Sync {
    /// Registers `command` to run at sign-in, or removes it.
    fn set(&self, enabled: bool, command: &str) -> PlatformResult<()>;
    /// The command registered now, if any.
    fn current(&self) -> PlatformResult<Option<String>>;
}
