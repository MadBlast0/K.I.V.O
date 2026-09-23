//! The tool router (TOOLS_AND_CONTROL §2, TOOL-04) and the local integration paths (INT-01).
//!
//! `control.act` takes what the user wants done in an app ("click Export in the test app", "type
//! the address into the search box") and walks the capability ladder for that app: its preferred
//! tiers from the App Capability Registry first, then Native → URI → App CLI → UIA → Browser DOM →
//! Vision → Input. A tier whose capability is off is skipped; the first that works wins, and the
//! result says which tier did it and what was tried.
//!
//! `apps.cli` runs a CLI verb a registry entry declares (`code`, `wt`, `git`, `gh`) with that
//! verb's own risk; `apps.open_uri` opens a URI only for a scheme a registry entry declares.

use crate::appreg::{AppEntry, Tier, fill};
use crate::builtin::{Def, object, platform_error};
use crate::controls::{Builder, Controls, app_target, missing, str_arg, window_source};
use crate::registry::{Output, Tool};
use crate::screen_tools::excerpt;
use kivo_core::capability::Capability;
use kivo_core::text;
use kivo_core::tool::{
    CapabilityTier, Initiator, Reversibility, Risk, SideEffect, Target, ToolError, ToolErrorCode,
};
use kivo_platform::{
    CaptureTarget, CommandSpec, ElementQuery, MouseButton, ShellKind, UiAction, UiNode, WindowInfo,
};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

/// What `control.act` can do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Click,
    Type,
    Read,
}

fn action_arg(args: &Value) -> Result<Action, ToolError> {
    match args["action"].as_str() {
        Some("click" | "press" | "invoke") => Ok(Action::Click),
        Some("type" | "fill" | "enter") => Ok(Action::Type),
        Some("read") => Ok(Action::Read),
        _ => Err(missing("action")),
    }
}

/// The app a call names, else the one in front, with its window.
fn resolve(args: &Value, c: &Controls) -> Result<(Option<AppEntry>, WindowInfo), ToolError> {
    let windows = c.windows.list().map_err(platform_error)?;
    if let Some(name) = args["app"].as_str().filter(|s| !s.trim().is_empty()) {
        let entry = c.apps.find_by_name(name).cloned();
        let window = windows
            .iter()
            .find(|w| match &entry {
                Some(e) => e.is_exe(&w.app_id),
                None => {
                    let stem = std::path::Path::new(&w.app_id)
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_lowercase())
                        .unwrap_or_default();
                    stem == name.to_lowercase()
                        || w.title.to_lowercase().contains(&name.to_lowercase())
                }
            })
            .cloned()
            .ok_or_else(|| {
                ToolError::new(
                    ToolErrorCode::NotFound,
                    text::tf("error.router.notOpen", &[("name", &name)]),
                )
            })?;
        return Ok((entry, window));
    }
    let front = c
        .windows
        .foreground()
        .map_err(platform_error)?
        .ok_or_else(|| missing("app"))?;
    Ok((c.apps.find_by_exe(&front.app_id).cloned(), front))
}

fn capability_of(tier: Tier) -> Option<Capability> {
    match tier {
        Tier::Uia => Some(Capability::UiAutomation),
        Tier::BrowserDom => Some(Capability::BrowserPages),
        Tier::Vision => Some(Capability::ScreenAwareness),
        Tier::Input => Some(Capability::ComputerUse),
        Tier::AppCli | Tier::Uri | Tier::Native => None,
    }
}

/// The hard limit before asking (SECURITY §1.1): typing whose target, or the focused field when
/// none is named, is a password field. Read-only lookups; failures are left for `run`.
fn password_target(args: &Value, c: &Controls) -> Result<(), ToolError> {
    if action_arg(args).ok() != Some(Action::Type) {
        return Ok(());
    }
    let refused = || {
        Err(ToolError::new(
            ToolErrorCode::AccessDenied,
            text::t("error.uia.password"),
        ))
    };
    match args["target"].as_str().filter(|t| !t.trim().is_empty()) {
        Some(target) => {
            let Ok((entry, window)) = resolve(args, c) else {
                return Ok(());
            };
            match find_control(c, entry.as_ref(), &window, target, Action::Type) {
                Ok(Some(node)) if node.is_password => refused(),
                _ => Ok(()),
            }
        }
        None if c
            .uia
            .focused()
            .ok()
            .flatten()
            .is_some_and(|n| n.is_password) =>
        {
            refused()
        }
        None => Ok(()),
    }
}

