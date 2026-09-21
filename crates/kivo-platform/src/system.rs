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
    pub gpus: Vec<GpuInfo>,
    pub on_battery: bool,
    pub battery_percent: Option<u8>,
    /// A fullscreen app or game is in front (UX §2: suppress the Island).
    pub fullscreen_app: bool,
    /// Windows Focus / Do not disturb (UX §7).
    pub focus_mode: bool,
}

pub trait SystemInfo: Send + Sync {
    fn snapshot(&self) -> PlatformResult<SystemSnapshot>;
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
}
