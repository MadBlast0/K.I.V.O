//! Screen awareness (CAPABILITIES §3, CAP-08, TOOL-15, TOOL-32): on request only, the active
//! window (or a region) is captured once, then read locally: the UIA tree excerpt and
//! `Windows.Media.Ocr`. Only if the question needs the picture itself, and the settings allow it,
//! does `screen.look` hand the image to a vision brain (the runtime says "Sent a screenshot of …
//! to …"). Captures live in memory only; nothing is written to disk.

use crate::builtin::{Def, object, platform_error, region_arg};
use crate::controls::{Builder, Controls, window_of, window_source, window_targets};
use crate::registry::{Output, Tool};
use kivo_core::capability::Capability;
use kivo_core::text;
use kivo_core::tool::{CapabilityTier, Reversibility, Risk, SideEffect, ToolError, ToolErrorCode};
use kivo_platform::{CaptureTarget, Image, PlatformError, UiNode};
use serde_json::{Value, json};
use std::sync::Arc;

/// Characters of the UIA excerpt a model gets.
const EXCERPT_CHARS: usize = 4_000;
/// The longest side of an image sent to a vision brain.
pub const VISION_MAX_SIDE: u32 = 1_568;

fn def(id: &'static str, description: &'static str, params: Value, egress: bool) -> Def {
    Def {
        id,
        description,
        params,
        risk: Risk::Low,
        effects: &[SideEffect::LocalRead],
        capability: Capability::ScreenAwareness,
        reversibility: Reversibility::NotApplicable,
        egress,
        timeout_ms: 20_000,
        tier: CapabilityTier::Vision,
    }
}

fn target_params() -> Value {
    object(
        json!({
            "window": { "type": "string", "description": "Window id; the window in front by default" },
            "region": { "type": "object", "properties": { "x": {"type": "integer"}, "y": {"type": "integer"}, "width": {"type": "integer"}, "height": {"type": "integer"} } }
        }),
        &[],
    )
}

/// What to capture and a label for it.
fn capture(
    args: &Value,
    c: &Controls,
) -> Result<(Image, String, Option<kivo_platform::WindowId>), ToolError> {
    if let Some(rect) = region_arg(args)? {
        let image = c
            .screen
            .capture(CaptureTarget::Region { rect })
            .map_err(platform_error)?;
        return Ok((image, text::t("source.screenRegion"), None));
    }
    let window = window_of(args, c)?;
    let image = c
        .screen
        .capture(CaptureTarget::Window { id: window.id })
        .map_err(platform_error)?;
    Ok((image, window_source(&window), Some(window.id)))
}

/// The UIA tree as indented lines ("Button Export"), for reading.
pub(crate) fn excerpt(node: &UiNode, max: usize) -> String {
    fn walk(n: &UiNode, depth: usize, out: &mut String, max: usize) {
        if out.len() >= max {
            return;
        }
        let name = n.name.trim();
        let value = n.value.as_deref().unwrap_or("").trim();
        if !name.is_empty() || !value.is_empty() {
            out.push_str(&"  ".repeat(depth.min(8)));
            out.push_str(&n.role);
            if !name.is_empty() {
                out.push(' ');
                out.push_str(name);
            }
            if !value.is_empty() {
                out.push_str(": ");
                out.push_str(value);
            }
            out.push('\n');
        }
        for child in &n.children {
            walk(child, depth + 1, out, max);
        }
    }
    let mut out = String::new();
    walk(node, 0, &mut out, max);
    if out.len() > max {
        let mut cut = max;
        while !out.is_char_boundary(cut) {
            cut -= 1;
        }
        out.truncate(cut);
        out.push('…');
    }
    out
}

fn ocr_text(c: &Controls, image: &Image) -> Result<String, ToolError> {
    match c.ocr.recognize(image, &c.language) {
        Ok(lines) => Ok(lines
            .into_iter()
            .map(|l| l.text)
            .collect::<Vec<_>>()
            .join("\n")),
        Err(PlatformError::Unsupported) => Err(ToolError::new(
            ToolErrorCode::Unsupported,
            text::t("error.screen.noOcr"),
        )),
        Err(e) => Err(platform_error(e)),
    }
}

/// Downscales so the longest side is at most `max` (nearest neighbour; enough for a model).
pub fn downscale(image: &Image, max: u32) -> Image {
    let side = image.width.max(image.height);
    if side <= max || side == 0 {
        return image.clone();
    }
    let scale = f64::from(max) / f64::from(side);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "pixel sizes"
    )]
    let (w, h) = (
        ((f64::from(image.width) * scale) as u32).max(1),
        ((f64::from(image.height) * scale) as u32).max(1),
    );
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "pixel sizes"
        )]
        let sy = ((f64::from(y) / scale) as u32).min(image.height - 1);
        for x in 0..w {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "pixel sizes"
            )]
            let sx = ((f64::from(x) / scale) as u32).min(image.width - 1);
            let i = ((sy * image.width + sx) * 4) as usize;
            rgba.extend_from_slice(&image.rgba[i..i + 4]);
        }
    }
    Image {
        width: w,
        height: h,
        rgba,
    }
}

