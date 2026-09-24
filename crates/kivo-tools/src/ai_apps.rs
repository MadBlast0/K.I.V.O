//! Prompting desktop AI apps (CONVERSATION §5.3, CONV-16): Claude Desktop, ChatGPT and Copilot,
//! through each app's registry entry (`[ai]`): where its composer, send button, latest reply and
//! project picker are, for the app version the hints were written against. The flow opens or
//! focuses the app, picks the project, puts the prompt in the composer, sends it and (if asked)
//! waits for the reply. The prompt is shown for review before anything is sent (the tool asks,
//! as it sends the user's words to an AI service). When the app's UI no longer matches (an update
//! moved things), it falls back as far as it got: "I've put the prompt in; press Enter when
//! ready", or the prompt on the clipboard to paste. Nothing is ever typed with synthetic keys.

use crate::appreg::{AppEntry, Hint};
use crate::builtin::{Def, object, platform_error};
use crate::controls::{Builder, Controls, window_source};
use crate::registry::{Output, Tool};
use kivo_core::capability::Capability;
use kivo_core::text;
use kivo_core::tool::{CapabilityTier, Reversibility, Risk, SideEffect, ToolError, ToolErrorCode};
use kivo_platform::{ElementQuery, UiNode, WindowInfo};
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// How long a just-opened app has to show its window.
const OPEN_WAIT: Duration = Duration::from_secs(12);
/// How long KIVO waits for a reply to start and settle.
const REPLY_WAIT: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(250);

/// How far the flow got, for the reply and the fallback (CONV-16).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reached {
    /// The prompt was sent.
    Sent,
    /// The prompt is in the composer; the send button wasn't found.
    Filled,
    /// The composer wasn't found or takes no text: the prompt is on the clipboard.
    Clipboard,
}

impl Reached {
    fn key(self) -> &'static str {
        match self {
            Self::Sent => "sent",
            Self::Filled => "filled",
            Self::Clipboard => "clipboard",
        }
    }
}

fn find(c: &Controls, window: &WindowInfo, hint: &Hint) -> Option<UiNode> {
    let query = ElementQuery {
        window: Some(window.id),
        automation_id: hint.automation_id.clone(),
        role: hint.role.clone(),
        name: hint.name.clone(),
    };
    c.uia.find(&query, 3).ok()?.into_iter().next()
}

/// What a reply element says now (its value, else its name).
fn text_of(c: &Controls, node: &UiNode) -> String {
    c.uia
        .describe(&node.element)
        .map(|n| n.value.unwrap_or(n.name))
        .unwrap_or_default()
}

/// The app's window, opening the app if it isn't running.
fn app_window(
    c: &Controls,
    app: &AppEntry,
    cancel: &CancellationToken,
) -> Result<WindowInfo, ToolError> {
    let find = || {
        c.windows.list().ok().and_then(|list| {
            list.into_iter()
                .find(|w| w.app_id == app.id || app.is_exe(&w.app_id))
        })
    };
    if let Some(w) = find() {
        return Ok(w);
    }
    // The installed app this entry describes: by its program, else by a name it goes by.
    let installed = c
        .launcher
        .installed()
        .map_err(platform_error)?
        .into_iter()
        .find(|i| {
            i.exe.as_deref().is_some_and(|e| app.is_exe(e))
                || app.is_named(&i.name)
                || i.aliases.iter().any(|a| app.is_named(a))
        })
        .ok_or_else(|| {
            ToolError::new(
                ToolErrorCode::NotFound,
                text::tf("reply.ai.notInstalled", &[("name", &app.name)]),
            )
        })?;
    c.launcher.launch(&installed, &[]).map_err(platform_error)?;
    let start = Instant::now();
    while start.elapsed() < OPEN_WAIT {
        if cancel.is_cancelled() {
            return Err(ToolError::new(
                ToolErrorCode::Cancelled,
                text::t("reply.cancelled"),
            ));
        }
        if let Some(w) = find() {
            return Ok(w);
        }
        std::thread::sleep(POLL);
    }
    Err(ToolError::new(
        ToolErrorCode::Timeout,
        text::tf("reply.ai.didntOpen", &[("name", &app.name)]),
    ))
}

