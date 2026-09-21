use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OsFamily {
    Windows,
    MacOs,
    Linux,
}

/// What this machine and OS version can do, detected once at startup (ARCHITECTURE §2).
/// Version differences (Windows 10 vs 11) are decided here and nowhere else in core code.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub os: OsFamily,
    /// Human-readable, e.g. "Windows 11 24H2 (build 26200)".
    pub os_version: String,
    /// The OS build number (Windows) or 0 when not meaningful.
    pub os_build: u32,
    /// OS-provided acoustic echo cancellation (Windows 11 22621+, device permitting; VOICE §3).
    pub os_echo_cancellation: bool,
    /// Mica/acrylic window materials (Windows 11; UX §3).
    pub mica: bool,
    /// The Windows AI Speech APIs need package identity (DISTRIBUTION §1); false until MSIX.
    pub package_identity: bool,
    /// A neural processing unit that ONNX Runtime can target.
    pub npu: bool,
}

impl Capabilities {
    /// Windows 11 is build 22000 and later.
    pub fn is_windows_11(&self) -> bool {
        self.os == OsFamily::Windows && self.os_build >= 22000
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn windows(build: u32) -> Capabilities {
        Capabilities {
            os: OsFamily::Windows,
            os_version: String::new(),
            os_build: build,
            os_echo_cancellation: false,
            mica: false,
            package_identity: false,
            npu: false,
        }
    }

    #[test]
    fn windows_11_starts_at_build_22000() {
        assert!(!windows(19045).is_windows_11());
        assert!(windows(22000).is_windows_11());
        assert!(windows(26200).is_windows_11());
    }
}
