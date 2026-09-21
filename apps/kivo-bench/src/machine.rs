//! The machine fingerprint every result carries (BENCHMARKS §1): CPU, GPU, RAM, OS build and
//! power source, so numbers from different PCs are never mixed up.

use kivo_platform::{GpuInfo, SystemInfo};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Machine {
    /// Short, file-name-safe name from the CPU, e.g. "amd-ryzen-7-6800h".
    pub id: String,
    pub cpu: String,
    pub logical_cpus: u32,
    pub ram_mb: u64,
    pub gpus: Vec<GpuInfo>,
    pub os: String,
    /// "AC" or "Battery 80%": laptops run slower on battery, so it matters.
    pub power: String,
}

impl Machine {
    pub fn detect() -> Result<Self, String> {
        #[cfg(windows)]
        {
            let snap = kivo_platform_windows::WindowsSystemInfo
                .snapshot()
                .map_err(|e| e.to_string())?;
            let caps = kivo_platform_windows::detect_capabilities();
            Ok(Self {
                id: slug(&snap.cpu_name),
                power: if snap.on_battery {
                    snap.battery_percent
                        .map_or_else(|| "Battery".into(), |p| format!("Battery {p}%"))
                } else {
                    "AC".into()
                },
                cpu: snap.cpu_name,
                logical_cpus: snap.logical_cpus,
                ram_mb: snap.ram_mb,
                gpus: snap.gpus,
                os: caps.os_version,
            })
        }
        #[cfg(not(windows))]
        {
            Err("kivo-bench measures Windows first; other platforms come with their ports".into())
        }
    }

    /// The same machine limited to `cores` physical cores, labelled as an emulation of the low
    /// tier (BENCHMARKS §2) so its numbers are never mistaken for a real low-end PC's.
    pub fn emulating_low_tier(mut self, cores: usize) -> Self {
        self.id = format!("{}-emulated-low", self.id);
        #[allow(clippy::cast_possible_truncation, reason = "a small core count")]
        {
            self.logical_cpus = cores as u32;
        }
        self.cpu = format!(
            "{} limited to {cores} cores (emulated low tier, approximate)",
            self.cpu
        );
        self
    }

    /// One line for reports.
    pub fn describe(&self) -> String {
        let gpus = if self.gpus.is_empty() {
            "no GPU".to_owned()
        } else {
            self.gpus
                .iter()
                .map(|g| format!("{} ({} MB)", g.name, g.vram_mb))
                .collect::<Vec<_>>()
                .join(" + ")
        };
        format!(
            "{} ({} threads), {} GB RAM, {gpus}, {}, {}",
            self.cpu,
            self.logical_cpus,
            (self.ram_mb + 512) / 1024,
            self.os,
            self.power
        )
    }
}

/// "AMD Ryzen 7 6800H with Radeon Graphics" → "amd-ryzen-7-6800h": the words up to the model
/// number, lower-case, joined by dashes.
fn slug(cpu: &str) -> String {
    let mut words = Vec::new();
    for word in cpu.split(|c: char| !c.is_ascii_alphanumeric()) {
        if word.is_empty() || matches!(word.to_ascii_lowercase().as_str(), "r" | "tm" | "cpu") {
            continue;
        }
        words.push(word.to_ascii_lowercase());
        // The model number (has a digit and a letter, or is long) ends the useful part.
        if word.len() >= 4 && word.chars().any(|c| c.is_ascii_digit()) {
            break;
        }
    }
    if words.is_empty() {
        "unknown".into()
    } else {
        words.join("-")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_names_become_short_file_names() {
        assert_eq!(
            slug("AMD Ryzen 7 6800H with Radeon Graphics"),
            "amd-ryzen-7-6800h"
        );
        assert_eq!(
            slug("Intel(R) Core(TM) i7-1165G7 @ 2.80GHz"),
            "intel-core-i7-1165g7"
        );
        assert_eq!(
            slug("Snapdragon(R) X Elite - X1E78100"),
            "snapdragon-x-elite-x1e78100"
        );
        assert_eq!(slug(""), "unknown");
    }
}
