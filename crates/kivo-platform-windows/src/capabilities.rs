//! Startup capability detection (ARCHITECTURE §2, ARCH-12).

use kivo_platform::{Capabilities, OsFamily};
use windows::Wdk::System::SystemServices::RtlGetVersion;
use windows::Win32::Foundation::APPMODEL_ERROR_NO_PACKAGE;
use windows::Win32::Graphics::DXCore::{
    DXCoreCreateAdapterFactory, IDXCoreAdapter, IDXCoreAdapterFactory, IDXCoreAdapterList,
};
use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
use windows::Win32::System::SystemInformation::OSVERSIONINFOW;
use windows::core::{GUID, w};

/// First Windows 11 build.
const WINDOWS_11: u32 = 22000;
/// Windows 11 22H2: first build with the OS echo-cancellation API (VOICE §3). Whether a given
/// device exposes it is checked when the capture stream opens.
const OS_AEC: u32 = 22621;

// From dxcore_interface.h (Windows SDK 10.0.26100); not yet in the `windows` crate.
const DXCORE_ADAPTER_ATTRIBUTE_D3D12_GENERIC_ML: GUID =
    GUID::from_u128(0xb71b0d41_1088_422f_a27c_0250b7d3a988);
const DXCORE_HARDWARE_TYPE_ATTRIBUTE_NPU: GUID =
    GUID::from_u128(0xd46140c4_add7_451b_9e56_06fe8c3b58ed);

/// Detects what this machine can do. Never fails: anything that can't be determined is reported
/// as unavailable, so the app falls back instead of refusing to start.
pub fn detect() -> Capabilities {
    let build = os_build();
    let display = display_version();
    let name = if build >= WINDOWS_11 {
        "Windows 11"
    } else {
        "Windows 10"
    };
    Capabilities {
        os: OsFamily::Windows,
        os_version: match display {
            Some(v) => format!("{name} {v} (build {build})"),
            None => format!("{name} (build {build})"),
        },
        os_build: build,
        os_echo_cancellation: build >= OS_AEC,
        mica: build >= WINDOWS_11,
        package_identity: has_package_identity(),
        npu: has_npu(),
    }
}

/// The real build number. `RtlGetVersion` isn't subject to the compatibility shims that make
/// `GetVersionEx` report older versions.
fn os_build() -> u32 {
    let mut info = OSVERSIONINFOW {
        dwOSVersionInfoSize: u32::try_from(size_of::<OSVERSIONINFOW>()).unwrap_or(0),
        ..Default::default()
    };
    // SAFETY: `info` is a properly sized, writable OSVERSIONINFOW with its size field set.
    let status = unsafe { RtlGetVersion(&raw mut info) };
    if status.is_ok() {
        info.dwBuildNumber
    } else {
        0
    }
}

/// The marketing version, e.g. "24H2".
fn display_version() -> Option<String> {
    let mut buf = [0u16; 32];
    let mut bytes = u32::try_from(size_of_val(&buf)).ok()?;
    // SAFETY: `buf` is writable for `bytes` bytes; RRF_RT_REG_SZ guarantees a terminated string.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion"),
            w!("DisplayVersion"),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&raw mut bytes),
        )
    };
    if status.is_err() {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len])).filter(|s| !s.is_empty())
}

/// True when running as a packaged (MSIX or sparse) app (DISTRIBUTION §1).
fn has_package_identity() -> bool {
    let mut len = 0u32;
    // SAFETY: querying the length only (no buffer), as documented.
    let status = unsafe { GetCurrentPackageFullName(&raw mut len, None) };
    status != APPMODEL_ERROR_NO_PACKAGE
}

/// True when DXCore reports an ML-capable adapter whose hardware type is NPU.
fn has_npu() -> bool {
    let find = || -> windows::core::Result<bool> {
        // SAFETY: plain COM calls on interfaces DXCore returned; indexes stay below the count.
        unsafe {
            let factory: IDXCoreAdapterFactory = DXCoreCreateAdapterFactory()?;
            let list: IDXCoreAdapterList =
                factory.CreateAdapterList(&[DXCORE_ADAPTER_ATTRIBUTE_D3D12_GENERIC_ML])?;
            for i in 0..list.GetAdapterCount() {
                let adapter: IDXCoreAdapter = list.GetAdapter(i)?;
                if adapter.IsAttributeSupported(&DXCORE_HARDWARE_TYPE_ATTRIBUTE_NPU) {
                    return Ok(true);
                }
            }
            Ok(false)
        }
    };
    // DXCore is missing on older Windows 10 builds; that simply means no NPU.
    find().unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_this_machine() {
        let caps = detect();
        println!("{caps:#?}");
        assert_eq!(caps.os, OsFamily::Windows);
        assert!(
            caps.os_build >= 10240,
            "any supported Windows 10 or 11 build"
        );
        assert!(caps.os_version.contains(&caps.os_build.to_string()));
        assert_eq!(caps.mica, caps.is_windows_11());
        // A test binary is never a packaged app.
        assert!(!caps.package_identity);
    }

    #[test]
    fn reads_the_marketing_version_on_windows_11() {
        if os_build() >= WINDOWS_11 {
            let v = display_version().expect("DisplayVersion exists on Windows 11");
            assert!(v.len() == 4 && v.contains('H'), "looks like 24H2: {v}");
        }
    }
}