/// Finds the control a target names: the registry's UIA hint first, then by name.
fn find_control(
    c: &Controls,
    entry: Option<&AppEntry>,
    window: &WindowInfo,
    target: &str,
    action: Action,
) -> Result<Option<UiNode>, ToolError> {
    let hint = entry
        .and_then(|e| e.uia.as_ref())
        .and_then(|u| u.hints.get(&target.to_lowercase()));
    let role = match action {
        Action::Type => Some("Edit".to_owned()),
        _ => None,
    };
    let query = match hint {
        Some(h) => ElementQuery {
            window: Some(window.id),
            name: h.name.clone(),
            role: h.role.clone(),
            automation_id: h.automation_id.clone(),
        },
        None => ElementQuery {
            window: Some(window.id),
            name: Some(target.to_owned()),
            role,
            automation_id: None,
        },
    };
    let found = c.uia.find(&query, 5).map_err(platform_error)?;
    // Something that can do the action.
    Ok(found.into_iter().find(|n| match action {
        Action::Click => n.actions.iter().any(|a| {
            matches!(
                a,
                UiAction::Invoke | UiAction::Toggle | UiAction::Select | UiAction::Expand
            )
        }),
        Action::Type => n.actions.contains(&UiAction::SetValue),
        Action::Read => true,
    }))
}