/// Runs the flow; returns how far it got and the reply when one was read.
pub fn prompt(
    c: &Controls,
    app: &AppEntry,
    prompt: &str,
    project: Option<&str>,
    send: bool,
    read_reply: bool,
    cancel: &CancellationToken,
) -> Result<(Reached, Option<String>, WindowInfo), ToolError> {
    let hints = app.ai.as_ref().ok_or_else(|| {
        ToolError::new(
            ToolErrorCode::Unsupported,
            text::tf("reply.ai.notAnAiApp", &[("name", &app.name)]),
        )
    })?;
    let window = app_window(c, app, cancel)?;
    let _ = c.windows.focus(window.id);
    // The project, when the app has a picker and one was named.
    if let (Some(name), Some(hint)) = (project, &hints.project)
        && let Some(picker) = find(c, &window, hint)
    {
        let _ = c.uia.expand(&picker.element, true);
        let item = ElementQuery {
            window: Some(window.id),
            name: Some(name.to_owned()),
            ..ElementQuery::default()
        };
        if let Some(choice) = c.uia.find(&item, 1).ok().and_then(|v| v.into_iter().next()) {
            let _ = c
                .uia
                .select(&choice.element)
                .or_else(|_| c.uia.invoke(&choice.element));
        }
    }
    // The prompt in the composer; if that can't take text, on the clipboard to paste.
    let filled = find(c, &window, &hints.composer)
        .is_some_and(|composer| c.uia.set_value(&composer.element, prompt).is_ok());
    if !filled {
        c.clipboard.write_text(prompt).map_err(platform_error)?;
        return Ok((Reached::Clipboard, None, window));
    }
    if !send {
        return Ok((Reached::Filled, None, window));
    }
    let reply_node = hints.reply.as_ref().and_then(|h| find(c, &window, h));
    let before = reply_node.as_ref().map(|n| text_of(c, n));
    let Some(button) = hints.send.as_ref().and_then(|h| find(c, &window, h)) else {
        return Ok((Reached::Filled, None, window));
    };
    if c.uia.invoke(&button.element).is_err() {
        return Ok((Reached::Filled, None, window));
    }
    if !read_reply {
        return Ok((Reached::Sent, None, window));
    }
    // The latest reply: wait for it to change from what it was, then to stop changing.
    let start = Instant::now();
    let mut last: Option<(String, Instant)> = None;
    while start.elapsed() < REPLY_WAIT && !cancel.is_cancelled() {
        std::thread::sleep(POLL);
        let node = hints.reply.as_ref().and_then(|h| find(c, &window, h));
        let Some(now) = node.as_ref().map(|n| text_of(c, n)) else {
            continue;
        };
        if Some(&now) == before.as_ref() || now.trim().is_empty() {
            continue;
        }
        match &last {
            Some((text, since))
                if *text == now && since.elapsed() >= Duration::from_millis(1_500) =>
            {
                return Ok((Reached::Sent, Some(now), window));
            }
            Some((text, _)) if *text == now => {}
            _ => last = Some((now, Instant::now())),
        }
    }
    Ok((Reached::Sent, last.map(|(t, _)| t), window))
}

fn def() -> Def {
    Def {
        id: "ai_apps.prompt",
        description: "Prompt a desktop AI app the user has (Claude Desktop, ChatGPT, Copilot): opens or focuses it, picks a project if named, puts the prompt in and sends it, and can wait for the reply. The user reviews the prompt first.",
        params: object(
            json!({
                "app": { "type": "string", "description": "Claude, ChatGPT or Copilot" },
                "prompt": { "type": "string" },
                "project": { "type": "string", "description": "A project in the app, if it has projects" },
                "send": { "type": "boolean", "description": "Press send (default true)" },
                "readReply": { "type": "boolean", "description": "Wait for the reply and return it" }
            }),
            &["app", "prompt"],
        ),
        risk: Risk::Medium,
        effects: &[SideEffect::ExternalComms],
        capability: Capability::UiAutomation,
        reversibility: Reversibility::Irreversible,
        egress: true,
        timeout_ms: 90_000,
        tier: CapabilityTier::Uia,
    }
}

