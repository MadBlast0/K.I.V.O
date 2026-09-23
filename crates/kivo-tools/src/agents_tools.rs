//! Visible terminal agents (CONVERSATION §5.2, CONV-14): KIVO opens Windows Terminal in a folder
//! with an agent CLI in the mode the user asked for (the flags come from the app registry and are
//! checked against the installed CLI's own `--help` first; a mode that skips the agent's
//! permission checks is always High risk), tracks the session by its window title, types a prompt
//! into it only after the user's "send" (the Draft card, CONV-15, confirms every send) and reads
//! the reply back from the window's text.

use crate::appreg::AppEntry;
use crate::builtin::{Def, object, platform_error};
use crate::controls::{Builder, Controls, missing, str_arg};
use crate::registry::{Output, Tool};
use crate::screen_tools::excerpt;
use kivo_core::capability::Capability;
use kivo_core::text;
use kivo_core::tool::{
    CapabilityTier, Initiator, Reversibility, Risk, SideEffect, Target, ToolError, ToolErrorCode,
};
use kivo_platform::{CommandSpec, ShellKind, WindowInfo};
use serde::Serialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A terminal agent session KIVO started (CONV-14).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSession {
    pub id: String,
    /// The registry entry (`claude-code`).
    pub agent: String,
    /// "Claude Code".
    pub name: String,
    /// The window's title, how KIVO finds it again: "Claude Code · K.I.V.O".
    pub title: String,
    pub cwd: PathBuf,
    pub mode: String,
    pub started: i64,
}

#[derive(Default)]
pub struct TerminalSessions(Mutex<Vec<TerminalSession>>);

impl TerminalSessions {
    pub fn list(&self) -> Vec<TerminalSession> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn add(&self, session: TerminalSession) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(session);
    }

    /// By id, by agent name ("Claude"), or the newest.
    pub fn find(&self, wanted: Option<&str>) -> Option<TerminalSession> {
        let all = self.list();
        match wanted.map(str::trim).filter(|w| !w.is_empty()) {
            None => all.last().cloned(),
            Some(w) => {
                let w = w.to_lowercase();
                all.iter()
                    .rev()
                    .find(|s| {
                        s.id == w
                            || s.agent == w
                            || s.name.to_lowercase().contains(&w)
                            || s.title.to_lowercase().contains(&w)
                    })
                    .cloned()
            }
        }
    }
}

fn failed(key: &str) -> ToolError {
    ToolError::new(ToolErrorCode::Failed, text::t(key))
}

/// The registry entry for an agent the user named ("Claude", "codex").
fn agent_entry<'a>(c: &'a Controls, name: &str) -> Option<&'a AppEntry> {
    let lower = name.trim().to_lowercase();
    c.apps
        .entries()
        .iter()
        .filter(|e| e.agent.is_some())
        .find(|e| e.is_named(&lower) || e.id == lower || e.name.to_lowercase().starts_with(&lower))
}

/// The folder: an absolute path that exists, or a remembered workspace's name.
fn folder(c: &Controls, raw: Option<&str>) -> Result<PathBuf, ToolError> {
    let Some(raw) = raw.map(str::trim).filter(|r| !r.is_empty()) else {
        return Ok(c.shell_home.clone());
    };
    if let Ok(p) = crate::files::safe_path(raw)
        && p.is_dir()
    {
        return Ok(p);
    }
    (c.workspace_folder)(raw).ok_or_else(|| {
        ToolError::new(
            ToolErrorCode::NotFound,
            text::tf("error.agents.noFolder", &[("name", &raw)]),
        )
    })
}

