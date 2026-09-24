//! What routines' event triggers look at (ROUT-11): the networks the PC is connected to, by the
//! names Windows shows (the Network List Manager, which needs no location permission, unlike
//! asking the Wi-Fi driver for the SSID), and the USB devices plugged in, by their friendly
//! names (SetupAPI).

use crate::com::Com;
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    DIGCF_ALLCLASSES, DIGCF_PRESENT, SP_DEVINFO_DATA, SPDRP_DEVICEDESC, SPDRP_FRIENDLYNAME,
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW,
    SetupDiGetDeviceRegistryPropertyW,
};
use windows::Win32::Networking::NetworkListManager::{
    INetworkListManager, NLM_ENUM_NETWORK_CONNECTED, NetworkListManager,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
use windows::core::w;

/// The names of the networks the PC is connected to now.
pub fn networks() -> Vec<String> {
    let Ok(_com) = Com::init() else {
        return Vec::new();
    };
    // SAFETY: COM is initialized on this thread for the lifetime of `_com`; the interfaces are
    // released when dropped, before it.
    unsafe {
        let Ok(nlm) =
            CoCreateInstance::<_, INetworkListManager>(&NetworkListManager, None, CLSCTX_ALL)
        else {
            return Vec::new();
        };
        let Ok(list) = nlm.GetNetworks(NLM_ENUM_NETWORK_CONNECTED) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        loop {
            let mut item = [None];
            let mut fetched = 0;
            if list.Next(&mut item, Some(&raw mut fetched)).is_err() || fetched == 0 {
                break;
            }
            if let Some(network) = item[0].take()
                && let Ok(name) = network.GetName()
            {
                let name = name.to_string();
                if !name.is_empty() && !out.contains(&name) {
                    out.push(name);
                }
            }
        }
        out
    }
}

/// The friendly names of the USB devices present now (hubs and controllers included; callers
/// look for the device they want by name).
pub fn usb_devices() -> Vec<String> {
    // SAFETY: the device list is destroyed on every path; buffers are sized as SetupAPI reports.
    unsafe {
        let Ok(set) = SetupDiGetClassDevsW(None, w!("USB"), None, DIGCF_PRESENT | DIGCF_ALLCLASSES)
        else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut index = 0;
        loop {
            let mut info = SP_DEVINFO_DATA {
                cbSize: u32::try_from(std::mem::size_of::<SP_DEVINFO_DATA>()).unwrap_or(0),
                ..Default::default()
            };
            if SetupDiEnumDeviceInfo(set, index, &raw mut info).is_err() {
                break;
            }
            index += 1;
            let name = [SPDRP_FRIENDLYNAME, SPDRP_DEVICEDESC]
                .into_iter()
                .find_map(|property| {
                    let mut buffer = [0u8; 512];
                    let mut size = 0;
                    SetupDiGetDeviceRegistryPropertyW(
                        set,
                        &raw const info,
                        property,
                        None,
                        Some(&mut buffer),
                        Some(&raw mut size),
                    )
                    .ok()?;
                    let wide: Vec<u16> = buffer[..usize::try_from(size).unwrap_or(0).min(512)]
                        .chunks_exact(2)
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .take_while(|c| *c != 0)
                        .collect();
                    let text = String::from_utf16_lossy(&wide).trim().to_owned();
                    (!text.is_empty()).then_some(text)
                });
            if let Some(name) = name {
                out.push(name);
            }
        }
        let _ = SetupDiDestroyDeviceInfoList(set);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Read-only: lists what's there. A PC with a network has at least one name; USB lists the
    /// root hubs at least on any PC with USB.
    #[test]
    fn reads_networks_and_usb_devices() {
        let nets = networks();
        let usb = usb_devices();
        println!(
            "networks: {nets:?}\nusb: {} devices, e.g. {:?}",
            usb.len(),
            usb.first()
        );
        assert!(nets.iter().all(|n| !n.is_empty()));
        assert!(!usb.is_empty(), "a PC has USB hubs");
    }
}
