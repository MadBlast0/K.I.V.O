//! File operations that need the shell (TOOL-16): the Windows Search index through the documented
//! `search-ms:` protocol, the Recycle Bin through `IFileOperation`, and open/reveal.

use crate::com::{Com, os_error};
use kivo_platform::{FileHit, FileOps, PlatformError, PlatformResult};
use std::path::{Path, PathBuf};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
use windows::Win32::UI::Shell::{
    BHID_EnumItems, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FOFX_RECYCLEONDELETE,
    FileOperation, IEnumShellItems, IFileOperation, ILFree, IShellItem,
    SHCreateItemFromParsingName, SHOpenFolderAndSelectItems, SHParseDisplayName, SIGDN_FILESYSPATH,
    ShellExecuteW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, PCWSTR, w};

#[derive(Default)]
pub struct WindowsFiles;

fn shell_item(path: &Path) -> PlatformResult<IShellItem> {
    // SAFETY: the string outlives the call.
    unsafe { SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None) }
        .map_err(|_| PlatformError::NotFound(path.display().to_string()))
}

fn hit(path: PathBuf) -> FileHit {
    let meta = std::fs::metadata(&path).ok();
    FileHit {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        is_dir: meta.as_ref().is_some_and(std::fs::Metadata::is_dir),
        modified: meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs()),
        size: meta
            .as_ref()
            .filter(|m| m.is_file())
            .map(std::fs::Metadata::len),
        path,
    }
}

/// `search-ms:` escapes: its values are URL-encoded.
fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(b));
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

impl FileOps for WindowsFiles {
    fn search_index(
        &self,
        query: &str,
        roots: &[PathBuf],
        limit: usize,
    ) -> PlatformResult<Vec<FileHit>> {
        let _com = Com::init()?;
        let mut out = Vec::new();
        for root in roots {
            // Only names: `System.FileName:~="…"` matches names containing the words.
            let uri = format!(
                "search-ms:query={}&crumb=location:{}",
                encode(&format!("System.FileName:~=\"{query}\"")),
                encode(&root.to_string_lossy())
            );
            // SAFETY: COM is initialized; the items are released when dropped.
            unsafe {
                let Ok(search) = SHCreateItemFromParsingName::<_, _, IShellItem>(
                    &HSTRING::from(uri.as_str()),
                    None,
                ) else {
                    return Err(PlatformError::Unsupported);
                };
                let items: IEnumShellItems = search
                    .BindToHandler(None, &BHID_EnumItems)
                    .map_err(|_| PlatformError::Unsupported)?;
                loop {
                    let mut item = [None];
                    let mut fetched = 0u32;
                    if items.Next(&mut item, Some(&raw mut fetched)).is_err() || fetched == 0 {
                        break;
                    }
                    let Some(item) = item[0].take() else { break };
                    if let Ok(name) = item.GetDisplayName(SIGDN_FILESYSPATH) {
                        let path = PathBuf::from(name.to_string().unwrap_or_default());
                        windows::Win32::System::Com::CoTaskMemFree(Some(name.0.cast()));
                        if !path.as_os_str().is_empty() {
                            out.push(hit(path));
                        }
                    }
                    if out.len() >= limit {
                        return Ok(out);
                    }
                }
            }
        }
        Ok(out)
    }

    fn recycle(&self, paths: &[PathBuf]) -> PlatformResult<()> {
        let _com = Com::init()?;
        // SAFETY: COM is initialized; plain IFileOperation calls on objects owned here.
        unsafe {
            let op: IFileOperation =
                CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(|e| os_error(&e))?;
            // Recycle, never delete; no UI (KIVO already confirmed with the user).
            op.SetOperationFlags(
                FOFX_RECYCLEONDELETE | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI,
            )
            .map_err(|e| os_error(&e))?;
            for path in paths {
                let item = shell_item(path)?;
                op.DeleteItem(&item, None).map_err(|e| os_error(&e))?;
            }
            op.PerformOperations().map_err(|e| os_error(&e))?;
            if op
                .GetAnyOperationsAborted()
                .map_err(|e| os_error(&e))?
                .as_bool()
            {
                return Err(PlatformError::Cancelled);
            }
        }
        Ok(())
    }

    fn open(&self, path: &Path) -> PlatformResult<()> {
        if !path.exists() {
            return Err(PlatformError::NotFound(path.display().to_string()));
        }
        let _com = Com::init()?;
        // SAFETY: the strings outlive the call.
        let result = unsafe {
            ShellExecuteW(
                None,
                w!("open"),
                &HSTRING::from(path.as_os_str()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        if result.0 as isize > 32 {
            Ok(())
        } else {
            Err(PlatformError::Os {
                code: result.0 as i64,
                message: "ShellExecuteW".into(),
            })
        }
    }

    fn reveal(&self, path: &Path) -> PlatformResult<()> {
        if !path.exists() {
            return Err(PlatformError::NotFound(path.display().to_string()));
        }
        let _com = Com::init()?;
        // SAFETY: the item list is freed after use.
        unsafe {
            let mut pidl = std::ptr::null_mut();
            SHParseDisplayName(
                &HSTRING::from(path.as_os_str()),
                None,
                &raw mut pidl,
                0,
                None,
            )
            .map_err(|e| os_error(&e))?;
            let result = SHOpenFolderAndSelectItems(pidl, None, 0);
            ILFree(Some(pidl));
            result.map_err(|e| os_error(&e))
        }
    }

    fn protected_roots(&self) -> Vec<PathBuf> {
        let mut roots: Vec<PathBuf> = [
            "SystemRoot",
            "ProgramFiles",
            "ProgramFiles(x86)",
            "ProgramW6432",
            "ProgramData",
            "APPDATA",
            "LOCALAPPDATA",
        ]
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .collect();
        if let Some(home) = std::env::var_os("USERPROFILE") {
            roots.push(PathBuf::from(home).join("AppData"));
        }
        roots.sort();
        roots.dedup();
        roots
    }

    fn user_folders(&self) -> Vec<PathBuf> {
        let Some(home) = std::env::var_os("USERPROFILE").map(PathBuf::from) else {
            return Vec::new();
        };
        [
            "Desktop",
            "Documents",
            "Downloads",
            "Pictures",
            "Music",
            "Videos",
        ]
        .iter()
        .map(|f| home.join(f))
        .filter(|p| p.is_dir())
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_roots_include_the_os_and_app_data() {
        let roots = WindowsFiles.protected_roots();
        let windir = PathBuf::from(std::env::var_os("SystemRoot").unwrap());
        assert!(roots.contains(&windir));
        assert!(roots.iter().any(|r| r.ends_with("AppData")));
    }

    #[test]
    fn recycling_a_temp_file_moves_it_out_of_its_folder() {
        // A file KIVO made itself in a temp folder: the Recycle Bin takes it, nothing else.
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("kivo-recycle-test.txt");
        std::fs::write(&file, "bye").unwrap();
        WindowsFiles.recycle(std::slice::from_ref(&file)).unwrap();
        assert!(!file.exists());
    }

    #[test]
    fn a_missing_file_is_not_found() {
        let e = WindowsFiles
            .recycle(&[PathBuf::from(r"C:\definitely\not\here.txt")])
            .unwrap_err();
        assert!(matches!(e, PlatformError::NotFound(_)));
    }

    #[test]
    fn search_ms_values_are_encoded() {
        assert_eq!(encode("a b\"c"), "a%20b%22c");
        assert_eq!(encode(r"C:\x"), "C%3A%5Cx");
    }
}