/// The flags this mode needs, checked against the installed CLI's `--help` (the registry's
/// flags are the vendors'; a version without one is refused rather than guessed).
fn mode_flags(
    c: &Controls,
    entry: &AppEntry,
    mode: &str,
    cwd: &Path,
) -> Result<Vec<String>, ToolError> {
    let launch = entry
        .agent
        .as_ref()
        .ok_or_else(|| failed("error.agents.notAgent"))?;
    let flags = launch.modes.get(mode).cloned().ok_or_else(|| {
        ToolError::new(
            ToolErrorCode::InvalidArgs,
            text::tf(
                "error.agents.noMode",
                &[("name", &entry.name), ("mode", &mode)],
            ),
        )
    })?;
    let dashed: Vec<&String> = flags.iter().filter(|f| f.starts_with('-')).collect();
    if dashed.is_empty() {
        return Ok(flags);
    }
    let help = c
        .commands
        .run(
            &CommandSpec {
                command: format!("{} --help", launch.command),
                shell: ShellKind::Cmd,
                cwd: cwd.to_path_buf(),
                timeout: Duration::from_secs(15),
                env: Vec::new(),
                max_memory_mb: 512,
                max_cpu_percent: 50,
            },
            &|| false,
        )
        .map_err(platform_error)?;
    let listed = format!("{}\n{}", help.stdout, help.stderr);
    if let Some(missing_flag) = dashed.iter().find(|f| !listed.contains(f.as_str())) {
        return Err(ToolError::new(
            ToolErrorCode::Unsupported,
            text::tf(
                "error.agents.flagMissing",
                &[("name", &entry.name), ("flag", missing_flag)],
            ),
        ));
    }
    Ok(flags)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(0))
}

/// The session's window, found by its title.
fn session_window(c: &Controls, session: &TerminalSession) -> Result<WindowInfo, ToolError> {
    c.windows
        .list()
        .map_err(platform_error)?
        .into_iter()
        .find(|w| w.title.contains(&session.title))
        .ok_or_else(|| {
            ToolError::new(
                ToolErrorCode::NotFound,
                text::tf("error.agents.windowGone", &[("name", &session.name)]),
            )
        })
}

