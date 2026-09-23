//! Registering KIVO's native messaging host with Chrome, Edge and Brave (TOOL-24, SEC-30): a
//! manifest file that names the runtime and allows exactly one caller, KIVO's own extension, and
//! a per-user registry key per browser pointing at it. Nothing is registered machine-wide.

use std::io;
use std::path::{Path, PathBuf};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
    RegCreateKeyExW, RegDeleteTreeW, RegSetValueExW,
};
use windows::core::HSTRING;

/// Where each Chromium browser looks for native messaging hosts, under HKCU.
pub const BROWSER_KEYS: &[&str] = &[
    r"Software\Google\Chrome\NativeMessagingHosts",
    r"Software\Microsoft\Edge\NativeMessagingHosts",
    r"Software\BraveSoftware\Brave-Browser\NativeMessagingHosts",
];

/// The host manifest: the program, and the one extension allowed to start it.
pub fn manifest(name: &str, description: &str, program: &Path, origin: &str) -> String {
    serde_like_json(name, description, program, origin)
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out
}

fn serde_like_json(name: &str, description: &str, program: &Path, origin: &str) -> String {
    format!(
        "{{\n  \"name\": \"{}\",\n  \"description\": \"{}\",\n  \"path\": \"{}\",\n  \"type\": \"stdio\",\n  \"allowed_origins\": [\"{}\"]\n}}\n",
        escape(name),
        escape(description),
        escape(&program.to_string_lossy()),
        escape(origin)
    )
}

fn os(e: &windows::core::Error) -> io::Error {
    io::Error::other(e.message())
}

fn set_default(key_path: &str, value: &str) -> io::Result<()> {
    let mut key = HKEY::default();
    // SAFETY: HKCU subkey created/opened here and closed below; the value buffer outlives the call.
    unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(key_path),
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &raw mut key,
            None,
        )
        .ok()
        .map_err(|e| os(&e))?;
        let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes = std::slice::from_raw_parts(wide.as_ptr().cast::<u8>(), wide.len() * 2);
        let result = RegSetValueExW(key, None, None, REG_SZ, Some(bytes)).ok();
        let _ = RegCloseKey(key);
        result.map_err(|e| os(&e))
    }
}

fn delete_tree(key_path: &str) -> io::Result<()> {
    // SAFETY: deletes only this HKCU subkey.
    let result = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(key_path)) };
    // ERROR_FILE_NOT_FOUND: nothing to remove.
    if result.is_ok() || result.0 == 2 {
        Ok(())
    } else {
        Err(os(&result.ok().unwrap_err()))
    }
}

/// Writes the manifest to `manifest_path` and points each browser key (under `roots`, relative
/// to HKCU) at it.
pub fn register(
    roots: &[&str],
    name: &str,
    manifest_path: &Path,
    manifest_json: &str,
) -> io::Result<()> {
    if let Some(dir) = manifest_path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(manifest_path, manifest_json)?;
    for root in roots {
        set_default(&format!(r"{root}\{name}"), &manifest_path.to_string_lossy())?;
    }
    Ok(())
}

/// Removes the browser keys and the manifest.
pub fn unregister(roots: &[&str], name: &str, manifest_path: &Path) -> io::Result<()> {
    for root in roots {
        delete_tree(&format!(r"{root}\{name}"))?;
    }
    match std::fs::remove_file(manifest_path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// The registered manifest path for `name` under `root`, if any.
pub fn registered(root: &str, name: &str) -> Option<PathBuf> {
    use windows::Win32::System::Registry::{RRF_RT_REG_SZ, RegGetValueW};
    let mut buf = vec![0u16; 1024];
    let mut len = u32::try_from(buf.len() * 2).unwrap_or(0);
    // SAFETY: the buffer and its size are passed together.
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(format!(r"{root}\{name}")),
            None,
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&raw mut len),
        )
    };
    if result.is_err() {
        return None;
    }
    let chars = (len as usize / 2).saturating_sub(1);
    Some(PathBuf::from(String::from_utf16_lossy(&buf[..chars])))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_manifest_allows_only_one_origin() {
        let m = manifest(
            "com.kivo.bridge",
            "KIVO",
            Path::new(r"C:\Program Files\KIVO\kivo-runtime.exe"),
            "chrome-extension://abc/",
        );
        let v: serde_json::Value = serde_json::from_str(&m).unwrap();
        assert_eq!(v["type"], "stdio");
        assert_eq!(v["path"], r"C:\Program Files\KIVO\kivo-runtime.exe");
        assert_eq!(
            v["allowed_origins"],
            serde_json::json!(["chrome-extension://abc/"])
        );
    }

    #[test]
    fn registration_round_trips_under_a_test_key() {
        // A test-only key under HKCU\Software\KIVO-test, removed at the end; the browsers' own
        // keys are never touched by tests.
        let root = r"Software\KIVO-test\NativeMessagingHosts";
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("com.kivo.test.json");
        register(&[root], "com.kivo.test", &path, "{}").unwrap();
        assert_eq!(
            registered(root, "com.kivo.test").as_deref(),
            Some(path.as_path())
        );
        unregister(&[root], "com.kivo.test", &path).unwrap();
        assert!(registered(root, "com.kivo.test").is_none());
        assert!(!path.exists());
        delete_tree(r"Software\KIVO-test").unwrap();
    }
}
