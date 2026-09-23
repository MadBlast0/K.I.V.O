//! Whether the user is here and free to be spoken to (UX §7, UX-40): how long since the last
//! input, whether the session is locked, and whether another app is using the microphone (a
//! call), read from Windows' own privacy bookkeeping of microphone use.

use kivo_platform::Presence;
use windows::Win32::Foundation::{ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, RRF_RT_REG_QWORD, RegCloseKey, RegEnumKeyExW, RegGetValueW,
    RegOpenKeyExW,
};
use windows::Win32::System::StationsAndDesktops::{
    CloseDesktop, DESKTOP_ACCESS_FLAGS, DESKTOP_CONTROL_FLAGS, OpenInputDesktop,
};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::core::{HSTRING, PCWSTR};

/// Where Windows records which apps use the microphone and when (Settings → Privacy → Microphone
/// reads the same keys).
const MIC_STORE: &str =
    r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";

pub fn presence() -> Presence {
    Presence {
        idle_seconds: idle_seconds(),
        locked: locked(),
        mic_in_use_elsewhere: mic_in_use_elsewhere(),
    }
}

fn idle_seconds() -> u32 {
    let mut info = LASTINPUTINFO {
        cbSize: u32::try_from(std::mem::size_of::<LASTINPUTINFO>()).unwrap_or(0),
        dwTime: 0,
    };
    // SAFETY: `info` has its size set.
    if !unsafe { GetLastInputInfo(&raw mut info) }.as_bool() {
        return 0;
    }
    // SAFETY: no arguments. Both are milliseconds since boot and wrap together.
    let now = unsafe { GetTickCount() };
    now.wrapping_sub(info.dwTime) / 1_000
}

/// The input desktop can't be opened while the lock screen (the secure desktop) is up.
fn locked() -> bool {
    // SAFETY: opened with no rights beyond enumeration, closed right away.
    match unsafe { OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_ACCESS_FLAGS(0)) } {
        Ok(desk) => {
            // SAFETY: the handle just opened.
            let _ = unsafe { CloseDesktop(desk) };
            false
        }
        Err(_) => true,
    }
}

struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: an open key, closed once.
        let _ = unsafe { RegCloseKey(self.0) };
    }
}

fn open(parent: HKEY, path: &str) -> Option<Key> {
    let mut key = HKEY::default();
    let path = HSTRING::from(path);
    // SAFETY: a read-only open of a key under HKCU.
    let r = unsafe { RegOpenKeyExW(parent, &path, None, KEY_READ, &raw mut key) };
    (r == ERROR_SUCCESS).then_some(Key(key))
}

fn subkeys(key: &Key) -> Vec<String> {
    let mut out = Vec::new();
    for i in 0.. {
        let mut name = [0u16; 512];
        let mut len = u32::try_from(name.len()).unwrap_or(0);
        // SAFETY: `name` holds `len` characters.
        let r: WIN32_ERROR = unsafe {
            RegEnumKeyExW(
                key.0,
                i,
                Some(windows::core::PWSTR(name.as_mut_ptr())),
                &raw mut len,
                None,
                None,
                None,
                None,
            )
        };
        if r != ERROR_SUCCESS {
            break;
        }
        out.push(String::from_utf16_lossy(&name[..len as usize]));
    }
    out
}

fn qword(key: &Key, sub: &str, value: &str) -> Option<u64> {
    let mut data = 0u64;
    let mut size = 8u32;
    let sub = HSTRING::from(sub);
    let value = HSTRING::from(value);
    // SAFETY: an 8-byte buffer for a REG_QWORD.
    let r = unsafe {
        RegGetValueW(
            key.0,
            PCWSTR(sub.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_QWORD,
            None,
            Some((&raw mut data).cast()),
            Some(&raw mut size),
        )
    };
    (r == ERROR_SUCCESS).then_some(data)
}

/// An app other than KIVO has the microphone open now: its last use started and hasn't stopped.
fn mic_in_use_elsewhere() -> bool {
    let in_use = |key: &Key, sub: &str| {
        !sub.to_lowercase().contains("kivo")
            && qword(key, sub, "LastUsedTimeStart").is_some_and(|s| s > 0)
            && qword(key, sub, "LastUsedTimeStop") == Some(0)
    };
    let Some(store) = open(HKEY_CURRENT_USER, MIC_STORE) else {
        return false;
    };
    // Store apps are direct subkeys; desktop apps are under NonPackaged, named by their path.
    let packaged = subkeys(&store)
        .into_iter()
        .filter(|s| s != "NonPackaged")
        .any(|s| in_use(&store, &s));
    if packaged {
        return true;
    }
    open(store.0, "NonPackaged")
        .is_some_and(|desktop| subkeys(&desktop).into_iter().any(|s| in_use(&desktop, &s)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presence_reads_on_this_pc() {
        let p = presence();
        println!("{p:?}");
        // Readable, whatever the user is doing (a long test run may even outlast a lock).
        assert!(p.idle_seconds < 30 * 24 * 3600);
    }
}
