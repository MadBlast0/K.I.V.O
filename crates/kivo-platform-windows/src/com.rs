//! COM and OS-error helpers shared by the Windows implementations.

use kivo_platform::{PlatformError, PlatformResult};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};

pub(crate) fn os_error(e: &windows::core::Error) -> PlatformError {
    PlatformError::Os {
        code: i64::from(e.code().0),
        message: e.message(),
    }
}

/// COM (multithreaded apartment) for the calling thread, released when dropped.
pub(crate) struct Com;

impl Com {
    pub(crate) fn init() -> PlatformResult<Self> {
        // SAFETY: initializing COM on this thread; balanced by `CoUninitialize` in Drop. An
        // already-initialized thread (S_FALSE) still needs the matching uninitialize.
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .map_err(|e| os_error(&e))?;
        Ok(Self)
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        // SAFETY: balances the successful `CoInitializeEx` in `init` on this thread.
        unsafe { CoUninitialize() };
    }
}
