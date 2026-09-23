//! Fakes of the M4 control traits: a UI Automation tree shaped like the dummy app, input that only
//! records, OCR that reads back what it was given, an in-memory clipboard, a Recycle Bin that is a
//! folder, a command runner that records, monitors, power and Windows Hello. Nothing here touches
//! the real desktop.

use kivo_platform::{
    Battery, Clipboard, CommandOutput, CommandRunner, CommandSpec, Displays, ElementQuery,
    ElementRef, FileHit, FileOps, Image, Input, Monitor, MouseButton, Ocr, PlatformError,
    PlatformResult, Point, Power, Rect, TextLine, UiAction, UiAutomation, UiEvent, UiEventSink,
    UiNode, UiSubscription, UserVerifier, WindowId,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, RwLock};

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The fake dummy app's window id and program.
pub const APP_WINDOW: WindowId = WindowId(7);
pub const APP_EXE: &str = r"C:\testenv\kivo-test-app.exe";

struct Control {
    id: &'static str,
    role: &'static str,
    name: &'static str,
    password: bool,
    actions: &'static [UiAction],
}

const CONTROLS: &[Control] = &[
    Control {
        id: "101",
        role: "Edit",
        name: "Name:",
        password: false,
        actions: &[UiAction::SetValue],
    },
    Control {
        id: "102",
        role: "Edit",
        name: "Password:",
        password: true,
        actions: &[UiAction::SetValue],
    },
    Control {
        id: "103",
        role: "Button",
        name: "Greet",
        password: false,
        actions: &[UiAction::Invoke],
    },
    Control {
        id: "104",
        role: "CheckBox",
        name: "Remember me",
        password: false,
        actions: &[UiAction::Toggle],
    },
    Control {
        id: "105",
        role: "List",
        name: "",
        password: false,
        actions: &[],
    },
    Control {
        id: "106",
        role: "ComboBox",
        name: "Colour:",
        password: false,
        actions: &[UiAction::Expand, UiAction::SetValue],
    },
    Control {
        id: "107",
        role: "Button",
        name: "Export…",
        password: false,
        actions: &[UiAction::Invoke],
    },
    Control {
        id: "108",
        role: "Text",
        name: "Ready",
        password: false,
        actions: &[],
    },
    Control {
        id: "110",
        role: "Edit",
        name: "Address and search bar",
        password: false,
        actions: &[UiAction::SetValue],
    },
];

/// UI Automation over a fixed tree shaped like `testenv/app`. Records every action.
#[derive(Default)]
pub struct FakeUia {
    pub invoked: Mutex<Vec<String>>,
    pub values: Mutex<HashMap<String, String>>,
    pub toggles: Mutex<HashMap<String, bool>>,
    pub selected: Mutex<Vec<String>>,
    pub expanded: Mutex<HashMap<String, bool>>,
    /// The window shows nothing (a custom-drawn app).
    pub empty: AtomicBool,
    /// The focused element's AutomationId.
    pub focus: Mutex<Option<String>>,
    /// The text selected in the focused element.
    pub selection: Mutex<Option<String>>,
    sinks: Mutex<HashMap<u64, (UiSubscription, UiEventSink)>>,
    next: AtomicU64,
}

impl FakeUia {
    /// The reference of the control with this AutomationId.
    pub fn element(&self, id: &str) -> String {
        format!("{}:{id}", APP_WINDOW.0)
    }

    pub fn value_of(&self, id: &str) -> String {
        lock(&self.values)
            .get(&self.element(id))
            .cloned()
            .unwrap_or_default()
    }

    pub fn toggled(&self, id: &str) -> bool {
        lock(&self.toggles)
            .get(&self.element(id))
            .copied()
            .unwrap_or(false)
    }

