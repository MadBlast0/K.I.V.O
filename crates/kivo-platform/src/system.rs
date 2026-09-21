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