/// One tier's attempt: `Ok(Some(output))` done, `Ok(None)` not applicable here, `Err` failed.
fn try_tier(
    tier: Tier,
    action: Action,
    args: &Value,
    c: &Controls,
    entry: Option<&AppEntry>,
    window: &WindowInfo,
) -> Result<Option<Output>, ToolError> {
    let target = args["target"].as_str().unwrap_or_default();
    let value = args["text"].as_str();
    match tier {
        Tier::Native | Tier::Uri | Tier::AppCli => Ok(None),
        Tier::Uia => {
            if action == Action::Read {
                let tree = c.uia.tree(window.id, 8, 300).map_err(platform_error)?;
                if tree.children.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(
                    Output::new(String::new(), json!({ "text": excerpt(&tree, 8_000) }))
                        .untrusted(window_source(window)),
                ));
            }
            let Some(node) = find_control(c, entry, window, target, action)? else {
                return Ok(None);
            };
            if node.is_password {
                return Err(ToolError::new(
                    ToolErrorCode::AccessDenied,
                    text::t("error.uia.password"),
                ));
            }
            let name = crate::uia_tools::label(&node);
            match action {
                Action::Click => {
                    let a = node
                        .actions
                        .iter()
                        .find(|a| {
                            matches!(
                                a,
                                UiAction::Invoke
                                    | UiAction::Toggle
                                    | UiAction::Select
                                    | UiAction::Expand
                            )
                        })
                        .copied();
                    match a {
                        Some(UiAction::Toggle) => c.uia.toggle(&node.element).map(|_| ()),
                        Some(UiAction::Select) => c.uia.select(&node.element),
                        Some(UiAction::Expand) => c.uia.expand(&node.element, true),
                        _ => c.uia.invoke(&node.element),
                    }
                    .map_err(platform_error)?;
                    Ok(Some(Output::new(
                        text::tf("reply.uia.pressed", &[("name", &name)]),
                        json!({ "element": node.element.0 }),
                    )))
                }
                Action::Type => {
                    let v = value.ok_or_else(|| missing("text"))?;
                    c.uia.set_value(&node.element, v).map_err(platform_error)?;
                    Ok(Some(Output::new(
                        text::tf("reply.uia.typed", &[("name", &name)]),
                        json!({ "element": node.element.0 }),
                    )))
                }
                Action::Read => Ok(None),
            }
        }
        Tier::BrowserDom => {
            if !entry.is_some_and(|e| e.browser) || !c.browser.connected() {
                return Ok(None);
            }
            let page_target = crate::browser::PageTarget {
                selector: None,
                text: Some(target.to_owned()),
            };
            match action {
                Action::Click => {
                    let what = c.browser.click(None, &page_target)?;
                    Ok(Some(Output::new(
                        text::tf("reply.uia.pressed", &[("name", &what)]),
                        json!({ "clicked": what }),
                    )))
                }
                Action::Type => {
                    let v = value.ok_or_else(|| missing("text"))?;
                    c.browser.type_text(None, &page_target, v, false)?;
                    Ok(Some(Output::new(text::t("reply.input.typed"), json!({}))))
                }
                Action::Read => {
                    let page = c.browser.read(None)?;
                    let source = crate::browser::host(&page.url).unwrap_or_default();
                    Ok(Some(
                        Output::new(String::new(), json!({ "text": page.text })).untrusted(source),
                    ))
                }
            }
        }
        Tier::Vision => {
            // Local OCR finds the target's text on screen; clicking it needs the Input tier too.
            let image = c
                .screen
                .capture(CaptureTarget::Window { id: window.id })
                .map_err(platform_error)?;
            let lines = c
                .ocr
                .recognize(&image, &c.language)
                .map_err(platform_error)?;
            if action == Action::Read {
                let joined = lines
                    .iter()
                    .map(|l| l.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                return Ok(Some(
                    Output::new(String::new(), json!({ "text": joined }))
                        .untrusted(window_source(window)),
                ));
            }
            if action != Action::Click || !(c.caps)().enabled(Capability::ComputerUse) {
                return Ok(None);
            }
            let wanted = target.to_lowercase();
            let Some(line) = lines
                .iter()
                .find(|l| !wanted.is_empty() && l.text.to_lowercase().contains(&wanted))
            else {
                return Ok(None);
            };
            // OCR boxes are relative to the window capture.
            let at = kivo_platform::Point {
                x: window.bounds.x + line.bounds.center().x,
                y: window.bounds.y + line.bounds.center().y,
            };
            click_checked(c, window, at)?;
            Ok(Some(Output::new(
                text::tf("reply.uia.pressed", &[("name", &line.text)]),
                json!({ "at": at }),
            )))
        }
        Tier::Input => {
            if action != Action::Type {
                return Ok(None);
            }
            let v = value.ok_or_else(|| missing("text"))?;
            let front = c.windows.foreground().map_err(platform_error)?;
            if front.map(|f| f.id) != Some(window.id) {
                return Err(ToolError::new(
                    ToolErrorCode::Failed,
                    text::t("error.input.notInFront"),
                ));
            }
            if c.uia
                .focused()
                .ok()
                .flatten()
                .is_some_and(|n| n.is_password)
            {
                return Err(ToolError::new(
                    ToolErrorCode::AccessDenied,
                    text::t("error.uia.password"),
                ));
            }
            c.input.type_text(v).map_err(platform_error)?;
            Ok(Some(Output::new(text::t("reply.input.typed"), json!({}))))
        }
    }
}

/// A click after re-validating the window (in front, same place, point inside).
fn click_checked(
    c: &Controls,
    window: &WindowInfo,
    at: kivo_platform::Point,
) -> Result<(), ToolError> {
    let front = c.windows.foreground().map_err(platform_error)?;
    match front {
        Some(f) if f.id == window.id && f.bounds == window.bounds && f.bounds.contains(at) => c
            .input
            .click(at, MouseButton::Left, 1)
            .map_err(platform_error),
        Some(f) if f.id == window.id => Err(ToolError::new(
            ToolErrorCode::Failed,
            text::t("error.input.moved"),
        )),
        _ => Err(ToolError::new(
            ToolErrorCode::Failed,
            text::t("error.input.notInFront"),
        )),
    }
}

/// Quotes one argument for PowerShell (`'…'`, with `'` doubled).
fn ps_quote(arg: &str) -> String {
    format!("'{}'", arg.replace('\'', "''"))
}

