//! The clipboard (TOOL-17): Unicode text only. What is read is untrusted content; the tools mark
//! it so (SECURITY §4).

use kivo_platform::{Clipboard, PlatformError, PlatformResult};
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;

/// Longest text read from the clipboard (a huge paste isn't a request).
const MAX_CHARS: usize = 100_000;

#[derive(Default)]
pub struct WindowsClipboard;

/// The clipboard, open for this thread until dropped. Another app may hold it briefly, so opening
/// retries for a moment.
struct Open;

impl Open {
    fn new() -> PlatformResult<Self> {
        for _ in 0..10 {
            // SAFETY: no owner window; closed in Drop.
            if unsafe { OpenClipboard(None) }.is_ok() {
                return Ok(Self);
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        Err(PlatformError::Conflict("the clipboard".into()))
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        // SAFETY: balances the successful OpenClipboard.
        let _ = unsafe { CloseClipboard() };
    }
}

impl Clipboard for WindowsClipboard {
    fn read_text(&self) -> PlatformResult<Option<String>> {
        let format = u32::from(CF_UNICODETEXT.0);
        // SAFETY: a plain query.
        if unsafe { IsClipboardFormatAvailable(format) }.is_err() {
            return Ok(None);
        }
        let _open = Open::new()?;
        // SAFETY: the clipboard is open; the handle stays owned by the clipboard and is only
        // locked while copied, then unlocked.
        unsafe {
            let Ok(handle) = GetClipboardData(format) else {
                return Ok(None);
            };
            let global = HGLOBAL(handle.0);
            let ptr = GlobalLock(global).cast::<u16>();
            if ptr.is_null() {
                return Ok(None);
            }
            let mut len = 0usize;
            while len < MAX_CHARS * 2 && *ptr.add(len) != 0 {
                len += 1;
            }
            let text = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
            let _ = GlobalUnlock(global);
            Ok(Some(text.chars().take(MAX_CHARS).collect()))
        }
    }

    fn write_text(&self, text: &str) -> PlatformResult<()> {
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let _open = Open::new()?;
        // SAFETY: the clipboard is open; the allocation is filled while locked and handed to the
        // clipboard, which owns it after a successful SetClipboardData (freed here otherwise).
        unsafe {
            EmptyClipboard().map_err(|e| crate::com::os_error(&e))?;
            let global =
                GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2).map_err(|e| crate::com::os_error(&e))?;
            let ptr = GlobalLock(global).cast::<u16>();
            if ptr.is_null() {
                let _ = GlobalFree(Some(global));
                return Err(PlatformError::Os {
                    code: 0,
                    message: "GlobalLock".into(),
                });
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
            let _ = GlobalUnlock(global);
            if let Err(e) = SetClipboardData(u32::from(CF_UNICODETEXT.0), Some(HANDLE(global.0))) {
                let _ = GlobalFree(Some(global));
                return Err(crate::com::os_error(&e));
            }
        }
        Ok(())
    }
}
