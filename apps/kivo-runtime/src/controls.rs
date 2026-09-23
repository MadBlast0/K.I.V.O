//! Assembling the computer-control tools (M4): the platform parts they act through, the live
//! settings they read, and the per-turn vision flag. `main.rs` builds them from the Windows
//! implementations; the scripted rig from the testkit fakes.

use crate::core::Core;
use kivo_core::tool::{ToolError, ToolErrorCode};
use kivo_platform::{
    Clipboard, CommandRunner, Displays, FileOps, Input, Ocr, Power, Screen, Secrets, UiAutomation,
    Windows,
};
use kivo_tools::{AppRegistry, Browser, Controls, ManagedBrowser};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock};

/// Opens something by name or URI with the shell.
pub type Opener = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// The platform parts the control tools need.
pub struct Platform {
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
    pub secrets: Arc<dyn Secrets>,
    pub browser: Arc<dyn Browser>,
    pub managed: Option<Arc<dyn ManagedBrowser>>,
    /// Opens a Windows settings page (`sound`).
    pub open_settings: Opener,
    /// Opens an app URI with the shell (only called for registry-declared schemes).
    pub open_uri: Opener,
    /// The audio outputs: (id, name, default).
    pub output_devices: Arc<dyn Fn() -> Vec<(String, String, bool)> + Send + Sync>,
    /// Switches KIVO's own voice output.
    pub voice_output: Arc<dyn Fn(Option<String>) + Send + Sync>,
}

fn failed(e: String) -> ToolError {
    ToolError::new(ToolErrorCode::Failed, kivo_core::text::t("error.failed")).with_detail(e)
}

/// The app registry: the core entries plus the user's own (`%APPDATA%\KIVO\apps`).
pub fn app_registry(user_dir: Option<PathBuf>) -> AppRegistry {
    let core = AppRegistry::core();
    let Some(dir) = user_dir else {
        return core;
    };
    let (registry, errors) = core.with_dir(&dir);
    for e in errors {
        tracing::warn!(%e, "skipped an app registry file");
    }
    registry
}

/// Builds `Controls` over `platform`, reading the live settings from `core`.
pub fn build(
    platform: Platform,
    core: &Arc<Core>,
    apps: AppRegistry,
    vision: Arc<AtomicBool>,
    shell_home: PathBuf,
) -> Arc<Controls> {
    let settings_core = Arc::clone(core);
    let caps_core = Arc::clone(core);
    let language = core.config().general.language;
    let open_settings = Arc::clone(&platform.open_settings);
    let open_uri = Arc::clone(&platform.open_uri);
    let uia: Arc<dyn UiAutomation> = Arc::new(BusUia {
        inner: platform.uia,
        bus: core.bus.clone(),
    });
    Arc::new(Controls {
        uia,
        input: platform.input,
        ocr: platform.ocr,
        screen: platform.screen,
        clipboard: platform.clipboard,
        files: platform.files,
        commands: platform.commands,
        displays: platform.displays,
        power: platform.power,
        windows: platform.windows,
        browser: platform.browser,
        managed: platform.managed,
        apps: Arc::new(apps),
        shell_home,
        language: if language.contains('-') {
            language
        } else {
            format!("{language}-US")
        },
        voice_output: platform.voice_output,
        output_devices: platform.output_devices,
        open_settings: Arc::new(move |page| open_settings(page).map_err(failed)),
        open_uri: Arc::new(move |uri| open_uri(uri).map_err(failed)),
        window_cache: RwLock::new(Vec::new()),
        secrets: platform.secrets,
        settings: Arc::new(move || settings_core.config().tools),
        caps: Arc::new(move || caps_core.config().capabilities),
        vision: Arc::new(move || vision.load(std::sync::atomic::Ordering::SeqCst)),
    })
}

/// UI Automation that isn't available on this PC: every call says so.
pub struct NoUia;

impl UiAutomation for NoUia {
    fn find(
        &self,
        _: &kivo_platform::ElementQuery,
        _: usize,
    ) -> kivo_platform::PlatformResult<Vec<kivo_platform::UiNode>> {
        Err(kivo_platform::PlatformError::Unsupported)
    }
    fn tree(
        &self,
        _: kivo_platform::WindowId,
        _: u8,
        _: usize,
    ) -> kivo_platform::PlatformResult<kivo_platform::UiNode> {
        Err(kivo_platform::PlatformError::Unsupported)
    }
    fn focused(&self) -> kivo_platform::PlatformResult<Option<kivo_platform::UiNode>> {
        Ok(None)
    }
    fn selected_text(&self) -> kivo_platform::PlatformResult<Option<String>> {
        Ok(None)
    }
    fn describe(
        &self,
        _: &kivo_platform::ElementRef,
    ) -> kivo_platform::PlatformResult<kivo_platform::UiNode> {
        Err(kivo_platform::PlatformError::Unsupported)
    }
    fn invoke(&self, _: &kivo_platform::ElementRef) -> kivo_platform::PlatformResult<()> {
        Err(kivo_platform::PlatformError::Unsupported)
    }
    fn set_value(
        &self,
        _: &kivo_platform::ElementRef,
        _: &str,
    ) -> kivo_platform::PlatformResult<()> {
        Err(kivo_platform::PlatformError::Unsupported)
    }
    fn toggle(&self, _: &kivo_platform::ElementRef) -> kivo_platform::PlatformResult<bool> {
        Err(kivo_platform::PlatformError::Unsupported)
    }
    fn select(&self, _: &kivo_platform::ElementRef) -> kivo_platform::PlatformResult<()> {
        Err(kivo_platform::PlatformError::Unsupported)
    }
    fn expand(&self, _: &kivo_platform::ElementRef, _: bool) -> kivo_platform::PlatformResult<()> {
        Err(kivo_platform::PlatformError::Unsupported)
    }
    fn scroll_into_view(&self, _: &kivo_platform::ElementRef) -> kivo_platform::PlatformResult<()> {
        Err(kivo_platform::PlatformError::Unsupported)
    }
    fn bounds(
        &self,
        _: &kivo_platform::ElementRef,
    ) -> kivo_platform::PlatformResult<kivo_platform::Rect> {
        Err(kivo_platform::PlatformError::Unsupported)
    }
    fn subscribe(
        &self,
        _: &kivo_platform::UiSubscription,
        _: kivo_platform::UiEventSink,
    ) -> kivo_platform::PlatformResult<u64> {
        Err(kivo_platform::PlatformError::Unsupported)
    }
    fn unsubscribe(&self, _: u64) -> kivo_platform::PlatformResult<()> {
        Ok(())
    }
}

