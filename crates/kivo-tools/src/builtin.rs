//! KIVO's built-in native tools for M1 (TOOLS_AND_CONTROL §3, §5): apps, windows, audio, media,
//! power, screenshots, notifications and the browser. Each declares its risk, side effects,
//! capability, timeout and reversibility; each talks to the OS only through `kivo-platform`.

use crate::registry::{Output, Tool};
use kivo_core::capability::Capability;
use kivo_core::text;
use kivo_core::tool::{
    CapabilityTier, Platform, Provenance, Reversibility, Risk, SideEffect, Target, ToolError,
    ToolErrorCode, ToolSpec,
};
use kivo_platform::{
    AppEntry, Apps, CaptureTarget, MediaAction, Notification, Notifications, PlatformError,
    PowerAction, Screen, SystemControl, WindowId, Windows,
};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

/// What the built-in tools act through.
pub struct Env {
    pub apps: Arc<dyn Apps>,
    pub windows: Arc<dyn Windows>,
    pub control: Arc<dyn SystemControl>,
    pub screen: Arc<dyn Screen>,
    pub notifications: Arc<dyn Notifications>,
    /// The installed apps as last indexed (the runtime refreshes it).
    pub catalog: Arc<RwLock<Vec<AppEntry>>>,
    /// Where screenshots are saved (Pictures\Screenshots).
    pub screenshots: PathBuf,
}

type Run = dyn Fn(&Value, &Env) -> Result<Output, ToolError> + Send + Sync;

struct Builtin {
    spec: ToolSpec,
    env: Arc<Env>,
    run: Box<Run>,
    /// Takes back what `run` did, from its result data (Undoable tools).
    undo: Option<Box<Run>>,
}

impl Tool for Builtin {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }
    fn run(&self, args: &Value) -> Result<Output, ToolError> {
        (self.run)(args, &self.env)
    }
    fn undo(&self, data: &Value) -> Result<Output, ToolError> {
        match &self.undo {
            Some(undo) => undo(data, &self.env),
            None => Err(ToolError::new(
                ToolErrorCode::Unsupported,
                text::t("error.cantUndo"),
            )),
        }
    }
}

/// A user-safe error for a platform failure (plan §146); the OS detail stays in `detail`.
pub fn platform_error(e: PlatformError) -> ToolError {
    let detail = format!("{e:?}");
    let (code, message) = match e {
        PlatformError::NotFound(what) => (
            ToolErrorCode::NotFound,
            text::tf("error.notFound", &[("what", &what)]),
        ),
        PlatformError::Unsupported => (ToolErrorCode::Unsupported, text::t("error.unsupported")),
        PlatformError::AccessDenied => (ToolErrorCode::AccessDenied, text::t("error.accessDenied")),
        PlatformError::Cancelled => (ToolErrorCode::Cancelled, text::t("reply.cancelled")),
        PlatformError::Elevated(name) => (
            ToolErrorCode::AccessDenied,
            text::tf("error.elevated", &[("name", &name)]),
        ),
        PlatformError::Timeout => (ToolErrorCode::Timeout, text::t("error.appHung")),
        PlatformError::Conflict(_) | PlatformError::Os { .. } => {
            (ToolErrorCode::Failed, text::t("error.failed"))
        }
    };
    ToolError::new(code, message).with_detail(detail)
}

pub(crate) fn done(say: impl Into<String>, data: Value) -> Result<Output, ToolError> {
    Ok(Output::new(say, data))
}

/// A missing argument; `what` names it in `error.missing.*`.
pub(crate) fn invalid(what: &str) -> ToolError {
    ToolError::new(
        ToolErrorCode::InvalidArgs,
        text::t(&format!("error.missing.{what}")),
    )
}

/// `{"app": {"id", "name"}}` → the indexed app.
fn app_arg(args: &Value, env: &Env) -> Result<AppEntry, ToolError> {
    let id = args["app"]["id"].as_str().ok_or_else(|| invalid("app"))?;
    let name = args["app"]["name"].as_str().unwrap_or(id);
    let catalog = env
        .catalog
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    Ok(catalog
        .iter()
        .find(|a| a.id == id)
        .cloned()
        .unwrap_or_else(|| AppEntry {
            id: id.into(),
            name: name.into(),
            aliases: Vec::new(),
            exe: None,
        }))
}

/// `{"window": {"id"}}` → that window, else the foreground window.
fn window_arg(args: &Value, env: &Env) -> Result<(WindowId, String), ToolError> {
    if let Some(id) = args["window"]["id"].as_str().and_then(|s| s.parse().ok()) {
        let title = args["window"]["name"]
            .as_str()
            .unwrap_or("the window")
            .to_owned();
        return Ok((WindowId(id), title));
    }
    let front = env
        .windows
        .foreground()
        .map_err(platform_error)?
        .ok_or_else(|| invalid("window"))?;
    Ok((front.id, front.title))
}

