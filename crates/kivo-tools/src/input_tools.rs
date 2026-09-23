//! Input (TOOL-33), the last tier of the capability ladder: click, type, press keys, scroll.
//!
//! - Before acting, the target window is re-validated: it must still be in front, and when the
//!   caller saw its bounds, they must not have changed. A click must land inside it.
//! - Typing (and any key but navigation) into a password field is refused, whatever the mode:
//!   UIA's `IsPassword` on the focused element decides (SECURITY §1.1 hard limit).
//! - Every input action is Medium risk unless the user asked for it directly.

use crate::builtin::{Def, object, platform_error};
use crate::controls::{Builder, Controls, missing, window_by_id, window_of, window_targets};
use crate::registry::{Output, Tool};
use crate::uia_tools::element_arg;
use kivo_core::capability::Capability;
use kivo_core::text;
use kivo_core::tool::{
    CapabilityTier, Initiator, Reversibility, Risk, SideEffect, ToolError, ToolErrorCode,
};
use kivo_platform::{MouseButton, Point, Rect, WindowInfo};
use serde_json::{Value, json};
use std::sync::Arc;

/// Keys that only move around (allowed even in a password field).
const NAVIGATION: &[&str] = &["tab", "esc", "escape", "enter", "return", "shift"];

fn def(id: &'static str, description: &'static str, params: Value) -> Def {
    Def {
        id,
        description,
        params,
        risk: Risk::Medium,
        effects: &[SideEffect::LocalWrite],
        capability: Capability::ComputerUse,
        reversibility: Reversibility::NotApplicable,
        egress: false,
        timeout_ms: 20_000,
        tier: CapabilityTier::Input,
    }
}

/// Medium, or Low when the user asked for exactly this themselves.
fn assess() -> Box<crate::controls::Assess> {
    Box::new(|_: &Value, initiator: Initiator, _: &Controls| {
        if initiator == Initiator::UserDirect {
            Risk::Low
        } else {
            Risk::Medium
        }
    })
}

fn moved() -> ToolError {
    ToolError::new(ToolErrorCode::Failed, text::t("error.input.moved"))
}

/// The window to act in, re-validated: in front now, and where the caller last saw it.
fn validated_window(args: &Value, c: &Controls) -> Result<WindowInfo, ToolError> {
    let from_element = args["element"]
        .as_str()
        .and_then(|e| kivo_platform::ElementRef(e.to_owned()).window());
    let window = match from_element {
        Some(id) => window_by_id(c, id).ok_or_else(moved)?,
        None => window_of(args, c)?,
    };
    let front = c.windows.foreground().map_err(platform_error)?;
    if front.as_ref().map(|f| f.id) != Some(window.id) {
        return Err(ToolError::new(
            ToolErrorCode::Failed,
            text::t("error.input.notInFront"),
        ));
    }
    let now = front.map(|f| f.bounds).unwrap_or(window.bounds);
    if let Ok(expected) = serde_json::from_value::<Rect>(args["expectBounds"].clone())
        && expected != now
    {
        return Err(moved());
    }
    Ok(WindowInfo {
        bounds: now,
        ..window
    })
}

/// The hard limit before asking: only the password refusal counts here; a lookup that fails is
/// left for `run` to report.
fn password_limit(args: &Value, c: &Controls) -> Result<(), ToolError> {
    match not_a_password(args, c) {
        Err(e) if e.code == ToolErrorCode::AccessDenied => Err(e),
        _ => Ok(()),
    }
}

/// Refuses when the focused element (or the given one) is a password field.
fn not_a_password(args: &Value, c: &Controls) -> Result<(), ToolError> {
    let node = match args["element"].as_str() {
        Some(_) => Some(
            c.uia
                .describe(&element_arg(args)?)
                .map_err(platform_error)?,
        ),
        None => c.uia.focused().ok().flatten(),
    };
    if node.is_some_and(|n| n.is_password) {
        return Err(ToolError::new(
            ToolErrorCode::AccessDenied,
            text::t("error.uia.password"),
        ));
    }
    Ok(())
}