fn cli_verb<'a>(
    c: &'a Controls,
    args: &Value,
) -> Result<(&'a AppEntry, String, &'a crate::appreg::CliVerb), ToolError> {
    let app = str_arg(args, "app")?;
    let entry = c
        .apps
        .find_by_name(app)
        .or_else(|| c.apps.get(app))
        .ok_or_else(|| {
            ToolError::new(
                ToolErrorCode::NotFound,
                text::tf("error.notFound", &[("what", &app)]),
            )
        })?;
    let verb_name = str_arg(args, "verb")?.to_owned();
    let cli = entry.cli.as_ref().ok_or_else(|| {
        ToolError::new(
            ToolErrorCode::Unsupported,
            text::tf("error.router.noCli", &[("name", &entry.name)]),
        )
    })?;
    let verb = cli.verbs.get(&verb_name).ok_or_else(|| {
        ToolError::new(
            ToolErrorCode::Unsupported,
            text::tf(
                "error.router.noVerb",
                &[("name", &entry.name), ("verb", &verb_name)],
            ),
        )
    })?;
    Ok((entry, cli.program.clone(), verb))
}

#[allow(clippy::too_many_lines, reason = "one table of tool definitions")]
pub(crate) fn tools(c: &Arc<Controls>) -> Vec<Arc<dyn Tool>> {
    let b = Builder::new(c);
    vec![
        b.tool(
            &Def {
                id: "control.act",
                description: "Do something in an app — click a control, type into a field, or read the window — and let KIVO pick the most reliable way (the app's own tools, UI Automation, the browser, then the screen). Prefer this over the lower-level uia/input tools.",
                params: object(
                    json!({
                        "app": { "type": "string", "description": "The app's name; the app in front by default" },
                        "action": { "type": "string", "enum": ["click", "type", "read"] },
                        "target": { "type": "string", "description": "The control's name or label (for click and type)" },
                        "text": { "type": "string", "description": "What to type" }
                    }),
                    &["action"],
                ),
                risk: Risk::Medium,
                effects: &[SideEffect::LocalWrite],
                capability: Capability::UiAutomation,
                reversibility: Reversibility::NotApplicable,
                egress: false,
                timeout_ms: 30_000,
                tier: CapabilityTier::Uia,
            },
            Box::new(|args, c, cancel| {
                let action = action_arg(args)?;
                if action != Action::Read && args["target"].as_str().is_none_or(|t| t.trim().is_empty()) && action == Action::Click {
                    return Err(missing("target"));
                }
                let (entry, window) = resolve(args, c)?;
                let ladder = entry.as_ref().map_or_else(|| crate::appreg::DEFAULT_LADDER.to_vec(), AppEntry::ladder);
                let caps = (c.caps)();
                let mut tried = Vec::new();
                let mut last_error = None;
                for tier in ladder {
                    if cancel.is_cancelled() {
                        return Err(ToolError::new(ToolErrorCode::Cancelled, text::t("reply.cancelled")));
                    }
                    if let Some(cap) = capability_of(tier)
                        && !caps.enabled(cap)
                    {
                        tried.push(json!({ "tier": tier, "skipped": "capability off" }));
                        continue;
                    }
                    match try_tier(tier, action, args, c, entry.as_ref(), &window) {
                        Ok(Some(mut out)) => {
                            tried.push(json!({ "tier": tier, "ok": true }));
                            if let Some(o) = out.data.as_object_mut() {
                                o.insert("via".into(), json!(tier));
                                o.insert("tried".into(), Value::Array(tried));
                            }
                            return Ok(out);
                        }
                        Ok(None) => tried.push(json!({ "tier": tier, "applies": false })),
                        Err(e) if e.code == ToolErrorCode::AccessDenied => return Err(e),
                        Err(e) => {
                            tried.push(json!({ "tier": tier, "error": e.message }));
                            last_error = Some(e);
                        }
                    }
                }
                Err(last_error.unwrap_or_else(|| {
                    ToolError::new(
                        ToolErrorCode::NotFound,
                        text::tf("error.router.noWay", &[("target", &args["target"].as_str().unwrap_or_default())]),
                    )
                }))
            }),
        )
        .assess(Box::new(|args: &Value, _: Initiator, _: &Controls| match args["action"].as_str() {
            Some("read") => Risk::Safe,
            _ => Risk::Medium,
        }))
        .targets(Box::new(|args: &Value, c: &Controls| {
            resolve(args, c)
                .map(|(_, w)| vec![app_target(&w), Target::Window { title: w.title.clone(), app_id: w.app_id }])
                .unwrap_or_default()
        }))
        .hard_limit(Box::new(password_target))
        .build(),
        b.tool(
            &Def {
                id: "apps.cli",
                description: "Run a command an app's registry entry declares (for example VS Code `open`, Windows Terminal `open`, git `status`, GitHub CLI `prs`) with its arguments.",
                params: object(
                    json!({
                        "app": { "type": "string" },
                        "verb": { "type": "string" },
                        "path": { "type": "string" },
                        "a": { "type": "string" },
                        "b": { "type": "string" },
                        "line": { "type": "integer" }
                    }),
                    &["app", "verb"],
                ),
                risk: Risk::Low,
                effects: &[SideEffect::LocalWrite],
                capability: Capability::AppsAndWindows,
                reversibility: Reversibility::NotApplicable,
                egress: false,
                timeout_ms: 60_000,
                tier: CapabilityTier::AppCli,
            },
            Box::new(|args, c, cancel| {
                let (entry, program, verb) = cli_verb(c, args)?;
                let mut parts = vec![ps_quote(&program)];
                for a in &verb.args {
                    let filled = fill(a, args).ok_or_else(|| missing(a.trim_matches(['{', '}'])))?;
                    // Paths go through the guard.
                    if a.contains("{path}") || a.contains("{a}") || a.contains("{b}") {
                        let p = filled.split(':').take(2).collect::<Vec<_>>().join(":");
                        crate::files::safe_path(&p)?;
                    }
                    parts.push(ps_quote(&filled));
                }
                let cwd = match &verb.cwd {
                    Some(t) => crate::files::safe_path(&fill(t, args).ok_or_else(|| missing("path"))?)?,
                    None => c.shell_home.clone(),
                };
                let spec = CommandSpec {
                    command: format!("& {}", parts.join(" ")),
                    shell: ShellKind::Pwsh,
                    cwd,
                    timeout: Duration::from_secs(55),
                    env: Vec::new(),
                    max_memory_mb: 1024,
                    max_cpu_percent: 50,
                };
                let out = c.commands.run(&spec, &|| cancel.is_cancelled()).map_err(platform_error)?;
                if out.cancelled {
                    return Err(ToolError::new(ToolErrorCode::Cancelled, text::t("reply.cancelled")));
                }
                let ok = out.exit_code == Some(0);
                let say = if ok {
                    text::tf("reply.router.ran", &[("name", &entry.name)])
                } else {
                    text::tf("reply.shell.failed", &[("code", &out.exit_code.unwrap_or(-1))])
                };
                Ok(Output::new(say, json!({ "exitCode": out.exit_code, "stdout": out.stdout, "stderr": out.stderr }))
                    .untrusted(text::t("source.commandOutput")))
            }),
        )
        .assess(Box::new(|args: &Value, _: Initiator, c: &Controls| {
            cli_verb(c, args).map_or(Risk::High, |(_, _, v)| v.risk)
        }))
        .build(),
        b.tool(
            &Def {
                id: "apps.open_uri",
                description: "Open an app link such as spotify:search:jazz or ms-settings:bluetooth (only protocols a known app declares).",
                params: object(json!({ "uri": { "type": "string" } }), &["uri"]),
                risk: Risk::Low,
                effects: &[SideEffect::LocalWrite],
                capability: Capability::AppsAndWindows,
                reversibility: Reversibility::NotApplicable,
                egress: false,
                timeout_ms: 10_000,
                tier: CapabilityTier::OsApi,
            },
            Box::new(|args, c, _| {
                let uri = str_arg(args, "uri")?;
                let entry = c.apps.allows_uri(uri).ok_or_else(|| {
                    ToolError::new(ToolErrorCode::AccessDenied, text::t("error.router.uriRefused"))
                })?;
                (c.open_uri)(uri)?;
                Ok(Output::new(text::tf("reply.opening", &[("name", &entry.name)]), json!({ "uri": uri })))
            }),
        )
        .build(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::rig;
    use kivo_platform::{Rect, TextLine};
    use kivo_testkit::InputAction;

    #[test]
    fn uia_is_used_first_for_a_desktop_app_and_says_so() {
        let r = rig();
        let out = r
            .tool("control.act")
            .run(&json!({ "app": "KIVO Test App", "action": "click", "target": "Export" }))
            .unwrap();
        assert_eq!(out.data["via"], "uia");
        assert_eq!(
            r.uia.invoked.lock().unwrap().as_slice(),
            [r.uia.element("107")]
        );
        // The registry hint maps "export" straight to its AutomationId.
        let out = r
            .tool("control.act")
            .run(&json!({ "action": "type", "target": "name", "text": "Ada" }))
            .unwrap();
        assert_eq!(out.data["via"], "uia");
        assert_eq!(r.uia.value_of("101"), "Ada");
    }

    #[test]
    fn custom_drawn_apps_fall_to_vision_and_input_only_when_allowed() {
        let r = rig();
        r.uia.empty.store(true, std::sync::atomic::Ordering::SeqCst);
        *r.ocr.lines.lock().unwrap() = vec![TextLine {
            text: "Export".into(),
            bounds: Rect {
                x: 40,
                y: 60,
                width: 80,
                height: 20,
            },
        }];
        // Screen awareness and computer use are off by default: nothing works, nothing is clicked.
        let e = r
            .tool("control.act")
            .run(&json!({ "action": "click", "target": "Export" }))
            .unwrap_err();
        assert_eq!(e.code, ToolErrorCode::NotFound);
        assert!(r.input.actions.lock().unwrap().is_empty());
        r.enable(&[Capability::ScreenAwareness, Capability::ComputerUse]);
        let out = r
            .tool("control.act")
            .run(&json!({ "action": "click", "target": "export" }))
            .unwrap();
        assert_eq!(out.data["via"], "vision");
        let tried = out.data["tried"].as_array().unwrap();
        assert!(tried.iter().any(|t| t["tier"] == "uia"));
        // OCR box centre (80, 70) inside the window at (120, 120).
        assert_eq!(
            r.input.actions.lock().unwrap()[0],
            InputAction::Click(
                kivo_platform::Point { x: 200, y: 190 },
                MouseButton::Left,
                1
            )
        );
    }

    #[test]
    fn password_fields_stop_the_ladder() {
        let r = rig();
        let e = r
            .tool("control.act")
            .run(&json!({ "action": "type", "target": "password", "text": "x" }))
            .unwrap_err();
        assert_eq!(e.code, ToolErrorCode::AccessDenied);
        assert!(r.input.actions.lock().unwrap().is_empty());
    }

    #[test]
    fn app_clis_run_declared_verbs_with_quoted_arguments() {
        let r = rig();
        let root = r.tree();
        r.commands.reply.lock().unwrap().exit_code = Some(0);
        let out = r
            .tool("apps.cli")
            .run(&json!({ "app": "VS Code", "verb": "open", "path": root.join("it's here") }))
            .unwrap();
        assert_eq!(out.say, "Ran Visual Studio Code.");
        let ran = r.commands.ran.lock().unwrap();
        assert!(ran[0].command.starts_with("& 'code' '"));
        assert!(ran[0].command.contains("it''s here"), "{}", ran[0].command);
        drop(ran);
        let e = r
            .tool("apps.cli")
            .run(&json!({ "app": "git", "verb": "push" }))
            .unwrap_err();
        assert_eq!(e.code, ToolErrorCode::Unsupported, "only declared verbs");
        assert_eq!(
            r.tool("apps.cli")
                .assess(&json!({ "app": "git", "verb": "status" }), Initiator::Brain),
            Risk::Low
        );
        assert_eq!(
            r.tool("apps.cli")
                .assess(&json!({ "app": "nope", "verb": "x" }), Initiator::Brain),
            Risk::High
        );
    }

    #[test]
    fn only_declared_uri_schemes_open() {
        let r = rig();
        r.tool("apps.open_uri")
            .run(&json!({ "uri": "spotify:search:jazz" }))
            .unwrap();
        assert_eq!(r.uris.lock().unwrap().as_slice(), ["spotify:search:jazz"]);
        for bad in [
            "javascript:alert(1)",
            "file:///C:/Windows",
            "ms-msdt:/id",
            "search-ms:query=x",
        ] {
            let e = r
                .tool("apps.open_uri")
                .run(&json!({ "uri": bad }))
                .unwrap_err();
            assert_eq!(e.code, ToolErrorCode::AccessDenied, "{bad}");
        }
    }

    #[test]
    fn quoting_for_powershell() {
        assert_eq!(ps_quote("a'b"), "'a''b'");
        assert_eq!(ps_quote("$(evil)"), "'$(evil)'");
    }
}