pub(crate) fn tools(c: &Arc<Controls>) -> Vec<Arc<dyn Tool>> {
    let b = Builder::new(c);
    vec![
        b.tool(
            &def(),
            Box::new(|args, c, cancel| {
                let name = args["app"].as_str().unwrap_or_default();
                let app = c
                    .apps
                    .entries()
                    .iter()
                    .find(|e| e.ai.is_some() && e.is_named(name))
                    .cloned()
                    .ok_or_else(|| {
                        ToolError::new(
                            ToolErrorCode::NotFound,
                            text::tf("reply.ai.notAnAiApp", &[("name", &name)]),
                        )
                    })?;
                let text = args["prompt"].as_str().unwrap_or_default().trim();
                if text.is_empty() {
                    return Err(ToolError::new(
                        ToolErrorCode::InvalidArgs,
                        text::t("reply.ai.noPrompt"),
                    ));
                }
                let (reached, reply, window) = prompt(
                    c,
                    &app,
                    text,
                    args["project"].as_str(),
                    args["send"].as_bool().unwrap_or(true),
                    args["readReply"].as_bool().unwrap_or(false),
                    cancel,
                )?;
                let say = text::tf(
                    &format!("reply.ai.{}", reached.key()),
                    &[("name", &app.name)],
                );
                let out = Output::new(
                    say,
                    json!({ "app": app.name, "reached": reached.key(), "reply": reply }),
                );
                // A reply is the app's content: untrusted, like anything a window shows.
                Ok(if reply.is_some() {
                    out.untrusted(window_source(&window))
                } else {
                    out
                })
            }),
        )
        .build(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::rig;
    use kivo_platform::UiAutomation;

    /// The flow on the dummy app's shape (composer = its address bar, send = Greet, reply = its
    /// name field): the prompt goes in, send is pressed, and the reply is read once it settles.
    #[test]
    fn prompts_an_ai_app_and_reads_its_reply() {
        let r = rig();
        let reply = {
            let uia = Arc::clone(&r.uia);
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(600));
                let name = uia
                    .find(
                        &ElementQuery {
                            automation_id: Some("101".into()),
                            ..ElementQuery::default()
                        },
                        1,
                    )
                    .unwrap()
                    .remove(0);
                uia.values
                    .lock()
                    .unwrap()
                    .insert(name.element.0, "Here's your summary.".into());
            })
        };
        let out = r
            .tool("ai_apps.prompt")
            .run(&json!({ "app": "test app", "prompt": "Summarize my notes", "readReply": true }))
            .unwrap();
        reply.join().unwrap();
        assert_eq!(out.data["reached"], "sent");
        assert_eq!(out.data["reply"], "Here's your summary.");
        assert!(out.source.is_some(), "a reply is the app's content");
        let values = r.uia.values.lock().unwrap().clone();
        assert!(
            values.values().any(|v| v == "Summarize my notes"),
            "{values:?}"
        );
        assert_eq!(r.uia.invoked.lock().unwrap().len(), 1, "send pressed once");
        assert!(
            r.input.actions.lock().unwrap().is_empty(),
            "no synthetic input"
        );
    }

    /// When the app's UI no longer matches: the prompt left in with "press Enter when ready", or
    /// on the clipboard.
    #[test]
    fn falls_back_when_the_app_changed() {
        let r = rig();
        let mut app = r
            .controls
            .apps
            .entries()
            .iter()
            .find(|e| e.id == "kivo-test-app")
            .cloned()
            .unwrap();
        let cancel = CancellationToken::new();
        let hints = app.ai.as_mut().unwrap();
        hints.send = Some(Hint {
            name: Some("Send (moved)".into()),
            ..Hint::default()
        });
        let c = Arc::clone(&r.controls);
        let (reached, _, _) = prompt(&c, &app, "hello", None, true, false, &cancel).unwrap();
        assert_eq!(reached, Reached::Filled);
        app.ai.as_mut().unwrap().composer = Hint {
            name: Some("Composer (moved)".into()),
            ..Hint::default()
        };
        let (reached, _, _) = prompt(&c, &app, "hello again", None, true, false, &cancel).unwrap();
        assert_eq!(reached, Reached::Clipboard);
        assert_eq!(
            r.clipboard.text.lock().unwrap().as_deref(),
            Some("hello again")
        );
        assert!(
            r.tool("ai_apps.prompt")
                .run(&json!({ "app": "notepad", "prompt": "x" }))
                .is_err(),
            "not an AI app"
        );
    }

    #[test]
    fn the_real_apps_have_versioned_hints() {
        let apps = crate::appreg::AppRegistry::core();
        for id in ["claude-desktop", "chatgpt", "copilot"] {
            let e = apps.entries().iter().find(|e| e.id == id).unwrap();
            let ai = e.ai.as_ref().unwrap_or_else(|| panic!("{id} has [ai]"));
            assert!(!ai.version.is_empty() && ai.send.is_some(), "{id}");
            assert!(e.desktop_ai);
        }
    }
}