pub(crate) fn tools(c: &Arc<Controls>) -> Vec<Arc<dyn Tool>> {
    let b = Builder::new(c);
    vec![
        b.tool(
            &Def {
                id: "agents.open_terminal",
                description: "Open Windows Terminal in a folder (an absolute path or a remembered workspace's name) and start a CLI agent (Claude Code, Codex, Gemini CLI) there in a mode: default, plan, accept-edits or bypass. Bypass lets the agent act without asking.",
                params: object(
                    json!({
                        "agent": { "type": "string" },
                        "folder": { "type": "string" },
                        "mode": { "type": "string", "enum": ["default", "plan", "accept-edits", "bypass"] }
                    }),
                    &["agent"],
                ),
                risk: Risk::Medium,
                effects: &[SideEffect::LocalWrite],
                capability: Capability::CliAgents,
                reversibility: Reversibility::NotApplicable,
                egress: false,
                timeout_ms: 30_000,
                tier: CapabilityTier::AppCli,
            },
            Box::new(|args, c, _| {
                let name = str_arg(args, "agent")?;
                let entry = agent_entry(c, name).ok_or_else(|| {
                    ToolError::new(
                        ToolErrorCode::NotFound,
                        text::tf("error.agents.unknown", &[("name", &name)]),
                    )
                })?;
                let mode = args["mode"].as_str().unwrap_or("default");
                let cwd = folder(c, args["folder"].as_str())?;
                let flags = mode_flags(c, entry, mode, &cwd)?;
                let launch = entry.agent.as_ref().ok_or_else(|| failed("error.agents.notAgent"))?;
                let place = cwd
                    .file_name()
                    .map_or_else(|| cwd.display().to_string(), |n| n.to_string_lossy().into_owned());
                let title = format!("{} · {place}", entry.name);
                c.terminals
                    .open(&cwd, &title, &launch.command, &flags)
                    .map_err(platform_error)?;
                let session = TerminalSession {
                    id: format!("term-{}", now_ms()),
                    agent: entry.id.clone(),
                    name: entry.name.clone(),
                    title: title.clone(),
                    cwd: cwd.clone(),
                    mode: mode.to_owned(),
                    started: now_ms(),
                };
                c.terminal_sessions.add(session.clone());
                Ok(Output::new(
                    text::tf("reply.agents.opened", &[("name", &entry.name), ("place", &place)]),
                    json!({ "session": session.id, "title": title, "cwd": cwd }),
                ))
            }),
        )
        // SECURITY §1.1, CONVERSATION §5.2: a mode that skips the agent's own checks is High.
        .assess(Box::new(|args: &Value, _: Initiator, c: &Controls| {
            let mode = args["mode"].as_str().unwrap_or("default");
            let bypass = args["agent"]
                .as_str()
                .and_then(|n| agent_entry(c, n))
                .and_then(|e| e.agent.as_ref())
                .is_some_and(|a| a.bypass_modes.iter().any(|m| m == mode))
                || mode == "bypass";
            if bypass { Risk::High } else { Risk::Medium }
        }))
        .targets(Box::new(|args: &Value, c: &Controls| {
            args["agent"]
                .as_str()
                .and_then(|n| agent_entry(c, n))
                .map(|e| {
                    vec![Target::App {
                        id: e.id.clone(),
                        name: e.name.clone(),
                    }]
                })
                .unwrap_or_default()
        }))
        .build(),
        b.tool(
            &Def {
                id: "agents.send_prompt",
                description: "Type a prompt into a terminal agent session KIVO opened and press Enter. The user sees it as a draft first and says send (or edits it).",
                params: object(
                    json!({
                        "session": { "type": "string", "description": "The session's id or the agent's name; the newest by default" },
                        "text": { "type": "string" }
                    }),
                    &["text"],
                ),
                risk: Risk::Medium,
                effects: &[SideEffect::LocalWrite],
                capability: Capability::ComputerUse,
                // Once typed into another AI, it can't be taken back: always a draft first.
                reversibility: Reversibility::Irreversible,
                egress: false,
                timeout_ms: 15_000,
                tier: CapabilityTier::Input,
            },
            Box::new(|args, c, _| {
                let prompt = str_arg(args, "text")?;
                let session = c
                    .terminal_sessions
                    .find(args["session"].as_str())
                    .ok_or_else(|| failed("error.agents.noSession"))?;
                let window = session_window(c, &session)?;
                c.windows.focus(window.id).map_err(platform_error)?;
                c.clipboard.write_text(prompt).map_err(platform_error)?;
                c.input
                    .press(&["Ctrl".into(), "V".into()])
                    .map_err(platform_error)?;
                c.input.press(&["Enter".into()]).map_err(platform_error)?;
                Ok(Output::new(
                    text::tf("reply.agents.sent", &[("name", &session.name)]),
                    json!({ "session": session.id }),
                ))
            }),
        )
        .targets(Box::new(|args: &Value, c: &Controls| {
            c.terminal_sessions
                .find(args["session"].as_str())
                .map(|s| {
                    vec![Target::Window {
                        title: s.title,
                        app_id: String::new(),
                    }]
                })
                .unwrap_or_default()
        }))
        .build(),
        b.tool(
            &Def {
                id: "agents.read_reply",
                description: "Read the latest text in a terminal agent session's window (what the agent answered).",
                params: object(
                    json!({ "session": { "type": "string" } }),
                    &[],
                ),
                risk: Risk::Low,
                effects: &[SideEffect::LocalRead],
                capability: Capability::UiAutomation,
                reversibility: Reversibility::NotApplicable,
                egress: false,
                timeout_ms: 15_000,
                tier: CapabilityTier::Uia,
            },
            Box::new(|args, c, _| {
                let session = c
                    .terminal_sessions
                    .find(args["session"].as_str())
                    .ok_or_else(|| failed("error.agents.noSession"))?;
                let window = session_window(c, &session)?;
                let tree = c.uia.tree(window.id, 12, 600).map_err(platform_error)?;
                let all = excerpt(&tree, 60_000);
                // The end of the window is the latest reply.
                let tail: String = {
                    let chars: Vec<char> = all.chars().collect();
                    chars[chars.len().saturating_sub(3_000)..].iter().collect()
                };
                if tail.trim().is_empty() {
                    return Err(missing("text"));
                }
                Ok(Output::new(String::new(), json!({ "text": tail }))
                    .untrusted(session.title))
            }),
        )
        .build(),
    ]
}