    /// Delivers an event to every matching subscriber.
    pub fn fire(&self, event: UiEvent) {
        for (sub, sink) in lock(&self.sinks).values() {
            let kind = match &event {
                UiEvent::FocusChanged { .. } => kivo_platform::UiEventKind::FocusChanged,
                UiEvent::StructureChanged { .. } => kivo_platform::UiEventKind::StructureChanged,
                UiEvent::WindowOpened { .. } => kivo_platform::UiEventKind::WindowOpened,
                UiEvent::WindowClosed { .. } => kivo_platform::UiEventKind::WindowClosed,
                UiEvent::PropertyChanged { .. } => kivo_platform::UiEventKind::PropertyChanged,
            };
            if sub.kind == kind {
                sink(event.clone());
            }
        }
    }

    pub fn subscriptions(&self) -> usize {
        lock(&self.sinks).len()
    }

    fn node(&self, c: &Control) -> UiNode {
        let element = ElementRef(self.element(c.id));
        let index = CONTROLS.iter().position(|x| x.id == c.id).unwrap_or(0);
        let has = |a| c.actions.contains(&a);
        UiNode {
            role: c.role.into(),
            name: c.name.into(),
            automation_id: c.id.into(),
            value: (has(UiAction::SetValue) && !c.password).then(|| {
                lock(&self.values)
                    .get(&element.0)
                    .cloned()
                    .unwrap_or_default()
            }),
            toggled: has(UiAction::Toggle).then(|| {
                lock(&self.toggles)
                    .get(&element.0)
                    .copied()
                    .unwrap_or(false)
            }),
            selected: None,
            expanded: has(UiAction::Expand).then(|| {
                lock(&self.expanded)
                    .get(&element.0)
                    .copied()
                    .unwrap_or(false)
            }),
            enabled: true,
            bounds: Some(Rect {
                x: 200,
                y: 150 + i32::try_from(index).unwrap_or(0) * 30,
                width: 120,
                height: 24,
            }),
            is_password: c.password,
            actions: c.actions.to_vec(),
            children: Vec::new(),
            element,
        }
    }

    fn get(&self, element: &ElementRef) -> PlatformResult<&'static Control> {
        let id = element
            .0
            .strip_prefix(&format!("{}:", APP_WINDOW.0))
            .ok_or_else(|| PlatformError::NotFound("that control".into()))?;
        CONTROLS
            .iter()
            .find(|c| c.id == id)
            .ok_or_else(|| PlatformError::NotFound("that control".into()))
    }

    fn check_window(window: WindowId) -> PlatformResult<()> {
        if window == APP_WINDOW {
            Ok(())
        } else {
            Err(PlatformError::NotFound("that window".into()))
        }
    }
}

impl UiAutomation for FakeUia {
    fn find(&self, query: &ElementQuery, limit: usize) -> PlatformResult<Vec<UiNode>> {
        Self::check_window(query.window.unwrap_or(APP_WINDOW))?;
        if self.empty.load(Ordering::SeqCst) {
            return Ok(Vec::new());
        }
        let norm = |s: &str| s.to_lowercase().replace(['…', ':', '&'], "");
        Ok(CONTROLS
            .iter()
            .filter(|c| query.automation_id.as_deref().is_none_or(|id| id == c.id))
            .filter(|c| {
                query
                    .role
                    .as_deref()
                    .is_none_or(|r| r.eq_ignore_ascii_case(c.role))
            })
            .filter(|c| {
                query
                    .name
                    .as_deref()
                    .is_none_or(|n| norm(c.name).contains(&norm(n)))
            })
            .take(limit)
            .map(|c| self.node(c))
            .collect())
    }

    fn tree(&self, window: WindowId, _max_depth: u8, max_nodes: usize) -> PlatformResult<UiNode> {
        Self::check_window(window)?;
        let children = if self.empty.load(Ordering::SeqCst) {
            Vec::new()
        } else {
            CONTROLS
                .iter()
                .take(max_nodes.saturating_sub(1))
                .map(|c| self.node(c))
                .collect()
        };
        Ok(UiNode {
            element: ElementRef(format!("{}:root", APP_WINDOW.0)),
            role: "Window".into(),
            name: "KIVO Test App".into(),
            enabled: true,
            bounds: Some(Rect {
                x: 120,
                y: 120,
                width: 520,
                height: 420,
            }),
            children,
            ..UiNode::default()
        })
    }

