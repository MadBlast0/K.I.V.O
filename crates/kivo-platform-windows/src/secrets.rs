//! Data protection on Windows (SECURITY §5): DPAPI in the CurrentUser scope, so only this user
//! on this PC can read what KIVO encrypts (voice enrollment clips and the speaker profile, VOICE
//! §5). Named secrets (API keys in Credential Manager) arrive with the brains (SEC-17, M3).

use kivo_platform::{PlatformError, PlatformResult, Secret, SecretHandle, Secrets};
use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};

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

impl Secrets for WindowsSecrets {
    fn get(&self, _handle: &SecretHandle) -> PlatformResult<Option<Secret<String>>> {
        Err(PlatformError::Unsupported)
    }

    fn set(&self, _handle: &SecretHandle, _value: Secret<String>) -> PlatformResult<()> {
        Err(PlatformError::Unsupported)
    }

    fn delete(&self, _handle: &SecretHandle) -> PlatformResult<()> {
        Err(PlatformError::Unsupported)
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