pub(crate) fn number_arg(args: &Value) -> Result<f32, ToolError> {
    let n = args["number"].as_f64().ok_or_else(|| invalid("number"))?;
    #[allow(clippy::cast_possible_truncation, reason = "0–100")]
    Ok((n as f32 / 100.0).clamp(0.0, 1.0))
}

/// What a call acts on, for the hard limits and the confirmation card.
pub fn targets(tool: &str, args: &Value) -> Vec<Target> {
    let mut out = Vec::new();
    if let (Some(id), Some(name)) = (args["app"]["id"].as_str(), args["app"]["name"].as_str()) {
        out.push(Target::App {
            id: id.into(),
            name: name.into(),
        });
    }
    if let Some(title) = args["window"]["name"].as_str() {
        out.push(Target::Window {
            title: title.into(),
            app_id: String::new(),
        });
    }
    if tool.starts_with("browser.")
        && let Some(url) = args["url"].as_str()
    {
        out.push(Target::Destination {
            address: url.into(),
            provenance: Provenance::User,
        });
    }
    out
}

/// A tool's definition. Its title (shown on confirmation cards and in Activity) is
/// `tool.<id>` in the text catalog; the description is for models and stays in English.
pub(crate) struct Def {
    pub id: &'static str,
    pub description: &'static str,
    pub params: Value,
    pub risk: Risk,
    pub effects: &'static [SideEffect],
    pub capability: Capability,
    pub reversibility: Reversibility,
    pub egress: bool,
    pub timeout_ms: u64,
    pub tier: CapabilityTier,
}

pub(crate) fn spec(d: &Def) -> ToolSpec {
    ToolSpec {
        id: d.id.into(),
        description: d.description.into(),
        title: text::t(&format!("tool.{}", d.id)),
        params: d.params.clone(),
        result: json!({ "type": "object" }),
        risk: d.risk,
        side_effects: d.effects.to_vec(),
        data_egress: d.egress,
        timeout_ms: d.timeout_ms,
        cancellable: true,
        tier: d.tier,
        platforms: vec![Platform::Windows],
        reversibility: d.reversibility,
        capability: d.capability,
    }
}

pub(crate) fn object(properties: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": properties, "required": required, "additionalProperties": false })
}

fn app_param() -> Value {
    object(
        json!({ "app": { "type": "object", "properties": { "id": {"type": "string"}, "name": {"type": "string"} }, "required": ["id"] } }),
        &["app"],
    )
}

pub(crate) fn window_param(required: bool) -> Value {
    let props = json!({ "window": { "type": "object", "properties": { "id": {"type": "string"}, "name": {"type": "string"} } } });
    object(props, if required { &["window"] } else { &[] })
}

pub(crate) fn none() -> Value {
    object(json!({}), &[])
}