    fn focused(&self) -> PlatformResult<Option<UiNode>> {
        let focus = lock(&self.focus).clone();
        Ok(focus.and_then(|id| CONTROLS.iter().find(|c| c.id == id).map(|c| self.node(c))))
    }

    fn describe(&self, element: &ElementRef) -> PlatformResult<UiNode> {
        Ok(self.node(self.get(element)?))
    }

    fn selected_text(&self) -> PlatformResult<Option<String>> {
        Ok(lock(&self.selection).clone())
    }

    fn invoke(&self, element: &ElementRef) -> PlatformResult<()> {
        let c = self.get(element)?;
        if !c.actions.contains(&UiAction::Invoke) {
            return Err(PlatformError::Unsupported);
        }
        lock(&self.invoked).push(element.0.clone());
        Ok(())
    }

    fn set_value(&self, element: &ElementRef, value: &str) -> PlatformResult<()> {
        let c = self.get(element)?;
        if c.password {
            return Err(PlatformError::AccessDenied);
        }
        if !c.actions.contains(&UiAction::SetValue) {
            return Err(PlatformError::Unsupported);
        }
        lock(&self.values).insert(element.0.clone(), value.into());
        Ok(())
    }

    fn toggle(&self, element: &ElementRef) -> PlatformResult<bool> {
        let c = self.get(element)?;
        if !c.actions.contains(&UiAction::Toggle) {
            return Err(PlatformError::Unsupported);
        }
        let mut t = lock(&self.toggles);
        let now = !t.get(&element.0).copied().unwrap_or(false);
        t.insert(element.0.clone(), now);
        Ok(now)
    }

    fn select(&self, element: &ElementRef) -> PlatformResult<()> {
        self.get(element)?;
        lock(&self.selected).push(element.0.clone());
        Ok(())
    }

    fn expand(&self, element: &ElementRef, expand: bool) -> PlatformResult<()> {
        let c = self.get(element)?;
        if !c.actions.contains(&UiAction::Expand) {
            return Err(PlatformError::Unsupported);
        }
        lock(&self.expanded).insert(element.0.clone(), expand);
        Ok(())
    }

    fn scroll_into_view(&self, element: &ElementRef) -> PlatformResult<()> {
        self.get(element).map(|_| ())
    }

    fn bounds(&self, element: &ElementRef) -> PlatformResult<Rect> {
        self.node(self.get(element)?)
            .bounds
            .ok_or_else(|| PlatformError::NotFound("bounds".into()))
    }

    fn subscribe(&self, subscription: &UiSubscription, sink: UiEventSink) -> PlatformResult<u64> {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        lock(&self.sinks).insert(id, (subscription.clone(), sink));
        Ok(id)
    }

    fn unsubscribe(&self, id: u64) -> PlatformResult<()> {
        lock(&self.sinks).remove(&id);
        Ok(())
    }
}

/// What the fake input device was asked to do. Nothing reaches the desktop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputAction {
    Click(Point, MouseButton, u8),
    Type(String),
    Press(Vec<String>),
    Scroll(Point, i32),
}

#[derive(Default)]
pub struct FakeInput {
    pub actions: Mutex<Vec<InputAction>>,
}

impl Input for FakeInput {
    fn click(&self, at: Point, button: MouseButton, count: u8) -> PlatformResult<()> {
        lock(&self.actions).push(InputAction::Click(at, button, count));
        Ok(())
    }
    fn type_text(&self, text: &str) -> PlatformResult<()> {
        lock(&self.actions).push(InputAction::Type(text.into()));
        Ok(())
    }
    fn press(&self, keys: &[String]) -> PlatformResult<()> {
        lock(&self.actions).push(InputAction::Press(keys.to_vec()));
        Ok(())
    }
    fn scroll(&self, at: Point, delta_y: i32) -> PlatformResult<()> {
        lock(&self.actions).push(InputAction::Scroll(at, delta_y));
        Ok(())
    }
}

