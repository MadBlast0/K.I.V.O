//! The M4 computer-control tools (TOOLS_AND_CONTROL §2–8): UI Automation, files, the clipboard,
//! the shell, screen reading and OCR, input, windows on monitors, brightness and battery, the
//! browser, and the capability-ladder router. They act through `Controls`, the platform traits
//! the runtime hands in (real ones on Windows, fakes in tests).

use crate::appreg::AppRegistry;
use crate::browser::{Browser, ManagedBrowser};
use crate::builtin::{Def, platform_error};
use crate::registry::{Output, Tool};
use kivo_core::capability::CapabilitySettings;
use kivo_core::config::Tools as ToolsConfig;
use kivo_core::text;
use kivo_core::tool::{Initiator, Risk, Target, ToolError, ToolErrorCode, ToolSpec};
use kivo_platform::{
    Clipboard, CommandRunner, Displays, FileOps, Input, Ocr, Power, Screen, Secrets, UiAutomation,
    WindowId, WindowInfo, Windows,
};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tokio_util::sync::CancellationToken;

/// Opens something by name or URI (a settings page, an app URI).
pub type Opener = Arc<dyn Fn(&str) -> Result<(), ToolError> + Send + Sync>;

/// A remembered workspace's folder by its name ("open Claude in K.I.V.O").
pub type FolderLookup = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;

/// What the control tools act through.
pub struct Controls {
    pub uia: Arc<dyn UiAutomation>,
    pub input: Arc<dyn Input>,
    pub ocr: Arc<dyn Ocr>,
    pub screen: Arc<dyn Screen>,
    pub clipboard: Arc<dyn Clipboard>,
    pub files: Arc<dyn FileOps>,
    pub commands: Arc<dyn CommandRunner>,
    pub displays: Arc<dyn Displays>,
    pub power: Arc<dyn Power>,
    pub windows: Arc<dyn Windows>,
    pub browser: Arc<dyn Browser>,
    /// KIVO's own browser profile over CDP, when available (TOOL-25).
    pub managed: Option<Arc<dyn ManagedBrowser>>,
    pub apps: Arc<AppRegistry>,
    /// Where commands run when no folder is given (the Brains workspace).
    pub shell_home: PathBuf,
    /// The OCR language (the UI language, e.g. "en-US").
    pub language: String,
    /// KIVO's own voice output device, switched by `audio.set_output` (the runtime applies it).
    pub voice_output: Arc<dyn Fn(Option<String>) + Send + Sync>,
    /// The output devices, for `audio.outputs` (id, name, default).
    pub output_devices: Arc<dyn Fn() -> Vec<(String, String, bool)> + Send + Sync>,
    /// Opens a page of the system settings (for the parts no documented API covers).
    pub open_settings: Opener,
    /// Opens an app URI (`spotify:…`); only called for schemes the registry declares.
    pub open_uri: Opener,
    /// Windows seen by the tools, for their app (per-app allow and block lists, CAP-07).
    pub window_cache: RwLock<Vec<WindowInfo>>,
    /// Stored secrets, read only when a command needs one as an environment variable.
    pub secrets: Arc<dyn Secrets>,
    /// The per-capability options as they are now (CAPABILITIES §1).
    pub settings: Arc<dyn Fn() -> ToolsConfig + Send + Sync>,
    /// The capability toggles as they are now (the router skips tiers that are off).
    pub caps: Arc<dyn Fn() -> CapabilitySettings + Send + Sync>,
    /// Whether a screenshot may go to the brain of the current turn (a vision model, and the
    /// privacy mode and the cloud-vision option allow it; CAP-08).
    pub vision: Arc<dyn Fn() -> bool + Send + Sync>,
    /// Visible terminals for CLI agents (CONV-14).
    pub terminals: Arc<dyn kivo_platform::Terminals>,
    /// The terminal agent sessions KIVO started.
    pub terminal_sessions: Arc<crate::agents_tools::TerminalSessions>,
    /// A remembered workspace's folder by its name ("K.I.V.O", CONV-10).
    pub workspace_folder: FolderLookup,
}