/// Every built-in tool, acting through `env`.
#[allow(clippy::too_many_lines, reason = "one table of tool definitions")]
pub fn builtin(env: &Arc<Env>) -> Vec<Arc<dyn Tool>> {
    use Capability as C;
    use Reversibility::{Irreversible, NotApplicable, Undoable};
    use SideEffect::{Destructive, ExternalComms, LocalRead, LocalWrite, None as NoEffect};
    let tool = |d: Def, run: Box<Run>| -> Arc<dyn Tool> {
        debug_assert!(d.reversibility != Undoable, "{} needs its undo", d.id);
        Arc::new(Builtin {
            spec: spec(&d),
            env: Arc::clone(env),
            run,
            undo: None,
        })
    };
    let undoable = |d: Def, run: Box<Run>, undo: Box<Run>| -> Arc<dyn Tool> {
        debug_assert!(d.reversibility == Undoable, "{} isn't undoable", d.id);
        Arc::new(Builtin {
            spec: spec(&d),
            env: Arc::clone(env),
            run,
            undo: Some(undo),
        })
    };
    let def = |id, description, params, risk, effects, capability, reversibility| Def {
        id,
        description,
        params,
        risk,
        effects,
        capability,
        reversibility,
        egress: false,
        timeout_ms: 5_000,
        tier: CapabilityTier::OsApi,
    };
    vec![
        tool(
            def(
                "apps.launch",
                "Open an installed app.",
                app_param(),
                Risk::Low,
                &[LocalWrite],
                C::AppsAndWindows,
                NotApplicable,
            ),
            Box::new(|args, env| {
                let app = app_arg(args, env)?;
                env.apps.launch(&app, &[]).map_err(platform_error)?;
                done(
                    text::tf("reply.opening", &[("name", &app.name)]),
                    json!({ "app": app.name }),
                )
            }),
        ),
        tool(
            def(
                "apps.close",
                "Close every window of an app (it may ask to save).",
                app_param(),
                Risk::Medium,
                &[LocalWrite],
                C::AppsAndWindows,
                NotApplicable,
            ),
            Box::new(|args, env| {
                let app = app_arg(args, env)?;
                let closed = env.apps.close(&app).map_err(platform_error)?;
                done(
                    text::tf("reply.closing", &[("name", &app.name)]),
                    json!({ "app": app.name, "windows": closed }),
                )
            }),
        ),
        tool(
            Def {
                timeout_ms: 20_000,
                ..def(
                    "apps.restart",
                    "Close an app and open it again (it may ask to save first).",
                    app_param(),
                    Risk::Medium,
                    &[LocalWrite],
                    C::AppsAndWindows,
                    NotApplicable,
                )
            },
            Box::new(|args, env| {
                let app = app_arg(args, env)?;
                if env.apps.running(&app).map_err(platform_error)? {
                    env.apps.close(&app).map_err(platform_error)?;
                    // Wait for it to go (it may ask to save), then start it again.
                    let deadline = std::time::Instant::now() + RESTART_WAIT;
                    while env.apps.running(&app).map_err(platform_error)? {
                        if std::time::Instant::now() > deadline {
                            return Err(ToolError::new(
                                ToolErrorCode::Timeout,
                                text::tf("error.stillOpen", &[("name", &app.name)]),
                            ));
                        }
                        std::thread::sleep(std::time::Duration::from_millis(250));
                    }
                }
                env.apps.launch(&app, &[]).map_err(platform_error)?;
                done(
                    text::tf("reply.restartingApp", &[("name", &app.name)]),
                    json!({ "app": app.name }),
                )
            }),
        ),
        tool(
            def(
                "windows.focus",
                "Bring a window to the front.",
                window_param(true),
                Risk::Low,
                &[LocalWrite],
                C::AppsAndWindows,
                NotApplicable,
            ),
            Box::new(|args, env| {
                let (id, title) = window_arg(args, env)?;
                env.windows.focus(id).map_err(platform_error)?;
                done(
                    text::tf("reply.switched", &[("title", &title)]),
                    json!({ "window": title }),
                )
            }),
        ),
        undoable(
            def(
                "windows.minimize",
                "Minimize a window (the one in front by default).",
                window_param(false),
                Risk::Low,
                &[LocalWrite],
                C::AppsAndWindows,
                Undoable,
            ),
            Box::new(|args, env| {
                let (id, title) = window_arg(args, env)?;
                env.windows.minimize(id).map_err(platform_error)?;
                done(
                    text::t("reply.minimized"),
                    json!({ "window": title, "windowId": id.0.to_string() }),
                )
            }),
            Box::new(restore_window),
        ),
        undoable(
            def(
                "windows.maximize",
                "Maximize a window (the one in front by default).",
                window_param(false),
                Risk::Low,
                &[LocalWrite],
                C::AppsAndWindows,
                Undoable,
            ),
            Box::new(|args, env| {
                let (id, title) = window_arg(args, env)?;
                env.windows.maximize(id).map_err(platform_error)?;
                done(
                    text::t("reply.maximized"),
                    json!({ "window": title, "windowId": id.0.to_string() }),
                )
            }),
            Box::new(restore_window),
        ),
        tool(
            def(
                "windows.close",
                "Close a window (the one in front by default; it may ask to save).",
                window_param(false),
                Risk::Medium,
                &[LocalWrite],
                C::AppsAndWindows,
                NotApplicable,
            ),
            Box::new(|args, env| {
                let (id, title) = window_arg(args, env)?;
                env.windows.close(id).map_err(platform_error)?;
                done(text::t("reply.closed"), json!({ "window": title }))
            }),
        ),
        undoable(
            def(
                "audio.mute",
                "Mute the speakers.",
                none(),
                Risk::Low,
                &[LocalWrite],
                C::SystemControls,
                Undoable,
            ),
            Box::new(|_, env| {
                let was = env.control.volume().map_err(platform_error)?.muted;
                env.control.set_muted(true).map_err(platform_error)?;
                done(
                    text::t("reply.muted"),
                    json!({ "muted": true, "wasMuted": was }),
                )
            }),
            Box::new(undo_mute),
        ),
        undoable(
            def(
                "audio.unmute",
                "Unmute the speakers.",
                none(),
                Risk::Low,
                &[LocalWrite],
                C::SystemControls,
                Undoable,
            ),
            Box::new(|_, env| {
                let was = env.control.volume().map_err(platform_error)?.muted;
                env.control.set_muted(false).map_err(platform_error)?;
                done(
                    text::t("reply.soundOn"),
                    json!({ "muted": false, "wasMuted": was }),
                )
            }),
            Box::new(undo_mute),
        ),
        undoable(
            def(
                "audio.volume_set",
                "Set the speaker volume (0–100).",
                object(
                    json!({ "number": { "type": "number", "minimum": 0, "maximum": 100 } }),
                    &["number"],
                ),
                Risk::Low,
                &[LocalWrite],
                C::SystemControls,
                Undoable,
            ),
            Box::new(|args, env| {
                let level = number_arg(args)?;
                let before = env.control.volume().map_err(platform_error)?.level;
                env.control.set_volume(level).map_err(platform_error)?;
                let percent = (level * 100.0).round();
                done(
                    text::tf("reply.volume", &[("percent", &percent)]),
                    json!({ "volume": percent, "previous": (before * 100.0).round() }),
                )
            }),
            Box::new(undo_volume),
        ),
        undoable(
            def(
                "audio.volume_up",
                "Raise the volume by 10%.",
                none(),
                Risk::Low,
                &[LocalWrite],
                C::SystemControls,
                Undoable,
            ),
            Box::new(|_, env| step_volume(env, 0.1)),
            Box::new(undo_volume),
        ),
        undoable(
            def(
                "audio.volume_down",
                "Lower the volume by 10%.",
                none(),
                Risk::Low,
                &[LocalWrite],
                C::SystemControls,
                Undoable,
            ),
            Box::new(|_, env| step_volume(env, -0.1)),
            Box::new(undo_volume),
        ),
        undoable(
            def(
                "audio.mic_mute",
                "Mute the default microphone for every app.",
                none(),
                Risk::Low,
                &[LocalWrite],
                C::SystemControls,
                Undoable,
            ),
            Box::new(|_, env| {
                let was = env.control.microphone().map_err(platform_error)?.muted;
                env.control.set_mic_muted(true).map_err(platform_error)?;
                done(
                    text::t("reply.micMuted"),
                    json!({ "micMuted": true, "wasMuted": was }),
                )
            }),
            Box::new(undo_mic),
        ),
        undoable(
            def(
                "audio.mic_unmute",
                "Unmute the default microphone.",
                none(),
                Risk::Low,
                &[LocalWrite],
                C::SystemControls,
                Undoable,
            ),
            Box::new(|_, env| {
                let was = env.control.microphone().map_err(platform_error)?.muted;
                env.control.set_mic_muted(false).map_err(platform_error)?;
                done(
                    text::t("reply.micOn"),
                    json!({ "micMuted": false, "wasMuted": was }),
                )
            }),
            Box::new(undo_mic),
        ),
        undoable(
            def(
                "media.play_pause",
                "Play or pause whatever is playing.",
                none(),
                Risk::Low,
                &[LocalWrite],
                C::SystemControls,
                Undoable,
            ),
            Box::new(|_, env| {
                env.control
                    .media(MediaAction::PlayPause)
                    .map_err(nothing_playing)?;
                done(text::t("reply.okay"), json!({}))
            }),
            // Play/pause is its own undo.
            Box::new(|_, env| {
                env.control
                    .media(MediaAction::PlayPause)
                    .map_err(nothing_playing)?;
                done(text::t("reply.undone"), json!({}))
            }),
        ),
        tool(
            def(
                "media.next",
                "Skip to the next track.",
                none(),
                Risk::Low,
                &[LocalWrite],
                C::SystemControls,
                NotApplicable,
            ),
            Box::new(|_, env| {
                env.control
                    .media(MediaAction::Next)
                    .map_err(nothing_playing)?;
                done(text::t("reply.next"), json!({}))
            }),
        ),
        tool(
            def(
                "media.previous",
                "Go back to the previous track.",
                none(),
                Risk::Low,
                &[LocalWrite],
                C::SystemControls,
                NotApplicable,
            ),
            Box::new(|_, env| {
                env.control
                    .media(MediaAction::Previous)
                    .map_err(nothing_playing)?;
                done(text::t("reply.previous"), json!({}))
            }),
        ),
        tool(
            def(
                "media.now_playing",
                "Report the current track.",
                none(),
                Risk::Safe,
                &[LocalRead],
                C::SystemControls,
                NotApplicable,
            ),
            Box::new(
                |_, env| match env.control.now_playing().map_err(platform_error)? {
                    Some(p) if !p.title.is_empty() => {
                        let say = if p.artist.is_empty() {
                            text::tf("reply.track", &[("title", &p.title)])
                        } else {
                            text::tf(
                                "reply.trackBy",
                                &[("title", &p.title), ("artist", &p.artist)],
                            )
                        };
                        done(
                            say,
                            json!({ "title": p.title, "artist": p.artist, "app": p.app, "playing": p.playing }),
                        )
                    }
                    _ => done(text::t("reply.nothingPlaying"), json!({ "playing": false })),
                },
            ),
        ),
        tool(
            def(
                "screen.screenshot",
                "Save a screenshot to Pictures\\Screenshots: the screen in front, or a region of the desktop in physical pixels.",
                object(
                    json!({ "region": { "type": "object", "properties": {
                        "x": {"type": "integer"}, "y": {"type": "integer"},
                        "width": {"type": "integer", "minimum": 1}, "height": {"type": "integer", "minimum": 1}
                    }, "required": ["x", "y", "width", "height"] } }),
                    &[],
                ),
                Risk::Low,
                &[LocalRead, LocalWrite],
                C::SystemControls,
                NotApplicable,
            ),
            Box::new(|args, env| {
                let target = match region_arg(args)? {
                    Some(rect) => CaptureTarget::Region { rect },
                    None => CaptureTarget::ActiveMonitor,
                };
                screenshot(env, target)
            }),
        ),
        tool(
            def(
                "screen.screenshot_window",
                "Save a screenshot of one window (the one in front by default) to Pictures\\Screenshots.",
                window_param(false),
                Risk::Low,
                &[LocalRead, LocalWrite],
                C::SystemControls,
                NotApplicable,
            ),
            Box::new(|args, env| {
                let (id, _) = window_arg(args, env)?;
                screenshot(env, CaptureTarget::Window { id })
            }),
        ),
        tool(
            def(
                "system.lock",
                "Lock Windows (sign-in needed to return).",
                none(),
                Risk::Low,
                &[LocalWrite],
                C::SystemControls,
                NotApplicable,
            ),
            Box::new(|_, env| {
                env.control
                    .power(PowerAction::Lock)
                    .map_err(platform_error)?;
                done(text::t("reply.locked"), json!({}))
            }),
        ),
        tool(
            def(
                "system.sleep",
                "Put the PC to sleep.",
                none(),
                Risk::Medium,
                &[LocalWrite],
                C::SystemControls,
                NotApplicable,
            ),
            Box::new(|_, env| {
                env.control
                    .power(PowerAction::Sleep)
                    .map_err(platform_error)?;
                done(text::t("reply.sleeping"), json!({}))
            }),
        ),
        tool(
            def(
                "system.restart",
                "Restart Windows. Unsaved work in other apps may be lost.",
                none(),
                Risk::High,
                &[Destructive],
                C::PowerActions,
                Irreversible,
            ),
            Box::new(|_, env| {
                env.control
                    .power(PowerAction::Restart)
                    .map_err(platform_error)?;
                done(text::t("reply.restarting"), json!({}))
            }),
        ),
        tool(
            def(
                "system.shutdown",
                "Shut Windows down. Unsaved work in other apps may be lost.",
                none(),
                Risk::High,
                &[Destructive],
                C::PowerActions,
                Irreversible,
            ),
            Box::new(|_, env| {
                env.control
                    .power(PowerAction::Shutdown)
                    .map_err(platform_error)?;
                done(text::t("reply.shuttingDown"), json!({}))
            }),
        ),
        tool(
            def(
                "notifications.show",
                "Show a Windows notification.",
                object(
                    json!({ "title": {"type": "string"}, "body": {"type": "string"} }),
                    &["title"],
                ),
                Risk::Safe,
                &[NoEffect],
                C::Notifications,
                NotApplicable,
            ),
            Box::new(|args, env| {
                let title = args["title"].as_str().ok_or_else(|| invalid("title"))?;
                let body = args["body"].as_str().unwrap_or_default();
                env.notifications
                    .show(&Notification {
                        title: title.into(),
                        body: body.into(),
                        actions: Vec::new(),
                        reply: false,
                    })
                    .map_err(platform_error)?;
                done(text::t("reply.done"), json!({}))
            }),
        ),
        tool(
            Def {
                egress: true,
                ..def(
                    "browser.open_url",
                    "Open a web address in the default browser.",
                    object(
                        json!({ "url": { "type": "string", "format": "uri" } }),
                        &["url"],
                    ),
                    Risk::Low,
                    &[ExternalComms],
                    C::BrowserOpenLinks,
                    NotApplicable,
                )
            },
            Box::new(|args, env| {
                let url = args["url"].as_str().ok_or_else(|| invalid("address"))?;
                env.control.open_url(url).map_err(platform_error)?;
                let host = url
                    .split("://")
                    .nth(1)
                    .unwrap_or(url)
                    .split('/')
                    .next()
                    .unwrap_or(url);
                done(
                    text::tf("reply.opening", &[("name", &host)]),
                    json!({ "url": url }),
                )
            }),
        ),
        tool(
            Def {
                egress: true,
                ..def(
                    "browser.search",
                    "Search the web in the default browser.",
                    object(json!({ "text": { "type": "string" } }), &["text"]),
                    Risk::Low,
                    &[ExternalComms],
                    C::BrowserOpenLinks,
                    NotApplicable,
                )
            },
            Box::new(|args, env| {
                let query = args["text"]
                    .as_str()
                    .filter(|t| !t.trim().is_empty())
                    .ok_or_else(|| invalid("query"))?;
                let url = format!("https://www.google.com/search?q={}", encode_query(query));
                env.control.open_url(&url).map_err(platform_error)?;
                done(
                    text::tf("reply.searching", &[("text", &query)]),
                    json!({ "url": url }),
                )
            }),
        ),
    ]
}