/// UI Automation whose subscribed events also go onto the event bus (TOOL-21): whoever asked
/// for them gets them, and so does everything listening to KIVO's events.
struct BusUia {
    inner: Arc<dyn UiAutomation>,
    bus: kivo_core::bus::EventBus,
}

fn publish(bus: &kivo_core::bus::EventBus, event: &kivo_platform::UiEvent) {
    use kivo_platform::UiEvent as E;
    let (kind, element, name) = match event {
        E::FocusChanged { element, name, .. } => ("focusChanged", element.0.clone(), name.clone()),
        E::WindowOpened { element, name } => ("windowOpened", element.0.clone(), name.clone()),
        E::WindowClosed { element } => (
            "windowClosed",
            element.as_ref().map(|e| e.0.clone()).unwrap_or_default(),
            String::new(),
        ),
        E::StructureChanged { window } => ("structureChanged", window.0.to_string(), String::new()),
        E::PropertyChanged {
            element, property, ..
        } => ("propertyChanged", element.0.clone(), property.clone()),
    };
    bus.publish(kivo_core::Event::new(kivo_core::event::EventKind::System(
        kivo_core::event::SystemEvent::Automation {
            kind: kind.into(),
            element,
            name,
        },
    )));
}

impl UiAutomation for BusUia {
    fn find(
        &self,
        q: &kivo_platform::ElementQuery,
        limit: usize,
    ) -> kivo_platform::PlatformResult<Vec<kivo_platform::UiNode>> {
        self.inner.find(q, limit)
    }
    fn tree(
        &self,
        w: kivo_platform::WindowId,
        depth: u8,
        nodes: usize,
    ) -> kivo_platform::PlatformResult<kivo_platform::UiNode> {
        self.inner.tree(w, depth, nodes)
    }
    fn focused(&self) -> kivo_platform::PlatformResult<Option<kivo_platform::UiNode>> {
        self.inner.focused()
    }
    fn selected_text(&self) -> kivo_platform::PlatformResult<Option<String>> {
        self.inner.selected_text()
    }
    fn describe(
        &self,
        e: &kivo_platform::ElementRef,
    ) -> kivo_platform::PlatformResult<kivo_platform::UiNode> {
        self.inner.describe(e)
    }
    fn invoke(&self, e: &kivo_platform::ElementRef) -> kivo_platform::PlatformResult<()> {
        self.inner.invoke(e)
    }
    fn set_value(
        &self,
        e: &kivo_platform::ElementRef,
        v: &str,
    ) -> kivo_platform::PlatformResult<()> {
        self.inner.set_value(e, v)
    }
    fn toggle(&self, e: &kivo_platform::ElementRef) -> kivo_platform::PlatformResult<bool> {
        self.inner.toggle(e)
    }
    fn select(&self, e: &kivo_platform::ElementRef) -> kivo_platform::PlatformResult<()> {
        self.inner.select(e)
    }
    fn expand(&self, e: &kivo_platform::ElementRef, x: bool) -> kivo_platform::PlatformResult<()> {
        self.inner.expand(e, x)
    }
    fn scroll_into_view(&self, e: &kivo_platform::ElementRef) -> kivo_platform::PlatformResult<()> {
        self.inner.scroll_into_view(e)
    }
    fn bounds(
        &self,
        e: &kivo_platform::ElementRef,
    ) -> kivo_platform::PlatformResult<kivo_platform::Rect> {
        self.inner.bounds(e)
    }
    fn subscribe(
        &self,
        s: &kivo_platform::UiSubscription,
        sink: kivo_platform::UiEventSink,
    ) -> kivo_platform::PlatformResult<u64> {
        let bus = self.bus.clone();
        self.inner.subscribe(
            s,
            Box::new(move |event| {
                publish(&bus, &event);
                sink(event);
            }),
        )
    }
    fn unsubscribe(&self, id: u64) -> kivo_platform::PlatformResult<()> {
        self.inner.unsubscribe(id)
    }
}
