//! Screen capture and OCR (TOOLS_AND_CONTROL §3, CAPABILITIES §3). Captures happen on request
//! only and stay in memory.

use crate::error::PlatformResult;
use crate::types::{Rect, WindowId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum CaptureTarget {
    /// The monitor with the foreground window.
    ActiveMonitor,
    Window {
        id: WindowId,
    },
    Region {
        rect: Rect,
    },
}

/// An RGBA image, 4 bytes per pixel, rows top to bottom.
#[derive(Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for Image {
    // Never dump pixel data into logs.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Image({}x{}, {} bytes)",
            self.width,
            self.height,
            self.rgba.len()
        )
    }
}

pub trait Screen: Send + Sync {
    fn capture(&self, target: CaptureTarget) -> PlatformResult<Image>;
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextLine {
    pub text: String,
    pub bounds: Rect,
}

pub trait Ocr: Send + Sync {
    /// Recognizes text locally. `language` is a BCP-47 tag, e.g. "en-US".
    fn recognize(&self, image: &Image, language: &str) -> PlatformResult<Vec<TextLine>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_pixels() {
        let img = Image {
            width: 2,
            height: 1,
            rgba: vec![255; 8],
        };
        assert_eq!(format!("{img:?}"), "Image(2x1, 8 bytes)");
    }
}
