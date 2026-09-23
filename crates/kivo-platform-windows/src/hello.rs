//! Windows Hello (SEC-11, CONV-29): `UserConsentVerifier` confirms it's the signed-in user (face,
//! fingerprint or PIN) before a High-risk action. The prompt is parented to the foreground window
//! through `IUserConsentVerifierInterop`, as desktop apps must.

use kivo_platform::{PlatformError, PlatformResult, UserVerifier};
use windows::Security::Credentials::UI::{
    UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability,
};
use windows::Win32::System::WinRT::IUserConsentVerifierInterop;
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
use windows::core::{HSTRING, factory};

#[derive(Default)]
pub struct WindowsHello;

impl UserVerifier for WindowsHello {
    fn available(&self) -> bool {
        UserConsentVerifier::CheckAvailabilityAsync()
            .and_then(|op| op.join())
            .is_ok_and(|a| a == UserConsentVerifierAvailability::Available)
    }

    fn verify(&self, message: &str) -> PlatformResult<bool> {
        if !self.available() {
            return Err(PlatformError::Unsupported);
        }
        let interop = factory::<UserConsentVerifier, IUserConsentVerifierInterop>()
            .map_err(|e| crate::com::os_error(&e))?;
        // SAFETY: a plain query; the prompt needs an owner window.
        let owner = unsafe { GetForegroundWindow() };
        // SAFETY: the message outlives the call; the operation is awaited below.
        let op: windows_future::IAsyncOperation<UserConsentVerificationResult> =
            unsafe { interop.RequestVerificationForWindowAsync(owner, &HSTRING::from(message)) }
                .map_err(|e| crate::com::os_error(&e))?;
        let result = op.join().map_err(|e| crate::com::os_error(&e))?;
        Ok(result == UserConsentVerificationResult::Verified)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Only availability: a test never shows the Hello prompt.
    #[test]
    fn availability_is_answered_without_a_prompt() {
        let _ = WindowsHello.available();
    }
}
