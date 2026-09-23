//! Data protection on Windows (SECURITY §5): DPAPI in the CurrentUser scope, so only this user
//! on this PC can read what KIVO encrypts (voice enrollment clips and the speaker profile, VOICE
//! §5). Named secrets — API keys and OAuth-issued keys — live in Windows Credential Manager as
//! generic credentials named `kivo/<provider>/<name>` (SEC-17); config holds only the
//! `secret://kivo/…` handle. Known limitation (SECURITY §5): other programs running as the same
//! user can read these entries.

use kivo_platform::{PlatformError, PlatformResult, Secret, SecretHandle, Secrets};
use windows::Win32::Foundation::{ERROR_NOT_FOUND, HLOCAL, LocalFree};
use windows::Win32::Security::Credentials::{
    CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree, CredReadW,
    CredWriteW,
};
use windows::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};
use windows::core::{HSTRING, PWSTR};
use zeroize::Zeroize;

#[derive(Default)]
pub struct WindowsSecrets;

/// Copies a DPAPI output blob and frees it.
///
/// # Safety
/// `blob` must come from a successful `CryptProtectData`/`CryptUnprotectData` call.
unsafe fn take(blob: &CRYPT_INTEGER_BLOB) -> Vec<u8> {
    let len = usize::try_from(blob.cbData).unwrap_or(0);
    // SAFETY: DPAPI returned `cbData` bytes at `pbData`, freed with LocalFree once copied.
    let out = unsafe { std::slice::from_raw_parts(blob.pbData, len) }.to_vec();
    unsafe {
        let _ = LocalFree(Some(HLOCAL(blob.pbData.cast())));
    }
    out
}

fn os(e: &windows::core::Error, what: &str) -> PlatformError {
    PlatformError::Os {
        code: i64::from(e.code().0),
        message: format!("couldn't {what}: {e}"),
    }
}

fn input(data: &[u8]) -> PlatformResult<CRYPT_INTEGER_BLOB> {
    Ok(CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(data.len()).map_err(|_| PlatformError::Os {
            code: 0,
            message: "the data is too large to protect".into(),
        })?,
        pbData: data.as_ptr().cast_mut(),
    })
}

/// The Credential Manager name for a handle.
fn target(handle: &SecretHandle) -> HSTRING {
    HSTRING::from(format!("kivo/{}/{}", handle.provider, handle.name))
}

impl Secrets for WindowsSecrets {
    fn get(&self, handle: &SecretHandle) -> PlatformResult<Option<Secret<String>>> {
        let name = target(handle);
        let mut found: *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: `found` receives a buffer owned by Credential Manager, freed with CredFree.
        let read = unsafe { CredReadW(&name, CRED_TYPE_GENERIC, None, &mut found) };
        if let Err(e) = read {
            if e.code() == ERROR_NOT_FOUND.to_hresult() {
                return Ok(None);
            }
            return Err(os(&e, "read a saved key"));
        }
        // SAFETY: CredReadW succeeded, so `found` points at a valid CREDENTIALW.
        let value = unsafe {
            let cred = &*found;
            let len = usize::try_from(cred.CredentialBlobSize).unwrap_or(0);
            let mut bytes = std::slice::from_raw_parts(cred.CredentialBlob, len).to_vec();
            CredFree(found.cast());
            let text = String::from_utf8(bytes.clone()).unwrap_or_default();
            bytes.zeroize();
            text
        };
        Ok(Some(Secret::new(value)))
    }

    fn set(&self, handle: &SecretHandle, value: Secret<String>) -> PlatformResult<()> {
        let name = target(handle);
        let mut wide: Vec<u16> = name
            .to_string_lossy()
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut blob = value.expose().as_bytes().to_vec();
        let cred = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: PWSTR(wide.as_mut_ptr()),
            CredentialBlobSize: u32::try_from(blob.len()).map_err(|_| PlatformError::Os {
                code: 0,
                message: "the key is too long".into(),
            })?,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            ..Default::default()
        };
        // SAFETY: every pointer in `cred` outlives the call.
        let written = unsafe { CredWriteW(&cred, 0) };
        blob.zeroize();
        written.map_err(|e| os(&e, "save the key"))
    }

    fn delete(&self, handle: &SecretHandle) -> PlatformResult<()> {
        let name = target(handle);
        // SAFETY: plain call with a valid string.
        match unsafe { CredDeleteW(&name, CRED_TYPE_GENERIC, None) } {
            Ok(()) => Ok(()),
            Err(e) if e.code() == ERROR_NOT_FOUND.to_hresult() => Ok(()),
            Err(e) => Err(os(&e, "remove the key")),
        }
    }

    fn protect(&self, data: &[u8]) -> PlatformResult<Vec<u8>> {
        let blob = input(data)?;
        let mut out = CRYPT_INTEGER_BLOB::default();
        // SAFETY: the input blob borrows `data` for the call; the output is freed by `take`.
        unsafe {
            CryptProtectData(
                &blob,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
            .map_err(|e| os(&e, "encrypt"))?;
            Ok(take(&out))
        }
    }

    fn unprotect(&self, data: &[u8]) -> PlatformResult<Vec<u8>> {
        let blob = input(data)?;
        let mut out = CRYPT_INTEGER_BLOB::default();
        // SAFETY: as in `protect`.
        unsafe {
            CryptUnprotectData(
                &blob,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
            .map_err(|e| os(&e, "decrypt"))?;
            Ok(take(&out))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SEC-17: a key goes into Credential Manager and comes back; deleting it twice is fine.
    /// Uses its own unique entry and removes it.
    #[test]
    fn keys_round_trip_through_credential_manager() {
        let secrets = WindowsSecrets;
        let handle = SecretHandle {
            provider: format!("kivo-test-{}", std::process::id()),
            name: "api-key".into(),
        };
        assert!(secrets.get(&handle).unwrap().is_none());
        secrets
            .set(&handle, Secret::new("sk-test-not-real-123".to_owned()))
            .unwrap();
        let back = secrets.get(&handle).unwrap().expect("saved");
        assert_eq!(back.expose(), "sk-test-not-real-123");
        secrets.delete(&handle).unwrap();
        assert!(secrets.get(&handle).unwrap().is_none());
        secrets.delete(&handle).unwrap();
    }

    #[test]
    fn data_round_trips_and_is_not_stored_in_the_clear() {
        let secrets = WindowsSecrets;
        let voice = b"a voiceprint is biometric data".repeat(20);
        let sealed = secrets.protect(&voice).unwrap();
        assert_ne!(sealed, voice);
        assert!(!sealed.windows(9).any(|w| w == b"biometric"));
        assert_eq!(secrets.unprotect(&sealed).unwrap(), voice);
        let mut damaged = sealed;
        let last = damaged.len() - 1;
        damaged[last] ^= 0xff;
        assert!(secrets.unprotect(&damaged).is_err());
    }
}