/// Captures `target` and saves it as a PNG.
fn screenshot(env: &Env, target: CaptureTarget) -> Result<Output, ToolError> {
    let image = env.screen.capture(target).map_err(platform_error)?;
    let path = save_png(&env.screenshots, &image)?;
    done(
        text::t("reply.screenshotSaved"),
        json!({ "path": path.to_string_lossy(), "width": image.width, "height": image.height }),
    )
}

/// `{"region": {x, y, width, height}}`, when given.
pub(crate) fn region_arg(args: &Value) -> Result<Option<kivo_platform::Rect>, ToolError> {
    let region = &args["region"];
    if region.is_null() {
        return Ok(None);
    }
    let int = |k: &str| region[k].as_i64().ok_or_else(|| invalid("region"));
    let size = |k: &str| {
        int(k).and_then(|v| {
            u32::try_from(v)
                .ok()
                .filter(|v| *v > 0)
                .ok_or_else(|| invalid("region"))
        })
    };
    Ok(Some(kivo_platform::Rect {
        x: i32::try_from(int("x")?).map_err(|_| invalid("region"))?,
        y: i32::try_from(int("y")?).map_err(|_| invalid("region"))?,
        width: size("width")?,
        height: size("height")?,
    }))
}

/// How long `apps.restart` waits for the app to close (it may ask to save first).
const RESTART_WAIT: std::time::Duration = std::time::Duration::from_secs(15);