fn point_arg(args: &Value, c: &Controls, window: &WindowInfo) -> Result<Point, ToolError> {
    let at = if args["element"].is_string() {
        let bounds = c.uia.bounds(&element_arg(args)?).map_err(platform_error)?;
        bounds.center()
    } else {
        let x = args["x"].as_i64().ok_or_else(|| missing("x"))?;
        let y = args["y"].as_i64().ok_or_else(|| missing("y"))?;
        Point {
            x: i32::try_from(x).map_err(|_| missing("x"))?,
            y: i32::try_from(y).map_err(|_| missing("y"))?,
        }
    };
    if !window.bounds.contains(at) {
        return Err(ToolError::new(
            ToolErrorCode::InvalidArgs,
            text::t("error.input.outside"),
        ));
    }
    Ok(at)
}

fn target_props(extra: Value) -> Value {
    let mut props = json!({
        "window": { "type": "string", "description": "Window id; the window in front by default" },
        "element": { "type": "string", "description": "An element reference (acts at its centre)" },
        "expectBounds": { "type": "object", "description": "The window bounds you saw; the action is refused if the window moved" }
    });
    if let (Some(p), Some(e)) = (props.as_object_mut(), extra.as_object()) {
        p.extend(e.clone());
    }
    props
}

pub(crate) fn tools(c: &Arc<Controls>) -> Vec<Arc<dyn Tool>> {
    let b = Builder::new(c);
    vec![
        b.tool(
            &def(
                "input.click",
                "Click at a screen point (physical pixels) or an element's centre, inside the window in front. The last resort after UIA and the browser tools.",
                object(
                    target_props(json!({
                        "x": { "type": "integer" },
                        "y": { "type": "integer" },
                        "button": { "type": "string", "enum": ["left", "right", "middle"] },
                        "count": { "type": "integer", "minimum": 1, "maximum": 3 }
                    })),
                    &[],
                ),
            ),
            Box::new(|args, c, _| {
                let window = validated_window(args, c)?;
                let at = point_arg(args, c, &window)?;
                let button: MouseButton = serde_json::from_value(args["button"].clone()).unwrap_or(MouseButton::Left);
                let count = args["count"].as_u64().map_or(1, |n| u8::try_from(n.clamp(1, 3)).unwrap_or(1));
                c.input.click(at, button, count).map_err(platform_error)?;
                Ok(Output::new(text::t("reply.input.clicked"), json!({ "at": at })))
            }),
        )
        .assess(assess())
        .targets(Box::new(window_targets))
        .build(),
        b.tool(
            &def(
                "input.type",
                "Type text into the focused field of the window in front (never into a password field).",
                object(target_props(json!({ "text": { "type": "string" } })), &["text"]),
            ),
            Box::new(|args, c, _| {
                let text_arg = args["text"].as_str().ok_or_else(|| missing("text"))?;
                validated_window(args, c)?;
                not_a_password(args, c)?;
                c.input.type_text(text_arg).map_err(platform_error)?;
                Ok(Output::new(text::t("reply.input.typed"), json!({ "chars": text_arg.chars().count() })))
            }),
        )
        .assess(assess())
        .targets(Box::new(window_targets))
        .hard_limit(Box::new(password_limit))
        .build(),
        b.tool(
            &def(
                "input.press",
                "Press a key or a combination, e.g. [\"Ctrl\", \"S\"], in the window in front.",
                object(
                    target_props(json!({ "keys": { "type": "array", "items": { "type": "string" }, "minItems": 1, "maxItems": 4 } })),
                    &["keys"],
                ),
            ),
            Box::new(|args, c, _| {
                let keys: Vec<String> = args["keys"]
                    .as_array()
                    .map(|a| a.iter().filter_map(Value::as_str).map(str::to_owned).collect())
                    .filter(|k: &Vec<String>| !k.is_empty())
                    .ok_or_else(|| missing("keys"))?;
                validated_window(args, c)?;
                let navigation = keys.iter().all(|k| NAVIGATION.contains(&k.to_lowercase().as_str()));
                if !navigation {
                    not_a_password(args, c)?;
                }
                c.input.press(&keys).map_err(platform_error)?;
                Ok(Output::new(text::tf("reply.input.pressed", &[("keys", &keys.join("+"))]), json!({ "keys": keys })))
            }),
        )
        .assess(assess())
        .targets(Box::new(window_targets))
        .hard_limit(Box::new(|args, c| {
            let navigation = args["keys"].as_array().is_some_and(|keys| {
                keys.iter()
                    .filter_map(Value::as_str)
                    .all(|k| NAVIGATION.contains(&k.to_lowercase().as_str()))
            });
            if navigation { Ok(()) } else { password_limit(args, c) }
        }))
        .build(),
        b.tool(
            &def(
                "input.scroll",
                "Scroll the window in front at a point (positive lines scroll up).",
                object(
                    target_props(json!({ "x": { "type": "integer" }, "y": { "type": "integer" }, "lines": { "type": "integer" } })),
                    &["lines"],
                ),
            ),
            Box::new(|args, c, _| {
                let window = validated_window(args, c)?;
                let at = if args["x"].is_i64() || args["element"].is_string() {
                    point_arg(args, c, &window)?
                } else {
                    window.bounds.center()
                };
                let lines = args["lines"].as_i64().ok_or_else(|| missing("lines"))?.clamp(-50, 50);
                c.input
                    .scroll(at, i32::try_from(lines).unwrap_or(0))
                    .map_err(platform_error)?;
                Ok(Output::new(String::new(), json!({ "at": at, "lines": lines })))
            }),
        )
        .assess(assess())
        .targets(Box::new(window_targets))
        .build(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::rig;
    use kivo_testkit::InputAction;

    #[test]
    fn clicks_land_inside_the_window_in_front_or_not_at_all() {
        let r = rig();
        r.tool("input.click")
            .run(&json!({ "x": 300, "y": 200 }))
            .unwrap();
        assert!(matches!(
            r.input.actions.lock().unwrap()[0],
            InputAction::Click(Point { x: 300, y: 200 }, MouseButton::Left, 1)
        ));
        let e = r
            .tool("input.click")
            .run(&json!({ "x": 5000, "y": 5000 }))
            .unwrap_err();
        assert_eq!(e.code, ToolErrorCode::InvalidArgs);
        // The window moved since the brain looked: refused.
        let e = r
            .tool("input.click")
            .run(&json!({ "x": 300, "y": 200, "expectBounds": { "x": 0, "y": 0, "width": 10, "height": 10 } }))
            .unwrap_err();
        assert_eq!(e.message, "The window moved or changed, so I didn’t click.");
        // An element's centre.
        r.tool("input.click")
            .run(&json!({ "element": r.uia.element("103") }))
            .unwrap();
        assert_eq!(r.input.actions.lock().unwrap().len(), 2);
    }

    #[test]
    fn a_window_that_is_no_longer_in_front_is_refused() {
        let r = rig();
        r.bring_other_window_to_front();
        let e = r
            .tool("input.type")
            .run(&json!({ "text": "hi", "window": r.app_window() }))
            .unwrap_err();
        assert_eq!(
            e.message,
            "That window isn’t in front anymore, so I stopped."
        );
        assert!(r.input.actions.lock().unwrap().is_empty());
    }

    #[test]
    fn never_types_into_a_password_field() {
        let r = rig();
        *r.uia.focus.lock().unwrap() = Some("102".into());
        let e = r
            .tool("input.type")
            .run(&json!({ "text": "hunter2" }))
            .unwrap_err();
        assert_eq!(e.code, ToolErrorCode::AccessDenied);
        let e = r
            .tool("input.press")
            .run(&json!({ "keys": ["Ctrl", "V"] }))
            .unwrap_err();
        assert_eq!(
            e.code,
            ToolErrorCode::AccessDenied,
            "no pasting into it either"
        );
        r.tool("input.press")
            .run(&json!({ "keys": ["Tab"] }))
            .unwrap();
        *r.uia.focus.lock().unwrap() = Some("101".into());
        r.tool("input.type").run(&json!({ "text": "Ada" })).unwrap();
        assert!(
            r.input
                .actions
                .lock()
                .unwrap()
                .contains(&InputAction::Type("Ada".into()))
        );
    }

    #[test]
    fn input_is_medium_unless_the_user_asked_directly() {
        let r = rig();
        let t = r.tool("input.click");
        assert_eq!(t.assess(&json!({}), Initiator::Brain), Risk::Medium);
        assert_eq!(t.assess(&json!({}), Initiator::UserDirect), Risk::Low);
        assert_eq!(t.spec().capability, Capability::ComputerUse);
    }
}