/// OCR that "reads" the lines it was told the next image holds.
#[derive(Default)]
pub struct FakeOcr {
    pub lines: Mutex<Vec<TextLine>>,
    pub calls: AtomicU64,
}

impl Ocr for FakeOcr {
    fn recognize(&self, _image: &Image, _language: &str) -> PlatformResult<Vec<TextLine>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(lock(&self.lines).clone())
    }
}

#[derive(Default)]
pub struct FakeClipboard {
    pub text: Mutex<Option<String>>,
}

impl Clipboard for FakeClipboard {
    fn read_text(&self) -> PlatformResult<Option<String>> {
        Ok(lock(&self.text).clone())
    }
    fn write_text(&self, text: &str) -> PlatformResult<()> {
        *lock(&self.text) = Some(text.into());
        Ok(())
    }
}

/// File operations on the real file system inside test folders; the Recycle Bin is a folder, and
/// "open" and "reveal" only record.
pub struct FakeFiles {
    pub bin: PathBuf,
    pub protected: Vec<PathBuf>,
    pub folders: Vec<PathBuf>,
    pub opened: Mutex<Vec<PathBuf>>,
    pub revealed: Mutex<Vec<PathBuf>>,
    /// Without an index, search falls back to walking the folders.
    pub indexed: bool,
}

impl FakeFiles {
    pub fn new(bin: &Path) -> Self {
        Self {
            bin: bin.to_path_buf(),
            protected: vec![
                PathBuf::from(r"C:\Windows"),
                PathBuf::from(r"C:\Program Files"),
            ],
            folders: Vec::new(),
            opened: Mutex::new(Vec::new()),
            revealed: Mutex::new(Vec::new()),
            indexed: false,
        }
    }
}

impl FileOps for FakeFiles {
    fn search_index(
        &self,
        _query: &str,
        _roots: &[PathBuf],
        _limit: usize,
    ) -> PlatformResult<Vec<FileHit>> {
        Err(PlatformError::Unsupported)
    }
    fn recycle(&self, paths: &[PathBuf]) -> PlatformResult<()> {
        std::fs::create_dir_all(&self.bin).map_err(|e| PlatformError::Os {
            code: 0,
            message: e.to_string(),
        })?;
        for p in paths {
            if !p.exists() {
                return Err(PlatformError::NotFound(p.display().to_string()));
            }
            let name = p.file_name().map(|n| n.to_owned()).unwrap_or_default();
            std::fs::rename(p, self.bin.join(name)).map_err(|e| PlatformError::Os {
                code: 0,
                message: e.to_string(),
            })?;
        }
        Ok(())
    }
    fn open(&self, path: &Path) -> PlatformResult<()> {
        lock(&self.opened).push(path.to_path_buf());
        Ok(())
    }
    fn reveal(&self, path: &Path) -> PlatformResult<()> {
        lock(&self.revealed).push(path.to_path_buf());
        Ok(())
    }
    fn protected_roots(&self) -> Vec<PathBuf> {
        self.protected.clone()
    }
    fn user_folders(&self) -> Vec<PathBuf> {
        self.folders.clone()
    }
}

/// Records commands and answers with a scripted output; runs nothing.
#[derive(Default)]
pub struct FakeCommands {
    pub ran: Mutex<Vec<CommandSpec>>,
    pub reply: Mutex<CommandOutput>,
    pub killed: AtomicU64,
    /// Waits until cancelled (a long-running command).
    pub hang: AtomicBool,
}