impl Controls {
    pub(crate) fn cloud_vision_allowed(&self) -> bool {
        (self.vision)()
    }

    pub(crate) fn options(&self) -> ToolsConfig {
        (self.settings)()
    }
}

pub(crate) type Run =
    dyn Fn(&Value, &Controls, &CancellationToken) -> Result<Output, ToolError> + Send + Sync;
pub(crate) type Undo = dyn Fn(&Value, &Controls) -> Result<Output, ToolError> + Send + Sync;
pub(crate) type Assess = dyn Fn(&Value, Initiator, &Controls) -> Risk + Send + Sync;
pub(crate) type Targets = dyn Fn(&Value, &Controls) -> Vec<Target> + Send + Sync;
pub(crate) type HardLimit = dyn Fn(&Value, &Controls) -> Result<(), ToolError> + Send + Sync;

pub(crate) struct Control {
    pub spec: ToolSpec,
    pub c: Arc<Controls>,
    pub run: Box<Run>,
    pub undo: Option<Box<Undo>>,
    pub assess: Option<Box<Assess>>,
    pub targets: Option<Box<Targets>>,
    pub hard_limit: Option<Box<HardLimit>>,
}

impl Tool for Control {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }
    fn run(&self, args: &Value) -> Result<Output, ToolError> {
        (self.run)(args, &self.c, &CancellationToken::new())
    }
    fn run_cancellable(
        &self,
        args: &Value,
        cancel: &CancellationToken,
    ) -> Result<Output, ToolError> {
        (self.run)(args, &self.c, cancel)
    }
    fn undo(&self, data: &Value) -> Result<Output, ToolError> {
        match &self.undo {
            Some(undo) => undo(data, &self.c),
            None => Err(ToolError::new(
                ToolErrorCode::Unsupported,
                text::t("error.cantUndo"),
            )),
        }
    }
    fn assess(&self, args: &Value, initiator: Initiator) -> Risk {
        match &self.assess {
            Some(f) => f(args, initiator, &self.c),
            None => self.spec.risk,
        }
    }
    fn targets(&self, args: &Value) -> Vec<Target> {
        match &self.targets {
            Some(f) => f(args, &self.c),
            None => Vec::new(),
        }
    }
    fn hard_limit(&self, args: &Value) -> Result<(), ToolError> {
        match &self.hard_limit {
            Some(f) => f(args, &self.c),
            None => Ok(()),
        }
    }
}

/// Builds one control tool.
pub(crate) struct Builder {
    c: Arc<Controls>,
}

impl Builder {
    pub(crate) fn new(c: &Arc<Controls>) -> Self {
        Self { c: Arc::clone(c) }
    }

    pub(crate) fn tool(&self, d: &Def, run: Box<Run>) -> ControlBuilder {
        ControlBuilder(Control {
            spec: crate::builtin::spec(d),
            c: Arc::clone(&self.c),
            run,
            undo: None,
            assess: None,
            targets: None,
            hard_limit: None,
        })
    }
}

pub(crate) struct ControlBuilder(Control);

impl ControlBuilder {
    pub(crate) fn undo(mut self, undo: Box<Undo>) -> Self {
        self.0.undo = Some(undo);
        self
    }
    pub(crate) fn assess(mut self, assess: Box<Assess>) -> Self {
        self.0.assess = Some(assess);
        self
    }
    pub(crate) fn targets(mut self, targets: Box<Targets>) -> Self {
        self.0.targets = Some(targets);
        self
    }
    pub(crate) fn hard_limit(mut self, check: Box<HardLimit>) -> Self {
        self.0.hard_limit = Some(check);
        self
    }
    pub(crate) fn build(self) -> Arc<dyn Tool> {
        debug_assert_eq!(
            self.0.undo.is_some(),
            self.0.spec.reversibility == kivo_core::tool::Reversibility::Undoable,
            "{}: undo handler and reversibility disagree",
            self.0.spec.id
        );
        Arc::new(self.0)
    }
}