fn restore_window(data: &Value, env: &Env) -> Result<Output, ToolError> {
    let id = data["windowId"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| invalid("window"))?;
    env.windows.restore(WindowId(id)).map_err(platform_error)?;
    done(text::t("reply.undone"), json!({}))
}

fn undo_mute(data: &Value, env: &Env) -> Result<Output, ToolError> {
    let was = data["wasMuted"].as_bool().unwrap_or(false);
    env.control.set_muted(was).map_err(platform_error)?;
    done(text::t("reply.undone"), json!({ "muted": was }))
}

fn undo_mic(data: &Value, env: &Env) -> Result<Output, ToolError> {
    let was = data["wasMuted"].as_bool().unwrap_or(false);
    env.control.set_mic_muted(was).map_err(platform_error)?;
    done(text::t("reply.undone"), json!({ "micMuted": was }))
}

fn undo_volume(data: &Value, env: &Env) -> Result<Output, ToolError> {
    let previous = data["previous"].as_f64().ok_or_else(|| invalid("number"))?;
    #[allow(clippy::cast_possible_truncation, reason = "0–100")]
    let level = (previous as f32 / 100.0).clamp(0.0, 1.0);
    env.control.set_volume(level).map_err(platform_error)?;
    done(text::t("reply.undone"), json!({ "volume": previous }))
}

