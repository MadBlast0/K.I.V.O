//! UI Automation tools (TOOL-20, TOOL-21): find and read controls in any window, act on them
//! through their patterns, and wait for UI events without polling. Everything a window shows is
//! the app's content, so reads are `Untrusted`; password fields are never read or typed into.

use crate::builtin::{Def, object, platform_error};
use crate::controls::{
    Builder, Controls, missing, str_arg, window_by_id, window_of, window_source, window_targets,
};
use crate::registry::{Output, Tool};
use kivo_core::capability::Capability;
use kivo_core::text;
use kivo_core::tool::{CapabilityTier, Reversibility, Risk, SideEffect, ToolError, ToolErrorCode};
use kivo_platform::{
    ElementQuery, ElementRef, PlatformError, UiEvent, UiEventKind, UiNode, UiSubscription,
};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

/// Tree limits: what a model can take in one look.
const TREE_DEPTH: u8 = 6;
const TREE_NODES: usize = 150;
/// The longest a `uia.wait` may wait.
const MAX_WAIT: Duration = Duration::from_secs(30);

fn def(
    id: &'static str,
    description: &'static str,
    params: Value,
    risk: Risk,
    effects: &'static [SideEffect],
    reversibility: Reversibility,
) -> Def {
    Def {
        id,
        description,
        params,
        risk,
        effects,
        capability: Capability::UiAutomation,
        reversibility,
        egress: false,
        timeout_ms: 15_000,
        tier: CapabilityTier::Uia,
    }
}

fn element_param() -> Value {
    object(
        json!({ "element": { "type": "string", "description": "An element reference from uia.find or uia.get_tree" } }),
        &["element"],
    )
}

pub(crate) fn element_arg(args: &Value) -> Result<ElementRef, ToolError> {
    Ok(ElementRef(str_arg(args, "element")?.to_owned()))
}

/// A platform error from UIA, worded for the element case.
fn uia_error(e: PlatformError) -> ToolError {
    match e {
        PlatformError::Unsupported => ToolError::new(
            ToolErrorCode::Unsupported,
            text::t("error.uia.unsupportedAction"),
        ),
        PlatformError::AccessDenied => {
            ToolError::new(ToolErrorCode::AccessDenied, text::t("error.uia.password"))
        }
        other => platform_error(other),
    }
}

/// The source label for content read from an element's window.
fn source_of(c: &Controls, element: &ElementRef) -> String {
    element
        .window()
        .and_then(|w| window_by_id(c, w))
        .map_or_else(|| text::t("source.window"), |w| window_source(&w))
}

/// A tree node for a model: nothing it doesn't need.
fn compact(node: &UiNode) -> Value {
    let mut v = serde_json::to_value(node).unwrap_or(Value::Null);
    if let Some(o) = v.as_object_mut() {
        o.remove("isPassword");
        if node.is_password {
            o.insert("password".into(), Value::Bool(true));
        }
        if node.enabled {
            o.remove("enabled");
        }
    }
    v
}

/// Where `target` sits in `window`, as a location key (`topRight`, `center`, …) for "It's at the
/// top right of the window" (UX-39, plan §141).
pub fn location_in(window: &kivo_platform::Rect, target: &kivo_platform::Rect) -> &'static str {
    let third = |start: i32, size: u32, at: i32| {
        let size = i64::from(size.max(3));
        let rel = i64::from(at) - i64::from(start);
        if rel * 3 < size {
            0
        } else if rel * 3 < size * 2 {
            1
        } else {
            2
        }
    };
    let cx = target.x + i32::try_from(target.width / 2).unwrap_or(0);
    let cy = target.y + i32::try_from(target.height / 2).unwrap_or(0);
    match (
        third(window.y, window.height, cy),
        third(window.x, window.width, cx),
    ) {
        (0, 0) => "topLeft",
        (0, 1) => "top",
        (0, _) => "topRight",
        (1, 0) => "left",
        (1, 1) => "center",
        (1, _) => "right",
        (_, 0) => "bottomLeft",
        (_, 1) => "bottom",
        _ => "bottomRight",
    }
}