/// PNG bytes, in memory.
pub fn png(image: &Image) -> Result<Vec<u8>, ToolError> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, image.width, image.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let fail = |e: png::EncodingError| {
            ToolError::new(ToolErrorCode::Failed, text::t("error.failed"))
                .with_detail(e.to_string())
        };
        let mut writer = encoder.write_header().map_err(fail)?;
        writer.write_image_data(&image.rgba).map_err(fail)?;
    }
    Ok(out)
}

pub(crate) fn tools(c: &Arc<Controls>) -> Vec<Arc<dyn Tool>> {
    let b = Builder::new(c);
    vec![
        b.tool(
            &def(
                "screen.read",
                "Read what's on screen: the window in front (or a region) captured once, read locally from its controls and with OCR. Use this before screen.look.",
                target_params(),
                false,
            ),
            Box::new(|args, c, _| {
                let (image, source, window) = capture(args, c)?;
                let controls = match window {
                    Some(id) => c
                        .uia
                        .tree(id, 8, 300)
                        .map(|t| excerpt(&t, EXCERPT_CHARS))
                        .unwrap_or_default(),
                    None => String::new(),
                };
                let ocr = ocr_text(c, &image).unwrap_or_default();
                // The capture ends here: dropped, never saved.
                drop(image);
                Ok(Output::new(
                    text::tf("reply.screen.read", &[("name", &source)]),
                    json!({ "source": source, "controls": controls, "text": ocr }),
                )
                .untrusted(source))
            }),
        )
        .targets(Box::new(window_targets))
        .build(),
        b.tool(
            &def(
                "screen.ocr",
                "Recognize the text in a window or region of the screen, locally.",
                target_params(),
                false,
            ),
            Box::new(|args, c, _| {
                let (image, source, _) = capture(args, c)?;
                let lines = c.ocr.recognize(&image, &c.language).map_err(|e| match e {
                    PlatformError::Unsupported => ToolError::new(
                        ToolErrorCode::Unsupported,
                        text::t("error.screen.noOcr"),
                    ),
                    other => platform_error(other),
                })?;
                Ok(Output::new(String::new(), json!({ "lines": lines })).untrusted(source))
            }),
        )
        .targets(Box::new(window_targets))
        .build(),
        b.tool(
            &def(
                "screen.look",
                "Send one screenshot of the window in front (or a region) to you, when the question needs the picture itself (layout, images, charts). Only after screen.read wasn't enough.",
                target_params(),
                true,
            ),
            Box::new(|args, c, _| {
                if !c.cloud_vision_allowed() {
                    return Err(ToolError::new(
                        ToolErrorCode::AccessDenied,
                        text::t("error.screen.visionOff"),
                    ));
                }
                let (image, source, _) = capture(args, c)?;
                let small = downscale(&image, VISION_MAX_SIDE);
                drop(image);
                let bytes = png(&small)?;
                let mut out = Output::new(
                    String::new(),
                    json!({ "source": source, "width": small.width, "height": small.height }),
                )
                .untrusted(source);
                out.image = Some(bytes);
                Ok(out)
            }),
        )
        .targets(Box::new(window_targets))
        .build(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::rig;
    use kivo_platform::{Rect, TextLine};

    #[test]
    fn reading_uses_the_tree_and_local_ocr_and_keeps_nothing() {
        let r = rig();
        *r.ocr.lines.lock().unwrap() = vec![TextLine {
            text: "error: cannot find value `x`".into(),
            bounds: Rect {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            },
        }];
        let out = r.tool("screen.read").run(&json!({})).unwrap();
        assert!(
            out.data["controls"]
                .as_str()
                .unwrap()
                .contains("Button Greet")
        );
        assert!(
            out.data["text"]
                .as_str()
                .unwrap()
                .contains("cannot find value")
        );
        assert!(out.source.is_some());
        assert!(
            out.image.is_none(),
            "no picture leaves the device from screen.read"
        );
        assert_eq!(
            std::fs::read_dir(r.dir.path())
                .unwrap()
                .filter(|e| e
                    .as_ref()
                    .unwrap()
                    .path()
                    .extension()
                    .is_some_and(|x| x == "png"))
                .count(),
            0
        );
    }

    #[test]
    fn looking_needs_cloud_vision_allowed_and_sends_a_small_png() {
        let r = rig();
        let e = r.tool("screen.look").run(&json!({})).unwrap_err();
        assert_eq!(e.code, ToolErrorCode::AccessDenied);
        r.allow_cloud_vision();
        let out = r.tool("screen.look").run(&json!({})).unwrap();
        let png = out.image.unwrap();
        assert_eq!(&png[1..4], b"PNG");
        assert!(r.tool("screen.look").spec().data_egress);
    }

    #[test]
    fn downscaling_keeps_the_aspect() {
        let img = Image {
            width: 4000,
            height: 2000,
            rgba: vec![7; 4000 * 2000 * 4],
        };
        let small = downscale(&img, 1000);
        assert_eq!((small.width, small.height), (1000, 500));
        assert_eq!(small.rgba.len(), 1000 * 500 * 4);
    }
}