impl CommandRunner for FakeCommands {
    fn run(
        &self,
        spec: &CommandSpec,
        cancelled: &(dyn Fn() -> bool + Sync),
    ) -> PlatformResult<CommandOutput> {
        lock(&self.ran).push(spec.clone());
        if self.hang.load(Ordering::SeqCst) {
            let started = std::time::Instant::now();
            while !cancelled() && started.elapsed() < spec.timeout {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            return Ok(CommandOutput {
                cancelled: cancelled(),
                timed_out: !cancelled(),
                ..CommandOutput::default()
            });
        }
        Ok(lock(&self.reply).clone())
    }
    fn kill_all(&self) -> usize {
        self.killed.fetch_add(1, Ordering::SeqCst);
        0
    }
}

/// Two side-by-side monitors; placements are recorded.
pub struct FakeDisplays {
    pub monitors: Vec<Monitor>,
    pub bounds: RwLock<HashMap<u64, Rect>>,
}

impl Default for FakeDisplays {
    fn default() -> Self {
        let rect = |x, w, h| Rect {
            x,
            y: 0,
            width: w,
            height: h,
        };
        Self {
            monitors: vec![
                Monitor {
                    index: 1,
                    name: "Primary".into(),
                    bounds: rect(0, 1920, 1080),
                    work_area: rect(0, 1920, 1040),
                    primary: true,
                },
                Monitor {
                    index: 2,
                    name: "Second".into(),
                    bounds: rect(1920, 2560, 1440),
                    work_area: rect(1920, 2560, 1400),
                    primary: false,
                },
            ],
            bounds: RwLock::new(HashMap::new()),
        }
    }
}

impl Displays for FakeDisplays {
    fn monitors(&self) -> PlatformResult<Vec<Monitor>> {
        Ok(self.monitors.clone())
    }
    fn window_bounds(&self, id: WindowId) -> PlatformResult<Rect> {
        Ok(self
            .bounds
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&id.0)
            .copied()
            .unwrap_or(Rect {
                x: 100,
                y: 100,
                width: 800,
                height: 600,
            }))
    }
    fn place_window(&self, id: WindowId, bounds: Rect) -> PlatformResult<()> {
        self.bounds
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(id.0, bounds);
        Ok(())
    }
}

pub struct FakePower {
    pub brightness: Mutex<Option<u8>>,
    pub battery: Option<Battery>,
    pub focus: AtomicBool,
}

impl Default for FakePower {
    fn default() -> Self {
        Self {
            brightness: Mutex::new(Some(60)),
            battery: Some(Battery {
                percent: 81,
                charging: false,
                on_battery: true,
                seconds_left: Some(9000),
            }),
            focus: AtomicBool::new(false),
        }
    }
}

impl Power for FakePower {
    fn brightness(&self) -> PlatformResult<u8> {
        lock(&self.brightness).ok_or(PlatformError::Unsupported)
    }
    fn set_brightness(&self, percent: u8) -> PlatformResult<()> {
        let mut b = lock(&self.brightness);
        if b.is_none() {
            return Err(PlatformError::Unsupported);
        }
        *b = Some(percent);
        Ok(())
    }
    fn battery(&self) -> PlatformResult<Option<Battery>> {
        Ok(self.battery)
    }
    fn focus_on(&self) -> PlatformResult<bool> {
        Ok(self.focus.load(Ordering::SeqCst))
    }
}

/// Windows Hello that answers as scripted and counts prompts.
pub struct FakeVerifier {
    pub available: bool,
    pub answer: AtomicBool,
    pub prompts: Mutex<Vec<String>>,
}

impl Default for FakeVerifier {
    fn default() -> Self {
        Self {
            available: true,
            answer: AtomicBool::new(true),
            prompts: Mutex::new(Vec::new()),
        }
    }
}

impl UserVerifier for FakeVerifier {
    fn available(&self) -> bool {
        self.available
    }
    fn verify(&self, message: &str) -> PlatformResult<bool> {
        if !self.available {
            return Err(PlatformError::Unsupported);
        }
        lock(&self.prompts).push(message.into());
        Ok(self.answer.load(Ordering::SeqCst))
    }
}