pub(crate) fn tools(c: &Arc<Controls>) -> Vec<Arc<dyn Tool>> {
    use Reversibility::{NotApplicable, Undoable};
    use SideEffect::{LocalRead, LocalWrite};
    let b = Builder::new(c);
    vec![
        b.tool(
            &def(
                "uia.find",
                "Find controls in a window by name (fuzzy), role (Button, Edit, ListItem, …) or AutomationId. Returns element references.",
                object(
                    json!({
                        "window": { "type": "string", "description": "Window id; the window in front by default" },
                        "name": { "type": "string" },
                        "role": { "type": "string" },
                        "automationId": { "type": "string" },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 50 }
                    }),
                    &[],
                ),
                Risk::Safe,
                &[LocalRead],
                NotApplicable,
            ),
            Box::new(|args, c, _| {
                let window = window_of(args, c)?;
                let query = ElementQuery {
                    window: Some(window.id),
                    name: args["name"].as_str().map(str::to_owned),
                    role: args["role"].as_str().map(str::to_owned),
                    automation_id: args["automationId"].as_str().map(str::to_owned),
                };
                let limit = args["limit"].as_u64().map_or(10, |n| n.clamp(1, 50)) as usize;
                let found = c.uia.find(&query, limit).map_err(uia_error)?;
                let say = text::plural("reply.uia.found", found.len() as u64, &[]);
                Ok(Output::new(say, json!({ "elements": found.iter().map(compact).collect::<Vec<_>>() }))
                    .untrusted(window_source(&window)))
            }),
        )
        .targets(Box::new(window_targets))
        .build(),
        b.tool(
            &def(
                "uia.point_at",
                "Show the user where a control is: finds it by name in a window (the one in front by default), points KIVO's Island at it and says where it is. Nothing is clicked.",
                object(
                    json!({
                        "window": { "type": "string", "description": "Window id; the window in front by default" },
                        "name": { "type": "string", "description": "The control's name, like Export or Save" },
                        "text": { "type": "string", "description": "The same, as the grammar passes it" }
                    }),
                    &[],
                ),
                Risk::Safe,
                &[LocalRead],
                NotApplicable,
            ),
            Box::new(|args, c, _| {
                let window = window_of(args, c)?;
                // `text` when the grammar matched "where is the … button".
                let name = args["name"].as_str().or_else(|| args["text"].as_str()).unwrap_or_default().trim().to_owned();
                let query = ElementQuery {
                    window: Some(window.id),
                    name: Some(name.clone()),
                    role: None,
                    automation_id: None,
                };
                let found = c.uia.find(&query, 5).map_err(uia_error)?;
                let Some((node, bounds)) = found
                    .into_iter()
                    .find_map(|n| n.bounds.filter(|b| b.width > 0 && b.height > 0).map(|b| (n, b)))
                else {
                    return Err(ToolError::new(
                        ToolErrorCode::NotFound,
                        text::tf("reply.uia.notFoundNamed", &[("name", &name)]),
                    ));
                };
                let place = location_in(&window.bounds, &bounds);
                let label = if node.name.is_empty() { name } else { node.name.clone() };
                let say = text::tf(
                    "reply.uia.pointed",
                    &[
                        ("name", &label),
                        ("where", &text::t(&format!("reply.uia.where.{place}"))),
                    ],
                );
                Ok(Output::new(
                    say,
                    json!({
                        "point": {
                            "x": bounds.x, "y": bounds.y,
                            "width": bounds.width, "height": bounds.height,
                            "label": label, "where": place,
                        }
                    }),
                )
                .untrusted(window_source(&window)))
            }),
        )
        .targets(Box::new(window_targets))
        .build(),
        b.tool(
            &def(
                "uia.get_tree",
                "The window's controls as a compact tree (depth-limited, pruned), with element references and what each can do.",
                object(
                    json!({
                        "window": { "type": "string", "description": "Window id; the window in front by default" },
                        "depth": { "type": "integer", "minimum": 1, "maximum": 8 }
                    }),
                    &[],
                ),
                Risk::Safe,
                &[LocalRead],
                NotApplicable,
            ),
            Box::new(|args, c, _| {
                let window = window_of(args, c)?;
                let depth = args["depth"]
                    .as_u64()
                    .map_or(TREE_DEPTH, |d| u8::try_from(d.clamp(1, 8)).unwrap_or(TREE_DEPTH));
                let tree = c.uia.tree(window.id, depth, TREE_NODES).map_err(uia_error)?;
                // A window with nothing inside is custom-drawn (a game, some Electron or Qt apps):
                // only Vision and Input can work there (TOOL-22).
                let custom_drawn = tree.children.is_empty();
                fn walk(n: &UiNode) -> Value {
                    let mut v = compact(n);
                    if let Some(o) = v.as_object_mut() {
                        o.insert(
                            "children".into(),
                            Value::Array(n.children.iter().map(walk).collect()),
                        );
                    }
                    v
                }
                let say = if custom_drawn {
                    text::t("reply.uia.customDrawn")
                } else {
                    text::plural("reply.uia.tree", tree.count() as u64, &[])
                };
                Ok(Output::new(
                    say,
                    json!({ "tree": walk(&tree), "customDrawn": custom_drawn, "nodes": tree.count() }),
                )
                .untrusted(window_source(&window)))
            }),
        )
        .targets(Box::new(window_targets))
        .build(),
        b.tool(
            &def(
                "uia.invoke",
                "Press a button, link or menu item.",
                element_param(),
                Risk::Medium,
                &[LocalWrite],
                NotApplicable,
            ),
            Box::new(|args, c, _| {
                let element = element_arg(args)?;
                let node = c.uia.describe(&element).map_err(uia_error)?;
                c.uia.invoke(&element).map_err(uia_error)?;
                Ok(Output::new(
                    text::tf("reply.uia.pressed", &[("name", &label(&node))]),
                    json!({ "element": element.0 }),
                ))
            }),
        )
        .targets(Box::new(window_targets))
        .build(),
        b.tool(
            &def(
                "uia.set_value",
                "Set the text of a field (never a password field).",
                object(
                    json!({
                        "element": { "type": "string" },
                        "value": { "type": "string" }
                    }),
                    &["element", "value"],
                ),
                Risk::Medium,
                &[LocalWrite],
                Undoable,
            ),
            Box::new(|args, c, _| {
                let element = element_arg(args)?;
                let value = args["value"].as_str().ok_or_else(|| missing("value"))?;
                let node = c.uia.describe(&element).map_err(uia_error)?;
                // A hard limit (SECURITY §1.1): refused before the platform is even asked.
                if node.is_password {
                    return Err(ToolError::new(
                        ToolErrorCode::AccessDenied,
                        text::t("error.uia.password"),
                    ));
                }
                c.uia.set_value(&element, value).map_err(uia_error)?;
                Ok(Output::new(
                    text::tf("reply.uia.typed", &[("name", &label(&node))]),
                    json!({ "element": element.0, "previous": node.value.unwrap_or_default() }),
                ))
            }),
        )
        .undo(Box::new(|data, c| {
            let element = element_arg(data)?;
            let previous = data["previous"].as_str().unwrap_or_default();
            c.uia.set_value(&element, previous).map_err(uia_error)?;
            Ok(Output::new(text::t("reply.undone"), json!({ "element": element.0 })))
        }))
        .targets(Box::new(window_targets))
        .hard_limit(Box::new(|args, c| {
            match element_arg(args).ok().and_then(|e| c.uia.describe(&e).ok()) {
                Some(node) if node.is_password => Err(ToolError::new(
                    ToolErrorCode::AccessDenied,
                    text::t("error.uia.password"),
                )),
                _ => Ok(()),
            }
        }))
        .build(),
        b.tool(
            &def(
                "uia.toggle",
                "Switch a check box or toggle on or off.",
                element_param(),
                Risk::Medium,
                &[LocalWrite],
                Undoable,
            ),
            Box::new(|args, c, _| {
                let element = element_arg(args)?;
                let node = c.uia.describe(&element).map_err(uia_error)?;
                let on = c.uia.toggle(&element).map_err(uia_error)?;
                let key = if on { "reply.uia.on" } else { "reply.uia.off" };
                Ok(Output::new(
                    text::tf(key, &[("name", &label(&node))]),
                    json!({ "element": element.0, "on": on }),
                ))
            }),
        )
        .undo(Box::new(|data, c| {
            let element = element_arg(data)?;
            c.uia.toggle(&element).map_err(uia_error)?;
            Ok(Output::new(text::t("reply.undone"), json!({ "element": element.0 })))
        }))
        .targets(Box::new(window_targets))
        .build(),
        b.tool(
            &def(
                "uia.select",
                "Select an item in a list, tab strip or tree.",
                element_param(),
                Risk::Low,
                &[LocalWrite],
                NotApplicable,
            ),
            Box::new(|args, c, _| {
                let element = element_arg(args)?;
                let node = c.uia.describe(&element).map_err(uia_error)?;
                c.uia.select(&element).map_err(uia_error)?;
                Ok(Output::new(
                    text::tf("reply.uia.selected", &[("name", &label(&node))]),
                    json!({ "element": element.0 }),
                ))
            }),
        )
        .targets(Box::new(window_targets))
        .build(),
        b.tool(
            &def(
                "uia.expand",
                "Expand (or collapse) a menu, combo box or tree item.",
                object(
                    json!({
                        "element": { "type": "string" },
                        "expand": { "type": "boolean", "description": "false to collapse" }
                    }),
                    &["element"],
                ),
                Risk::Low,
                &[LocalWrite],
                Undoable,
            ),
            Box::new(|args, c, _| {
                let element = element_arg(args)?;
                let expand = args["expand"].as_bool().unwrap_or(true);
                c.uia.expand(&element, expand).map_err(uia_error)?;
                let key = if expand { "reply.uia.expanded" } else { "reply.uia.collapsed" };
                Ok(Output::new(text::t(key), json!({ "element": element.0, "expand": expand })))
            }),
        )
        .undo(Box::new(|data, c| {
            let element = element_arg(data)?;
            let expanded = data["expand"].as_bool().unwrap_or(true);
            c.uia.expand(&element, !expanded).map_err(uia_error)?;
            Ok(Output::new(text::t("reply.undone"), json!({ "element": element.0 })))
        }))
        .targets(Box::new(window_targets))
        .build(),
        b.tool(
            &def(
                "uia.scroll_into_view",
                "Scroll a list or page until the element is visible.",
                element_param(),
                Risk::Low,
                &[LocalWrite],
                NotApplicable,
            ),
            Box::new(|args, c, _| {
                let element = element_arg(args)?;
                c.uia.scroll_into_view(&element).map_err(uia_error)?;
                Ok(Output::new(text::t("reply.okay"), json!({ "element": element.0 })))
            }),
        )
        .targets(Box::new(window_targets))
        .build(),
        b.tool(
            &def(
                "uia.get_bounds",
                "Where an element is on screen, in physical pixels.",
                element_param(),
                Risk::Safe,
                &[LocalRead],
                NotApplicable,
            ),
            Box::new(|args, c, _| {
                let element = element_arg(args)?;
                let r = c.uia.bounds(&element).map_err(uia_error)?;
                Ok(Output::new(
                    String::new(),
                    json!({ "element": element.0, "bounds": r }),
                ))
            }),
        )
        .targets(Box::new(window_targets))
        .build(),
        b.tool(
            &def(
                "context.selection",
                "The text the user has selected right now, in the app or browser tab in front (on-demand context).",
                object(json!({}), &[]),
                Risk::Low,
                &[LocalRead],
                NotApplicable,
            ),
            Box::new(|_, c, _| {
                let window = window_of(&Value::Null, c)?;
                // A browser with KIVO's extension: the page's own selection.
                let is_browser = c.apps.find_by_exe(&window.app_id).is_some_and(|a| a.browser);
                let selected = if is_browser && c.browser.connected() {
                    Some(c.browser.selection(None)?).filter(|s| !s.is_empty())
                } else {
                    c.uia.selected_text().map_err(uia_error)?
                };
                let say = if selected.is_some() { String::new() } else { text::t("reply.context.noSelection") };
                Ok(Output::new(say, json!({ "text": selected })).untrusted(window_source(&window)))
            }),
        )
        .targets(Box::new(window_targets))
        .build(),
        b.tool(
            &Def {
                timeout_ms: 35_000,
                ..def(
                    "uia.wait",
                    "Wait for a window to open or close, focus to move, or a window's content to change (event-driven, up to 30 s).",
                    object(
                        json!({
                            "event": { "type": "string", "enum": ["windowOpened", "windowClosed", "focusChanged", "structureChanged", "propertyChanged"] },
                            "name": { "type": "string", "description": "Only events whose name contains this" },
                            "window": { "type": "string", "description": "Required for structureChanged and propertyChanged" },
                            "timeoutMs": { "type": "integer", "minimum": 100, "maximum": 30000 }
                        }),
                        &["event"],
                    ),
                    Risk::Safe,
                    &[LocalRead],
                    NotApplicable,
                )
            },
            Box::new(wait),
        )
        .targets(Box::new(window_targets))
        .build(),
    ]
}