#[cfg(test)]
mod tests {
    use crate::testing::rig;
    use kivo_core::tool::{Initiator, Risk, ToolErrorCode};
    use serde_json::json;

    #[test]
    fn opening_an_agent_checks_its_flags_and_bypass_is_high() {
        let r = rig();
        let project = r.dir.path().join("K.I.V.O");
        std::fs::create_dir_all(&project).unwrap();
        let open = r.tool("agents.open_terminal");
        let args = json!({ "agent": "Claude", "folder": project, "mode": "bypass" });
        assert_eq!(open.assess(&args, Initiator::Brain), Risk::High);
        assert_eq!(
            open.assess(
                &json!({ "agent": "Claude", "mode": "plan" }),
                Initiator::Brain
            ),
            Risk::Medium
        );
        // This version's --help doesn't list the flag: refused, not guessed.
        r.commands.reply.lock().unwrap().stdout = "Usage: claude [options]\n  --resume".into();
        let e = open.run(&args).unwrap_err();
        assert_eq!(e.code, ToolErrorCode::Unsupported);
        assert!(r.terminals.opened.lock().unwrap().is_empty());
        r.commands.reply.lock().unwrap().stdout =
            "  --dangerously-skip-permissions  Bypass all permission checks".into();
        let out = open.run(&args).unwrap();
        assert_eq!(out.say, "Claude Code is starting in K.I.V.O.");
        let opened = r.terminals.opened.lock().unwrap().clone();
        assert_eq!(
            opened,
            [(
                project.clone(),
                "Claude Code · K.I.V.O".to_owned(),
                "claude".to_owned(),
                vec!["--dangerously-skip-permissions".to_owned()]
            )]
        );
        // A workspace's name works as the folder too.
        let e = open
            .run(&json!({ "agent": "codex", "folder": "Nowhere" }))
            .unwrap_err();
        assert_eq!(e.code, ToolErrorCode::NotFound);
    }

    #[test]
    fn prompts_go_into_the_sessions_window_and_replies_are_read_back() {
        let r = rig();
        let project = r.dir.path().join("app");
        std::fs::create_dir_all(&project).unwrap();
        r.tool("agents.open_terminal")
            .run(&json!({ "agent": "codex", "folder": project }))
            .unwrap();
        let send = r.tool("agents.send_prompt");
        assert_eq!(
            send.spec().reversibility,
            kivo_core::tool::Reversibility::Irreversible,
            "always a draft first"
        );
        // The window isn't open yet: nothing is typed anywhere.
        let e = send.run(&json!({ "text": "add tests" })).unwrap_err();
        assert_eq!(e.code, ToolErrorCode::NotFound);
        assert!(r.input.actions.lock().unwrap().is_empty());
        r.add_window(
            kivo_testkit::control::APP_WINDOW.0,
            "Codex · app",
            "WindowsTerminal.exe",
        );
        send.run(&json!({ "text": "add tests" })).unwrap();
        assert_eq!(
            r.clipboard.text.lock().unwrap().as_deref(),
            Some("add tests")
        );
        let actions = r.input.actions.lock().unwrap().clone();
        assert_eq!(actions.len(), 2, "{actions:?}");
        let read = r.tool("agents.read_reply").run(&json!({})).unwrap();
        assert!(
            read.source.is_some(),
            "an agent's reply is untrusted content"
        );
    }
}
