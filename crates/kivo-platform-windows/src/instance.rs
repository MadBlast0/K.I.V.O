//! Single instance (ARCHITECTURE §1): the runtime holds `Local\KIVO.Runtime.<user-sid>`, so a
//! second runtime for the same user exits immediately.

use kivo_platform::{PlatformError, PlatformResult};
use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::HSTRING;

/// Held for the runtime's lifetime; releasing it lets another runtime start.
pub struct InstanceGuard(HANDLE);

// SAFETY: a mutex handle can be closed from any thread.
unsafe impl Send for InstanceGuard {}

/// Claims `name`. Returns `None` if another process already holds it.
pub fn claim(name: &str) -> PlatformResult<Option<InstanceGuard>> {
    // SAFETY: creating a named mutex; the handle is owned by the guard (or closed right away).
    unsafe {
        let handle =
            CreateMutexW(None, true, &HSTRING::from(name)).map_err(|e| PlatformError::Os {
                code: i64::from(e.code().0),
                message: e.message(),
            })?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(handle);
            return Ok(None);
        }
        Ok(Some(InstanceGuard(handle)))
    }
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        // SAFETY: closing the handle this guard owns.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_holder_at_a_time_and_release_frees_it() {
        let name = format!(r"Local\KIVO.Test.{}", std::process::id());
        let first = claim(&name).unwrap().expect("first claim wins");
        assert!(claim(&name).unwrap().is_none(), "a second claim is refused");
        drop(first);
        assert!(claim(&name).unwrap().is_some(), "free again once released");
    }
}
