//! App icons for the Island's leading icon while KIVO acts on an app (UX-46): the shell's own
//! icon for a program, shortcut or packaged app, as RGBA pixels.

use crate::com::Com;
use kivo_platform::Image;
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDIBits, HGDIOBJ,
};
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
};
use windows::core::HSTRING;

/// The icon of `target` (a program path, a `.lnk`, or a packaged app's AUMID) at `size` px.
pub fn app_icon(target: &str, size: u32) -> Option<Image> {
    let _com = Com::init().ok()?;
    let path = if target.contains('!') && !target.contains('\\') {
        // A packaged app's AUMID.
        format!("shell:AppsFolder\\{target}")
    } else {
        target.to_owned()
    };
    let side = i32::try_from(size).ok()?;
    // SAFETY: COM is initialized; the bitmap and DC are freed below.
    unsafe {
        let factory: IShellItemImageFactory =
            SHCreateItemFromParsingName(&HSTRING::from(path.as_str()), None).ok()?;
        let bitmap = factory
            .GetImage(
                SIZE { cx: side, cy: side },
                SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK,
            )
            .ok()?;
        let dc = CreateCompatibleDC(None);
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: u32::try_from(size_of::<BITMAPINFOHEADER>()).unwrap_or(0),
                biWidth: side,
                biHeight: -side,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bgra = vec![0u8; (size * size * 4) as usize];
        let lines = GetDIBits(
            dc,
            bitmap,
            0,
            size,
            Some(bgra.as_mut_ptr().cast()),
            &raw mut info,
            DIB_RGB_COLORS,
        );
        let _ = DeleteDC(dc);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        if lines == 0 {
            return None;
        }
        for px in bgra.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        Some(Image {
            width: size,
            height: size,
            rgba: bgra,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notepad_has_an_icon() {
        let windir = std::env::var("SystemRoot").unwrap();
        let icon = app_icon(&format!(r"{windir}\notepad.exe"), 32);
        if let Some(icon) = icon {
            assert_eq!((icon.width, icon.height), (32, 32));
            assert!(icon.rgba.chunks_exact(4).any(|p| p[3] > 0), "not blank");
        }
        assert!(app_icon(r"C:\no\such\app.exe", 32).is_none());
    }
}