fn nothing_playing(e: PlatformError) -> ToolError {
    match e {
        PlatformError::NotFound(_) => {
            ToolError::new(ToolErrorCode::NotFound, text::t("reply.nothingPlaying"))
        }
        other => platform_error(other),
    }
}

fn step_volume(env: &Env, delta: f32) -> Result<Output, ToolError> {
    let now = env.control.volume().map_err(platform_error)?;
    let level = (now.level + delta).clamp(0.0, 1.0);
    env.control.set_volume(level).map_err(platform_error)?;
    let percent = (level * 100.0).round();
    done(
        text::tf("reply.volume", &[("percent", &percent)]),
        json!({ "volume": percent, "previous": (now.level * 100.0).round() }),
    )
}

/// Percent-encodes a search query.
fn encode_query(text: &str) -> String {
    let mut out = String::new();
    for b in text.trim().bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(b))
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Writes `image` as `KIVO <date> <time>.png` in `dir`.
pub(crate) fn save_png(
    dir: &std::path::Path,
    image: &kivo_platform::Image,
) -> Result<PathBuf, ToolError> {
    let fail = |e: &dyn std::fmt::Display| {
        ToolError::new(ToolErrorCode::Failed, text::t("error.screenshot"))
            .with_detail(e.to_string())
    };
    std::fs::create_dir_all(dir).map_err(|e| fail(&e))?;
    let stamp = jiff::Zoned::now().strftime("%Y-%m-%d %H%M%S").to_string();
    let mut path = dir.join(format!("KIVO {stamp}.png"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("KIVO {stamp} ({n}).png"));
        n += 1;
    }
    let file = std::fs::File::create(&path).map_err(|e| fail(&e))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| fail(&e))?;
    writer.write_image_data(&image.rgba).map_err(|e| fail(&e))?;
    writer.finish().map_err(|e| fail(&e))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kivo_platform::{PlatformResult, Rect, WindowInfo};
    use kivo_testkit::{FakeApps, FakeNotifications, FakeSystemControl, FakeWindows, WindowAction};

    struct FakeScreen;

    impl Screen for FakeScreen {
        fn capture(&self, _target: CaptureTarget) -> PlatformResult<kivo_platform::Image> {
            Ok(kivo_platform::Image {
                width: 2,
                height: 1,
                rgba: vec![255, 0, 0, 255, 0, 255, 0, 255],
            })
        }
    }

    struct Rig {
        env: Arc<Env>,
        apps: Arc<FakeApps>,
        windows: Arc<FakeWindows>,
        control: Arc<FakeSystemControl>,
        _dir: tempfile::TempDir,
    }

    fn rig() -> Rig {
        let chrome = AppEntry {
            id: "Chrome".into(),
            name: "Google Chrome".into(),
            aliases: Vec::new(),
            exe: None,
        };
        let apps = Arc::new(FakeApps {
            installed: vec![chrome.clone()],
            ..Default::default()
        });
        let window = WindowInfo {
            id: WindowId(7),
            title: "Inbox".into(),
            app_id: "mail".into(),
            bounds: Rect {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            },
            minimized: false,
        };
        let windows = Arc::new(FakeWindows {
            windows: std::sync::Mutex::new(vec![window]),
            ..Default::default()
        });
        let control = Arc::new(FakeSystemControl::default());
        let dir = tempfile::tempdir().unwrap();
        let env = Arc::new(Env {
            apps: apps.clone(),
            windows: windows.clone(),
            control: control.clone(),
            screen: Arc::new(FakeScreen),
            notifications: Arc::new(FakeNotifications::default()),
            catalog: Arc::new(RwLock::new(vec![chrome])),
            screenshots: dir.path().to_path_buf(),
        });
        Rig {
            env,
            apps,
            windows,
            control,
            _dir: dir,
        }
    }

    fn run(rig: &Rig, id: &str, args: Value) -> Result<Output, ToolError> {
        let tools = builtin(&rig.env);
        let tool = tools
            .iter()
            .find(|t| t.spec().id == id)
            .unwrap_or_else(|| panic!("no tool {id}"));
        tool.run(&args)
    }

    /// Runs `id`, then undoes it from the data it returned.
    fn run_and_undo(rig: &Rig, id: &str, args: Value) -> Output {
        let done = run(rig, id, args).unwrap();
        let tools = builtin(&rig.env);
        let tool = tools.iter().find(|t| t.spec().id == id).unwrap();
        tool.undo(&done.data).unwrap()
    }

    #[test]
    fn undoable_tools_take_back_what_they_did() {
        let r = rig();
        // Volume 50% → 20% → undo → 50%.
        run_and_undo(&r, "audio.volume_set", json!({ "number": 20 }));
        assert!((r.control.volume.lock().unwrap().level - 0.5).abs() < 1e-6);
        run_and_undo(&r, "audio.volume_up", json!({}));
        assert!((r.control.volume.lock().unwrap().level - 0.5).abs() < 1e-6);
        // Muting from unmuted, undone, is unmuted again; unmuting an already-on sound stays on.
        let said = run_and_undo(&r, "audio.mute", json!({}));
        assert_eq!(said.say, "Undone.");
        assert!(!r.control.volume.lock().unwrap().muted);
        r.control.mic.lock().unwrap().muted = true;
        run_and_undo(&r, "audio.mic_unmute", json!({}));
        assert!(r.control.mic.lock().unwrap().muted, "back to muted");
        // A minimized window is restored.
        run_and_undo(&r, "windows.minimize", json!({}));
        let actions = r.windows.actions.lock().unwrap();
        assert_eq!(
            actions.iter().map(|(_, a)| *a).collect::<Vec<_>>(),
            [WindowAction::Minimize, WindowAction::Restore]
        );
    }

    #[test]
    fn every_undoable_tool_has_an_undo_and_no_other_does() {
        let r = rig();
        for tool in builtin(&r.env) {
            let undoable = tool.spec().reversibility == Reversibility::Undoable;
            let unsupported = matches!(
                tool.undo(&json!({})),
                Err(ToolError {
                    code: ToolErrorCode::Unsupported,
                    ..
                })
            );
            assert_eq!(undoable, !unsupported, "{}", tool.spec().id);
        }
    }

    #[test]
    fn screenshots_of_the_screen_a_window_or_a_region() {
        let r = rig();
        for (id, args) in [
            ("screen.screenshot", json!({})),
            ("screen.screenshot_window", json!({})),
            (
                "screen.screenshot",
                json!({ "region": { "x": 10, "y": 20, "width": 300, "height": 200 } }),
            ),
        ] {
            let shot = run(&r, id, args).unwrap();
            assert!(PathBuf::from(shot.data["path"].as_str().unwrap()).is_file());
        }
        let bad = run(
            &r,
            "screen.screenshot",
            json!({ "region": { "x": 0, "y": 0, "width": 0, "height": 5 } }),
        );
        assert_eq!(bad.unwrap_err().code, ToolErrorCode::InvalidArgs);
    }

    #[test]
    fn restarting_closes_then_opens_the_app() {
        let r = rig();
        let chrome = json!({"app": {"id": "Chrome", "name": "Google Chrome"}});
        run(&r, "apps.launch", chrome.clone()).unwrap();
        let out = run(&r, "apps.restart", chrome.clone()).unwrap();
        assert_eq!(out.say, "Restarting Google Chrome.");
        assert_eq!(*r.apps.closed.lock().unwrap(), ["Chrome"]);
        assert_eq!(*r.apps.launched.lock().unwrap(), ["Chrome"]);
        // Not running: it just starts.
        r.apps.launched.lock().unwrap().clear();
        run(&r, "apps.restart", chrome).unwrap();
        assert_eq!(*r.apps.launched.lock().unwrap(), ["Chrome"]);
    }

    #[test]
    fn the_m1_journeys_act_through_the_platform() {
        let r = rig();
        let out = run(
            &r,
            "apps.launch",
            json!({"app": {"id": "Chrome", "name": "Google Chrome"}}),
        )
        .unwrap();
        assert_eq!(out.say, "Opening Google Chrome.");
        assert_eq!(*r.apps.launched.lock().unwrap(), ["Chrome"]);
        run(&r, "audio.mute", json!({})).unwrap();
        assert!(r.control.volume.lock().unwrap().muted);
        let shot = run(&r, "screen.screenshot", json!({})).unwrap();
        let path = PathBuf::from(shot.data["path"].as_str().unwrap());
        assert!(path.exists() && path.extension().unwrap() == "png");
    }

    #[test]
    fn volume_is_set_stepped_and_clamped() {
        let r = rig();
        run(&r, "audio.volume_set", json!({"number": 30})).unwrap();
        assert!((r.control.volume.lock().unwrap().level - 0.3).abs() < 1e-6);
        assert_eq!(
            run(&r, "audio.volume_down", json!({})).unwrap().say,
            "Volume 20%."
        );
        run(&r, "audio.volume_set", json!({"number": 100})).unwrap();
        assert_eq!(
            run(&r, "audio.volume_up", json!({})).unwrap().say,
            "Volume 100%."
        );
    }

    #[test]
    fn window_commands_default_to_the_window_in_front() {
        let r = rig();
        run(&r, "windows.minimize", json!({})).unwrap();
        assert_eq!(
            *r.windows.actions.lock().unwrap(),
            [(WindowId(7), WindowAction::Minimize)]
        );
    }

    #[test]
    fn failures_are_worded_for_people() {
        let r = rig();
        let e = run(
            &r,
            "apps.close",
            json!({"app": {"id": "Chrome", "name": "Google Chrome"}}),
        )
        .unwrap_err();
        assert_eq!(e.message, "I couldn’t find Google Chrome.");
        let e = run(&r, "media.next", json!({})).unwrap_err();
        assert_eq!(e.message, "Nothing is playing.");
        let e = run(&r, "audio.volume_set", json!({})).unwrap_err();
        assert_eq!(e.code, ToolErrorCode::InvalidArgs);
        assert_eq!(
            run(&r, "media.now_playing", json!({})).unwrap().say,
            "Nothing is playing."
        );
    }

    #[test]
    fn power_actions_are_high_risk_and_irreversible() {
        let r = rig();
        let tools = builtin(&r.env);
        for id in ["system.restart", "system.shutdown"] {
            let s = tools.iter().find(|t| t.spec().id == id).unwrap().spec();
            assert_eq!(
                (s.risk, s.reversibility, s.capability),
                (
                    Risk::High,
                    Reversibility::Irreversible,
                    Capability::PowerActions
                )
            );
        }
        // Tests never run them for real: the fake only records.
        run(&r, "system.shutdown", json!({})).unwrap();
        assert_eq!(*r.control.power.lock().unwrap(), [PowerAction::Shutdown]);
    }

    #[test]
    fn searches_and_links_open_in_the_browser_with_the_destination_recorded() {
        let r = rig();
        run(&r, "browser.search", json!({"text": "rust & wasm"})).unwrap();
        assert_eq!(
            r.control.opened.lock().unwrap()[0],
            "https://www.google.com/search?q=rust+%26+wasm"
        );
        let t = targets("browser.open_url", &json!({"url": "https://youtube.com"}));
        assert_eq!(
            t,
            [Target::Destination {
                address: "https://youtube.com".into(),
                provenance: Provenance::User
            }]
        );
        assert_eq!(
            targets(
                "apps.launch",
                &json!({"app": {"id": "c", "name": "Chrome"}})
            )
            .len(),
            1
        );
    }

    #[test]
    fn every_tool_is_fully_declared() {
        let r = rig();
        for t in builtin(&r.env) {
            let s = t.spec();
            assert!(
                s.id.contains('.') && !s.title.is_empty() && !s.description.is_empty(),
                "{}",
                s.id
            );
            assert!(
                !s.title.starts_with("tool."),
                "{} has a catalog title",
                s.id
            );
            assert_eq!(s.params["type"], "object");
            assert!(s.timeout_ms > 0 && !s.side_effects.is_empty());
        }
        for what in ["app", "window", "number", "title", "address", "query"] {
            assert!(
                !invalid(what).message.starts_with("error."),
                "{what} is worded"
            );
        }
    }
}
