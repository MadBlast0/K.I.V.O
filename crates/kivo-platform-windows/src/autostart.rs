//! "Open KIVO when Windows starts" (ARCH-06, UX §5): a value under the user's own Run key, the
//! same place Windows' Startup apps page lists and toggles. No admin rights needed.

use crate::com::os_error;
use kivo_platform::{Autostart, PlatformResult};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW, RegSetValueExW,
};
use windows::core::{HSTRING, PCWSTR, w};

const RUN: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");

pub struct WindowsAutostart {
    /// The value name under the Run key ("KIVO"; tests use their own).
    name: HSTRING,
}

impl Default for WindowsAutostart {
    fn default() -> Self {
        Self::named("KIVO")
    }
}

impl WindowsAutostart {
    pub fn named(name: &str) -> Self {
        Self {
            name: HSTRING::from(name),
        }
    }
}

fn open_run(access: windows::Win32::System::Registry::REG_SAM_FLAGS) -> PlatformResult<HKEY> {
    let mut key = HKEY::default();
    // SAFETY: opens (or creates) the per-user Run key; the caller closes it.
    unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            RUN,
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            access,
            None,
            &mut key,
            None,
        )
        .ok()
        .map_err(|e| os_error(&e))?;
    }
    Ok(key)
}

impl Autostart for WindowsAutostart {
    fn set(&self, enabled: bool, command: &str) -> PlatformResult<()> {
        let key = open_run(KEY_WRITE)?;
        // SAFETY: `key` is open for writing; the value data is a NUL-terminated UTF-16 string.
        let result = unsafe {
            if enabled {
                let wide: Vec<u16> = command.encode_utf16().chain(Some(0)).collect();
                let bytes = std::slice::from_raw_parts(wide.as_ptr().cast::<u8>(), wide.len() * 2);
                RegSetValueExW(key, &self.name, None, REG_SZ, Some(bytes)).ok()
            } else {
                // Already absent is fine.
                let _ = RegDeleteValueW(key, &self.name);
                Ok(())
            }
        };
        // SAFETY: opened above.
        let _ = unsafe { RegCloseKey(key) };
        result.map_err(|e| os_error(&e))
    }

    fn current(&self) -> PlatformResult<Option<String>> {
        let key = open_run(KEY_READ)?;
        let mut buf = vec![0u16; 1024];
        let mut bytes = u32::try_from(buf.len() * 2).unwrap_or(0);
        // SAFETY: `buf` is writable for `bytes` bytes.
        let status = unsafe {
            RegGetValueW(
                key,
                PCWSTR::null(),
                &self.name,
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&raw mut bytes),
            )
        };
        // SAFETY: opened above.
        let _ = unsafe { RegCloseKey(key) };
        if status.is_err() {
            return Ok(None);
        }
        let len = (bytes as usize / 2).saturating_sub(1);
        Ok(Some(String::from_utf16_lossy(&buf[..len])))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_startup_entry_is_written_read_and_removed() {
        // A test-only name, so the user's own KIVO entry is never touched.
        let autostart = WindowsAutostart::named("KIVO-test-autostart");
        let command = r#""C:\Program Files\KIVO\kivo-runtime.exe" --autostart"#;
        autostart.set(true, command).unwrap();
        assert_eq!(autostart.current().unwrap().as_deref(), Some(command));
        autostart.set(false, command).unwrap();
        assert_eq!(autostart.current().unwrap(), None);
        autostart.set(false, command).unwrap();
    }
}