/// A control's name for replies ("Export" for "E&xport…", or its role when unnamed).
pub(crate) fn label(node: &UiNode) -> String {
    let name = node
        .name
        .replace('&', "")
        .trim()
        .trim_end_matches(['…', ':', '.'])
        .trim()
        .to_owned();
    if name.is_empty() {
        node.role.clone()
    } else {
        name
    }
}

/// `uia.wait`: subscribes, waits for the first matching event (or the deadline or
/// cancellation), unsubscribes. Nothing stays subscribed afterwards.
fn wait(
    args: &Value,
    c: &Controls,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<Output, ToolError> {
    let kind: UiEventKind =
        serde_json::from_value(args["event"].clone()).map_err(|_| missing("event"))?;
    let window = match kind {
        UiEventKind::StructureChanged | UiEventKind::PropertyChanged => {
            Some(window_of(args, c)?.id)
        }
        _ => args["window"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .map(kivo_platform::WindowId),
    };
    let wanted = args["name"].as_str().map(str::to_lowercase);
    let timeout = args["timeoutMs"]
        .as_u64()
        .map_or(Duration::from_secs(10), Duration::from_millis)
        .min(MAX_WAIT);
    let (tx, rx) = std::sync::mpsc::channel::<UiEvent>();
    let tx = std::sync::Mutex::new(tx);
    let sub = c
        .uia
        .subscribe(
            &UiSubscription { kind, window },
            Box::new(move |e| {
                let _ = tx.lock().map(|tx| tx.send(e));
            }),
        )
        .map_err(uia_error)?;
    let deadline = std::time::Instant::now() + timeout;
    let found = loop {
        if cancel.is_cancelled() {
            let _ = c.uia.unsubscribe(sub);
            return Err(ToolError::new(
                ToolErrorCode::Cancelled,
                text::t("reply.cancelled"),
            ));
        }
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            break None;
        }
        match rx.recv_timeout(left.min(Duration::from_millis(100))) {
            Ok(event) => {
                let name = match &event {
                    UiEvent::FocusChanged { name, .. } | UiEvent::WindowOpened { name, .. } => {
                        name.to_lowercase()
                    }
                    UiEvent::PropertyChanged { value, .. } => value.to_lowercase(),
                    _ => String::new(),
                };
                if wanted.as_ref().is_none_or(|w| name.contains(w.as_str())) {
                    break Some(event);
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break None,
        }
    };
    let _ = c.uia.unsubscribe(sub);
    match found {
        Some(event) => {
            let source = match &event {
                UiEvent::FocusChanged { element, .. }
                | UiEvent::WindowOpened { element, .. }
                | UiEvent::PropertyChanged { element, .. } => source_of(c, element),
                _ => text::t("source.window"),
            };
            Ok(Output::new(String::new(), json!({ "event": event })).untrusted(source))
        }
        None => Err(ToolError::new(
            ToolErrorCode::Timeout,
            text::t("error.uia.noEvent"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::rig;

    fn run(r: &crate::testing::Rig, id: &str, args: Value) -> Result<Output, ToolError> {
        r.tool(id).run(&args)
    }

    #[test]
    fn find_and_act_on_controls() {
        let r = rig();
        let found = run(&r, "uia.find", json!({ "name": "export" })).unwrap();
        let el = found.data["elements"][0]["element"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(found.source.is_some(), "window content is untrusted");
        let out = run(&r, "uia.invoke", json!({ "element": el })).unwrap();
        assert_eq!(out.say, "Pressed Export.");
        assert_eq!(
            r.uia.invoked.lock().unwrap().as_slice(),
            std::slice::from_ref(&el)
        );
    }

    #[test]
    fn password_fields_are_never_typed_into_or_read() {
        let r = rig();
        let found = run(
            &r,
            "uia.find",
            json!({ "role": "Edit", "name": "password" }),
        )
        .unwrap();
        let first = &found.data["elements"][0];
        assert_eq!(first["password"], true);
        assert!(first.get("value").is_none());
        let el = first["element"].as_str().unwrap();
        let e = run(&r, "uia.set_value", json!({ "element": el, "value": "x" })).unwrap_err();
        assert_eq!(e.code, ToolErrorCode::AccessDenied);
        assert!(r.uia.values.lock().unwrap().is_empty());
    }

    #[test]
    fn set_value_and_toggle_undo() {
        let r = rig();
        let tool = r.tool("uia.set_value");
        let el = r.uia.element("101");
        let done = tool.run(&json!({ "element": el, "value": "Ada" })).unwrap();
        assert_eq!(r.uia.value_of("101"), "Ada");
        tool.undo(&done.data).unwrap();
        assert_eq!(r.uia.value_of("101"), "");
        let toggle = r.tool("uia.toggle");
        let el = r.uia.element("104");
        let done = toggle.run(&json!({ "element": el })).unwrap();
        assert_eq!(done.data["on"], true);
        toggle.undo(&done.data).unwrap();
        assert!(!r.uia.toggled("104"));
    }

    #[test]
    fn trees_are_compact_and_custom_drawn_windows_are_flagged() {
        let r = rig();
        let out = run(&r, "uia.get_tree", json!({})).unwrap();
        assert_eq!(out.data["customDrawn"], false);
        assert!(out.data["nodes"].as_u64().unwrap() > 3);
        r.uia.empty.store(true, std::sync::atomic::Ordering::SeqCst);
        let out = run(&r, "uia.get_tree", json!({})).unwrap();
        assert_eq!(out.data["customDrawn"], true);
    }

    #[test]
    fn waiting_is_event_driven_and_unsubscribes() {
        let r = rig();
        let uia = Arc::clone(&r.uia);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            uia.fire(UiEvent::WindowOpened {
                element: kivo_platform::ElementRef("7:1".into()),
                name: "Export".into(),
            });
        });
        let out = run(
            &r,
            "uia.wait",
            json!({ "event": "windowOpened", "name": "export", "timeoutMs": 3000 }),
        )
        .unwrap();
        assert_eq!(out.data["event"]["name"], "Export");
        assert_eq!(r.uia.subscriptions(), 0, "nothing stays subscribed");
        let e = run(
            &r,
            "uia.wait",
            json!({ "event": "windowOpened", "timeoutMs": 100 }),
        )
        .unwrap_err();
        assert_eq!(e.code, ToolErrorCode::Timeout);
        assert_eq!(r.uia.subscriptions(), 0);
    }

    #[test]
    fn the_selection_is_read_on_demand_and_untrusted() {
        let r = rig();
        let out = run(&r, "context.selection", json!({})).unwrap();
        assert_eq!(out.data["text"], Value::Null);
        *r.uia.selection.lock().unwrap() = Some("fn main() {}".into());
        let out = run(&r, "context.selection", json!({})).unwrap();
        assert_eq!(out.data["text"], "fn main() {}");
        assert!(out.source.is_some());
    }

    #[test]
    fn targets_name_the_app_behind_an_element() {
        let r = rig();
        let el = r.uia.element("103");
        let t = r.tool("uia.invoke").targets(&json!({ "element": el }));
        assert!(
            matches!(&t[0], kivo_core::tool::Target::App { name, .. } if name == "kivo-test-app")
        );
    }

    #[test]
    fn a_controls_place_in_its_window_in_words() {
        let window = kivo_platform::Rect {
            x: 100,
            y: 100,
            width: 900,
            height: 600,
        };
        let at = |x, y| kivo_platform::Rect {
            x,
            y,
            width: 60,
            height: 24,
        };
        assert_eq!(location_in(&window, &at(900, 110)), "topRight");
        assert_eq!(location_in(&window, &at(520, 380)), "center");
        assert_eq!(location_in(&window, &at(110, 660)), "bottomLeft");
        assert_eq!(location_in(&window, &at(520, 110)), "top");
    }
}
