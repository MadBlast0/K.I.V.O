//! Brightness, battery and Focus (TOOL-13). Brightness is the built-in panel's, through WMI
//! (`root\WMI` `WmiMonitorBrightness`), which laptops and tablets support; desktops with external
//! monitors report `Unsupported`.

use crate::com::{Com, os_error};
use kivo_platform::{Battery, PlatformError, PlatformResult, Power};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, CoCreateInstance, CoSetProxyBlanket, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL,
    RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::Rpc::{RPC_C_AUTHN_WINNT, RPC_C_AUTHZ_NONE};
use windows::Win32::System::Variant::{VARIANT, VT_BSTR, VT_I4, VT_UI1};
use windows::Win32::System::Wmi::{
    IWbemClassObject, IWbemLocator, IWbemServices, WBEM_FLAG_FORWARD_ONLY,
    WBEM_FLAG_RETURN_IMMEDIATELY, WBEM_GENERIC_FLAG_TYPE, WBEM_INFINITE, WbemLocator,
};
use windows::core::{BSTR, w};

#[derive(Default)]
pub struct WindowsPower;

fn services() -> PlatformResult<IWbemServices> {
    // SAFETY: COM is initialized by the caller; plain WMI setup.
    unsafe {
        let locator: IWbemLocator =
            CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER).map_err(|e| os_error(&e))?;
        let services = locator
            .ConnectServer(
                &BSTR::from("ROOT\\WMI"),
                &BSTR::new(),
                &BSTR::new(),
                &BSTR::new(),
                0,
                &BSTR::new(),
                None,
            )
            .map_err(|e| os_error(&e))?;
        CoSetProxyBlanket(
            &services,
            RPC_C_AUTHN_WINNT,
            RPC_C_AUTHZ_NONE,
            None,
            RPC_C_AUTHN_LEVEL_CALL,
            RPC_C_IMP_LEVEL_IMPERSONATE,
            None,
            EOAC_NONE,
        )
        .map_err(|e| os_error(&e))?;
        Ok(services)
    }
}

/// The first object of a WQL query, or `Unsupported` when there is none.
fn first(services: &IWbemServices, query: &str) -> PlatformResult<IWbemClassObject> {
    // SAFETY: plain WMI calls on live objects.
    unsafe {
        let rows = services
            .ExecQuery(
                &BSTR::from("WQL"),
                &BSTR::from(query),
                WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
                None,
            )
            .map_err(|_| PlatformError::Unsupported)?;
        let mut row = [None];
        let mut n = 0u32;
        let _ = rows.Next(WBEM_INFINITE, &mut row, &raw mut n);
        row[0]
            .take()
            .filter(|_| n == 1)
            .ok_or(PlatformError::Unsupported)
    }
}

impl Power for WindowsPower {
    fn brightness(&self) -> PlatformResult<u8> {
        let _com = Com::init()?;
        let services = services()?;
        let row = first(
            &services,
            "SELECT CurrentBrightness FROM WmiMonitorBrightness WHERE Active=TRUE",
        )?;
        let mut value = VARIANT::default();
        // SAFETY: reading one property into a VARIANT we own.
        unsafe {
            row.Get(w!("CurrentBrightness"), 0, &raw mut value, None, None)
                .map_err(|e| os_error(&e))?;
            let v = &value.Anonymous.Anonymous;
            if v.vt == VT_UI1 {
                Ok(v.Anonymous.bVal.min(100))
            } else if v.vt == VT_I4 {
                Ok(u8::try_from(v.Anonymous.lVal.clamp(0, 100)).unwrap_or(0))
            } else {
                Err(PlatformError::Unsupported)
            }
        }
    }

    fn set_brightness(&self, percent: u8) -> PlatformResult<()> {
        let _com = Com::init()?;
        let services = services()?;
        let row = first(
            &services,
            "SELECT * FROM WmiMonitorBrightnessMethods WHERE Active=TRUE",
        )?;
        // SAFETY: plain WMI calls; every VARIANT is owned here.
        unsafe {
            let mut path = VARIANT::default();
            row.Get(w!("__PATH"), 0, &raw mut path, None, None)
                .map_err(|e| os_error(&e))?;
            if path.Anonymous.Anonymous.vt != VT_BSTR {
                return Err(PlatformError::Unsupported);
            }
            let path: BSTR = (*path.Anonymous.Anonymous.Anonymous.bstrVal).clone();
            let mut class = None;
            services
                .GetObject(
                    &BSTR::from("WmiMonitorBrightnessMethods"),
                    WBEM_GENERIC_FLAG_TYPE(0),
                    None,
                    Some(&raw mut class),
                    None,
                )
                .map_err(|e| os_error(&e))?;
            let class = class.ok_or(PlatformError::Unsupported)?;
            let mut input = None;
            class
                .GetMethod(
                    w!("WmiSetBrightness"),
                    0,
                    &raw mut input,
                    std::ptr::null_mut(),
                )
                .map_err(|e| os_error(&e))?;
            let params = input
                .ok_or(PlatformError::Unsupported)?
                .SpawnInstance(0)
                .map_err(|e| os_error(&e))?;
            let timeout = VARIANT::from(0i32);
            let level = VARIANT::from(percent.min(100));
            params
                .Put(w!("Timeout"), 0, &raw const timeout, 0)
                .map_err(|e| os_error(&e))?;
            params
                .Put(w!("Brightness"), 0, &raw const level, 0)
                .map_err(|e| os_error(&e))?;
            services
                .ExecMethod(
                    &path,
                    &BSTR::from("WmiSetBrightness"),
                    WBEM_GENERIC_FLAG_TYPE(0),
                    None,
                    &params,
                    None,
                    None,
                )
                .map_err(|e| os_error(&e))?;
        }
        Ok(())
    }

    fn battery(&self) -> PlatformResult<Option<Battery>> {
        let mut status = SYSTEM_POWER_STATUS::default();
        // SAFETY: a writable SYSTEM_POWER_STATUS.
        unsafe { GetSystemPowerStatus(&raw mut status) }.map_err(|e| os_error(&e))?;
        // BatteryFlag 128: no battery; 255: unknown.
        if status.BatteryFlag == 128 || status.BatteryLifePercent > 100 {
            return Ok(None);
        }
        Ok(Some(Battery {
            percent: status.BatteryLifePercent,
            charging: status.BatteryFlag & 8 != 0,
            on_battery: status.ACLineStatus == 0,
            seconds_left: (status.BatteryLifeTime != u32::MAX).then_some(status.BatteryLifeTime),
        }))
    }

    fn focus_on(&self) -> PlatformResult<bool> {
        use kivo_platform::SystemInfo;
        Ok(crate::WindowsSystemInfo.attention()?.focus_mode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reads only: tests never change the real brightness.
    #[test]
    fn brightness_is_a_percentage_or_unsupported() {
        match WindowsPower.brightness() {
            Ok(p) => assert!(p <= 100),
            Err(e) => assert_eq!(e, PlatformError::Unsupported),
        }
    }

    #[test]
    fn battery_is_consistent_when_present() {
        if let Some(b) = WindowsPower.battery().unwrap() {
            assert!(b.percent <= 100);
            if b.charging {
                assert!(!b.on_battery);
            }
        }
    }

    #[test]
    fn focus_state_reads() {
        WindowsPower.focus_on().unwrap();
    }
}
