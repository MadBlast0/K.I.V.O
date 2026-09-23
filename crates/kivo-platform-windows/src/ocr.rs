//! Local OCR (TOOL-15): `Windows.Media.Ocr`, offline and free, in the user's languages. Images
//! stay in memory.

use kivo_platform::{Image, Ocr, PlatformError, PlatformResult, Rect, TextLine};
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::DataWriter;
use windows::core::HSTRING;

#[derive(Default)]
pub struct WindowsOcr;

fn engine(language: &str) -> PlatformResult<OcrEngine> {
    if !language.is_empty()
        && let Ok(lang) = Language::CreateLanguage(&HSTRING::from(language))
        && OcrEngine::IsLanguageSupported(&lang).unwrap_or(false)
        && let Ok(engine) = OcrEngine::TryCreateFromLanguage(&lang)
    {
        return Ok(engine);
    }
    // The user's own languages (a language pack with OCR must be installed).
    OcrEngine::TryCreateFromUserProfileLanguages().map_err(|_| PlatformError::Unsupported)
}

fn bitmap(image: &Image) -> PlatformResult<SoftwareBitmap> {
    let (w, h) = (image.width, image.height);
    if w == 0 || h == 0 || image.rgba.len() != (w as usize) * (h as usize) * 4 {
        return Err(PlatformError::NotFound("an image".into()));
    }
    // OCR reads BGRA: swap red and blue.
    let mut bgra = image.rgba.clone();
    for px in bgra.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    let writer = DataWriter::new().map_err(|e| crate::com::os_error(&e))?;
    writer
        .WriteBytes(&bgra)
        .map_err(|e| crate::com::os_error(&e))?;
    let buffer = writer
        .DetachBuffer()
        .map_err(|e| crate::com::os_error(&e))?;
    SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        i32::try_from(w).unwrap_or(i32::MAX),
        i32::try_from(h).unwrap_or(i32::MAX),
        BitmapAlphaMode::Premultiplied,
    )
    .map_err(|e| crate::com::os_error(&e))
}

impl Ocr for WindowsOcr {
    fn recognize(&self, image: &Image, language: &str) -> PlatformResult<Vec<TextLine>> {
        let engine = engine(language)?;
        let max = OcrEngine::MaxImageDimension().unwrap_or(10_000);
        if image.width > max || image.height > max {
            return Err(PlatformError::Unsupported);
        }
        let result = engine
            .RecognizeAsync(&bitmap(image)?)
            .and_then(|op| op.join())
            .map_err(|e| crate::com::os_error(&e))?;
        let lines = result.Lines().map_err(|e| crate::com::os_error(&e))?;
        let mut out = Vec::new();
        for line in lines {
            let text = line.Text().map(|t| t.to_string()).unwrap_or_default();
            // A line's box: the union of its words' boxes.
            let (mut l, mut t, mut r, mut b) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            if let Ok(words) = line.Words() {
                for word in words {
                    if let Ok(rect) = word.BoundingRect() {
                        l = l.min(rect.X);
                        t = t.min(rect.Y);
                        r = r.max(rect.X + rect.Width);
                        b = b.max(rect.Y + rect.Height);
                    }
                }
            }
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "pixels"
            )]
            let bounds = if l <= r {
                Rect {
                    x: l as i32,
                    y: t as i32,
                    width: (r - l) as u32,
                    height: (b - t) as u32,
                }
            } else {
                Rect {
                    x: 0,
                    y: 0,
                    width: 0,
                    height: 0,
                }
            };
            out.push(TextLine { text, bounds });
        }
        Ok(out)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use windows::Win32::Foundation::{COLORREF, RECT};
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, CreateFontW,
        DIB_RGB_COLORS, DT_LEFT, DT_NOPREFIX, DeleteDC, DeleteObject, DrawTextW, FW_NORMAL,
        HGDIOBJ, PatBlt, SelectObject, SetBkMode, SetTextColor, TRANSPARENT, WHITENESS,
    };

    /// Draws `text` black on white with GDI into an image (no screen involved).
    pub(crate) fn render(text: &str, width: u32, height: u32) -> Image {
        // SAFETY: an off-screen DIB section owned and freed here.
        unsafe {
            let dc = CreateCompatibleDC(None);
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: u32::try_from(size_of::<BITMAPINFOHEADER>()).unwrap(),
                    biWidth: i32::try_from(width).unwrap(),
                    biHeight: -i32::try_from(height).unwrap(),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let dib = CreateDIBSection(
                Some(dc),
                &raw const info,
                DIB_RGB_COLORS,
                &raw mut bits,
                None,
                0,
            )
            .unwrap();
            let old = SelectObject(dc, HGDIOBJ(dib.0));
            let _ = PatBlt(
                dc,
                0,
                0,
                i32::try_from(width).unwrap(),
                i32::try_from(height).unwrap(),
                WHITENESS,
            );
            let font = CreateFontW(
                48,
                0,
                0,
                0,
                FW_NORMAL.0 as i32,
                0,
                0,
                0,
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                0,
                &HSTRING::from("Segoe UI"),
            );
            let old_font = SelectObject(dc, HGDIOBJ(font.0));
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, COLORREF(0));
            let mut wide: Vec<u16> = text.encode_utf16().collect();
            let mut rect = RECT {
                left: 20,
                top: 20,
                right: i32::try_from(width).unwrap() - 20,
                bottom: i32::try_from(height).unwrap() - 20,
            };
            DrawTextW(dc, &mut wide, &raw mut rect, DT_LEFT | DT_NOPREFIX);
            let len = (width * height * 4) as usize;
            let bgra = std::slice::from_raw_parts(bits.cast::<u8>(), len).to_vec();
            SelectObject(dc, old_font);
            SelectObject(dc, old);
            let _ = DeleteObject(HGDIOBJ(font.0));
            let _ = DeleteObject(HGDIOBJ(dib.0));
            let _ = DeleteDC(dc);
            let mut rgba = bgra;
            for px in rgba.chunks_exact_mut(4) {
                px.swap(0, 2);
                px[3] = 255;
            }
            Image {
                width,
                height,
                rgba,
            }
        }
    }

    #[test]
    fn recognizes_text_drawn_into_an_image() {
        let image = render("Export complete", 640, 120);
        match WindowsOcr.recognize(&image, "en-US") {
            Ok(lines) => {
                let all: String = lines
                    .iter()
                    .map(|l| l.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                assert!(all.to_lowercase().contains("export"), "read: {all}");
                assert!(lines[0].bounds.width > 0);
            }
            // A machine without any OCR language pack.
            Err(e) => assert_eq!(e, PlatformError::Unsupported),
        }
    }

    #[test]
    fn a_malformed_image_is_refused() {
        let bad = Image {
            width: 10,
            height: 10,
            rgba: vec![0; 12],
        };
        assert!(WindowsOcr.recognize(&bad, "en-US").is_err());
    }
}