/// Every control tool.
pub fn controls(c: &Arc<Controls>) -> Vec<Arc<dyn Tool>> {
    let mut out = Vec::new();
    out.extend(crate::uia_tools::tools(c));
    out.extend(crate::files::tools(c));
    out.extend(crate::shell::tools(c));
    out.extend(crate::screen_tools::tools(c));
    out.extend(crate::input_tools::tools(c));
    out.extend(crate::system_tools::tools(c));
    out.extend(crate::browser::tools(c));
    out.extend(crate::router::tools(c));
    out.extend(crate::agents_tools::tools(c));
    out
}

/// A required string argument.
pub(crate) fn str_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, ToolError> {
    args[key]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| missing(key))
}

pub(crate) fn missing(key: &str) -> ToolError {
    ToolError::new(
        ToolErrorCode::InvalidArgs,
        text::tf("error.missingArg", &[("name", &key)]),
    )
}

/// `{"window": {"id"}}` or `{"window": "<id>"}`; the foreground window when absent.
pub(crate) fn window_of(args: &Value, c: &Controls) -> Result<WindowInfo, ToolError> {
    let id = args["window"]["id"]
        .as_str()
        .or_else(|| args["window"].as_str())
        .and_then(|s| s.parse::<u64>().ok());
    let list = c.windows.list().map_err(platform_error)?;
    let found = match id {
        Some(id) => list.into_iter().find(|w| w.id == WindowId(id)),
        None => c.windows.foreground().map_err(platform_error)?,
    };
    let window = found.ok_or_else(|| {
        ToolError::new(
            ToolErrorCode::NotFound,
            text::tf("error.notFound", &[("what", &text::t("error.what.window"))]),
        )
    })?;
    remember(c, &window);
    Ok(window)
}

/// The window an element lives in (from its reference), when known.
pub(crate) fn window_by_id(c: &Controls, id: WindowId) -> Option<WindowInfo> {
    let cached = c
        .window_cache
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .find(|w| w.id == id)
        .cloned();
    cached.or_else(|| {
        let found = c.windows.list().ok()?.into_iter().find(|w| w.id == id)?;
        remember(c, &found);
        Some(found)
    })
}

fn remember(c: &Controls, w: &WindowInfo) {
    let mut cache = c
        .window_cache
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cache.retain(|x| x.id != w.id);
    cache.push(w.clone());
    if cache.len() > 64 {
        cache.remove(0);
    }
}

/// The app behind a window, as a target for the per-app lists: its program as the id and the
/// program's name (`KeePassXC`) as the name.
pub(crate) fn app_target(w: &WindowInfo) -> Target {
    let stem = std::path::Path::new(&w.app_id)
        .file_stem()
        .map_or_else(|| w.app_id.clone(), |s| s.to_string_lossy().into_owned());
    Target::App {
        id: w.app_id.clone(),
        name: stem,
    }
}

/// Targets for tools that act on a window (by `window`) or an element (by `element`).
pub(crate) fn window_targets(args: &Value, c: &Controls) -> Vec<Target> {
    let from_element = args["element"]
        .as_str()
        .and_then(|e| kivo_platform::ElementRef(e.to_owned()).window());
    let window = match from_element {
        Some(id) => window_by_id(c, id),
        None => window_of(args, c).ok(),
    };
    window
        .map(|w| {
            vec![
                app_target(&w),
                Target::Window {
                    title: w.title.clone(),
                    app_id: w.app_id,
                },
            ]
        })
        .unwrap_or_default()
}

/// A short label for untrusted content from a window ("Notepad — notes.txt").
pub(crate) fn window_source(w: &WindowInfo) -> String {
    let app = std::path::Path::new(&w.app_id)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if app.is_empty() {
        w.title.clone()
    } else {
        format!("{app} — {}", w.title)
    }
}
