//! Authenticode checks with `WinVerifyTrust` (SEC-32): the whole chain to a trusted root, with
//! revocation checked, then the signer's name from the leaf certificate.

use kivo_platform::{CodeTrust, PlatformResult};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::Security::Cryptography::{CERT_NAME_SIMPLE_DISPLAY_TYPE, CertGetNameStringW};
use windows::Win32::Security::WinTrust::{
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_FILE_INFO, WTD_CHOICE_FILE,
    WTD_REVOKE_WHOLECHAIN, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE,
    WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData, WinVerifyTrust,
};
use windows::core::PCWSTR;

#[derive(Default)]
pub struct WindowsCodeTrust;

impl CodeTrust for WindowsCodeTrust {
    fn signer(&self, path: &Path) -> PlatformResult<Option<String>> {
        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut file = WINTRUST_FILE_INFO {
            cbStruct: u32::try_from(std::mem::size_of::<WINTRUST_FILE_INFO>()).unwrap_or(0),
            pcwszFilePath: PCWSTR(wide.as_ptr()),
            ..Default::default()
        };
        let mut data = WINTRUST_DATA {
            cbStruct: u32::try_from(std::mem::size_of::<WINTRUST_DATA>()).unwrap_or(0),
            dwUIChoice: WTD_UI_NONE,
            fdwRevocationChecks: WTD_REVOKE_WHOLECHAIN,
            dwUnionChoice: WTD_CHOICE_FILE,
            dwStateAction: WTD_STATEACTION_VERIFY,
            ..Default::default()
        };
        data.Anonymous.pFile = &raw mut file;
        let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
        // SAFETY: `data` and `file` outlive both calls; the state is closed below.
        let status = unsafe {
            WinVerifyTrust(
                HWND(std::ptr::null_mut()),
                &raw mut action,
                (&raw mut data).cast(),
            )
        };
        let name = if status == 0 {
            // SAFETY: the provider data belongs to the open state; the leaf certificate is read
            // before the state is closed.
            unsafe { leaf_name(data.hWVTStateData) }
        } else {
            tracing::debug!(
                status = format_args!("{status:#010x}"),
                "no valid signature"
            );
            None
        };
        data.dwStateAction = WTD_STATEACTION_CLOSE;
        // SAFETY: closes the state opened above.
        unsafe {
            WinVerifyTrust(
                HWND(std::ptr::null_mut()),
                &raw mut action,
                (&raw mut data).cast(),
            );
        }
        Ok(name)
    }
}

/// The display name of the first signer's leaf certificate.
///
/// # Safety
/// `state` must be an open `WinVerifyTrust` state.
unsafe fn leaf_name(state: HANDLE) -> Option<String> {
    unsafe {
        let provider = WTHelperProvDataFromStateData(state);
        if provider.is_null() {
            return None;
        }
        let signer = WTHelperGetProvSignerFromChain(provider, 0, false, 0);
        if signer.is_null() || (*signer).csCertChain == 0 || (*signer).pasCertChain.is_null() {
            return None;
        }
        let cert = (*(*signer).pasCertChain).pCert;
        if cert.is_null() {
            return None;
        }
        let len = CertGetNameStringW(cert, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, None, None);
        if len <= 1 {
            return None;
        }
        let mut buf = vec![0u16; len as usize];
        CertGetNameStringW(cert, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, None, Some(&mut buf));
        Some(String::from_utf16_lossy(&buf[..buf.len() - 1]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Microsoft-signed program every Windows 11 PC with WebView2 has.
    fn signed_program() -> Option<std::path::PathBuf> {
        let x86 = std::env::var_os("ProgramFiles(x86)").map(std::path::PathBuf::from)?;
        let edge = x86.join(r"Microsoft\Edge\Application\msedge.exe");
        if edge.is_file() {
            return Some(edge);
        }
        let webview = x86.join(r"Microsoft\EdgeWebView\Application");
        std::fs::read_dir(webview)
            .ok()?
            .flatten()
            .map(|e| e.path().join("msedgewebview2.exe"))
            .find(|p| p.is_file())
    }

    #[test]
    fn a_signed_program_names_its_publisher_and_an_altered_copy_doesnt() {
        let Some(program) = signed_program() else {
            eprintln!("no Edge or WebView2 on this PC; skipping");
            return;
        };
        let trust = WindowsCodeTrust;
        let signer = trust.signer(&program).unwrap();
        assert_eq!(
            signer.as_deref(),
            Some("Microsoft Corporation"),
            "{program:?}"
        );

        // One byte changed in the middle: the signature no longer matches.
        let dir = tempfile::tempdir().unwrap();
        let copy = dir.path().join("altered.exe");
        let mut bytes = std::fs::read(&program).unwrap();
        let middle = bytes.len() / 2;
        bytes[middle] ^= 0xFF;
        std::fs::write(&copy, bytes).unwrap();
        assert_eq!(trust.signer(&copy).unwrap(), None);
    }

    #[test]
    fn an_unsigned_program_has_no_signer() {
        // This test binary is built here and never signed.
        let me = std::env::current_exe().unwrap();
        assert_eq!(WindowsCodeTrust.signer(&me).unwrap(), None);
        assert_eq!(
            WindowsCodeTrust
                .signer(Path::new(r"C:\no\such\file.exe"))
                .unwrap(),
            None
        );
    }
}
