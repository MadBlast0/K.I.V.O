//! The tool tests' rig: `Controls` over the testkit fakes, a temp folder with a copy of the
//! synthetic file tree, and a fake browser extension. Nothing touches the real desktop.

use crate::appreg::AppRegistry;
use crate::browser::{Browser, ManagedBrowser, Page, PageTarget, Tab, no_extension};
use crate::controls::{Controls, controls};
use crate::registry::Tool;
use kivo_core::capability::{Capability, CapabilitySettings};
use kivo_core::config::Tools as ToolsConfig;
use kivo_core::tool::ToolError;
use kivo_platform::{Rect, SecretHandle, Secrets, WindowId, WindowInfo};
use kivo_testkit::control::{APP_EXE, APP_WINDOW};
use kivo_testkit::{
    FakeClipboard, FakeCommands, FakeDisplays, FakeFiles, FakeInput, FakeOcr, FakePower,
    FakeSecrets, FakeUia, FakeWindows,
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

#[derive(Default)]
pub(crate) struct FakeBrowser {
    tabs: Mutex<Option<Vec<Tab>>>,
    page: Mutex<Page>,
    pub typed: Mutex<Vec<(PageTarget, String)>>,
}

impl FakeBrowser {
    pub(crate) fn connect(&self, tabs: Vec<Tab>) {
        *self.tabs.lock().unwrap() = Some(tabs);
    }
    pub(crate) fn page(&self, page: Page) {
        *self.page.lock().unwrap() = page;
    }
}

impl Browser for FakeBrowser {
    fn connected(&self) -> bool {
        self.tabs.lock().unwrap().is_some()
    }
    fn tabs(&self) -> Result<Vec<Tab>, ToolError> {
        self.tabs.lock().unwrap().clone().ok_or_else(no_extension)
    }
    fn read(&self, _: Option<i64>) -> Result<Page, ToolError> {
        if !self.connected() {
            return Err(no_extension());
        }
        Ok(self.page.lock().unwrap().clone())
    }
    fn selection(&self, _: Option<i64>) -> Result<String, ToolError> {
        Ok("selected words".into())
    }
    fn click(&self, _: Option<i64>, target: &PageTarget) -> Result<String, ToolError> {
        Ok(target
            .text
            .clone()
            .or(target.selector.clone())
            .unwrap_or_default())
    }
    fn type_text(
        &self,
        _: Option<i64>,
        target: &PageTarget,
        text: &str,
        _: bool,
    ) -> Result<(), ToolError> {
        self.typed
            .lock()
            .unwrap()
            .push((target.clone(), text.into()));
        Ok(())
    }
}

#[derive(Default)]
pub(crate) struct FakeManaged {
    pub opened: Mutex<Vec<String>>,
}

impl ManagedBrowser for FakeManaged {
    fn open(&self, url: &str) -> Result<Page, ToolError> {
        self.opened.lock().unwrap().push(url.into());
        Ok(Page {
            url: url.into(),
            title: "Example".into(),
            text: "Example Domain".into(),
            ..Page::default()
        })
    }
    fn read(&self) -> Result<Page, ToolError> {
        Ok(Page::default())
    }
    fn click(&self, _: &PageTarget) -> Result<Page, ToolError> {
        Ok(Page::default())
    }
    fn type_text(&self, _: &PageTarget, _: &str, _: bool) -> Result<Page, ToolError> {
        Ok(Page::default())
    }
    fn close(&self) -> Result<(), ToolError> {
        Ok(())
    }
}

pub(crate) struct Rig {
    pub dir: tempfile::TempDir,
    pub tools: Vec<Arc<dyn Tool>>,
    pub uia: Arc<FakeUia>,
    pub input: Arc<FakeInput>,
    pub ocr: Arc<FakeOcr>,
    pub clipboard: Arc<FakeClipboard>,
    pub commands: Arc<FakeCommands>,
    pub displays: Arc<FakeDisplays>,
    pub power: Arc<FakePower>,
    pub windows: Arc<FakeWindows>,
    pub browser: Arc<FakeBrowser>,
    pub secrets: Arc<FakeSecrets>,
    pub voice_output: Arc<Mutex<Option<String>>>,
    pub settings_opened: Arc<Mutex<Vec<String>>>,
    pub uris: Arc<Mutex<Vec<String>>>,
    options: Arc<RwLock<ToolsConfig>>,
    caps: Arc<RwLock<CapabilitySettings>>,
    vision: Arc<std::sync::atomic::AtomicBool>,
}

fn test_app_window() -> WindowInfo {
    WindowInfo {
        id: APP_WINDOW,
        title: "KIVO Test App".into(),
        app_id: APP_EXE.into(),
        bounds: Rect {
            x: 120,
            y: 120,
            width: 520,
            height: 420,
        },
        minimized: false,
    }
}

pub(crate) fn rig() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let uia = Arc::new(FakeUia::default());
    let input = Arc::new(FakeInput::default());
    let ocr = Arc::new(FakeOcr::default());
    let clipboard = Arc::new(FakeClipboard::default());
    let commands = Arc::new(FakeCommands::default());
    let displays = Arc::new(FakeDisplays::default());
    let power = Arc::new(FakePower::default());
    let windows = Arc::new(FakeWindows {
        windows: Mutex::new(vec![test_app_window()]),
        ..Default::default()
    });
    let browser = Arc::new(FakeBrowser::default());
    let secrets = Arc::new(FakeSecrets::default());
    let mut files = FakeFiles::new(&dir.path().join("bin"));
    // The temp folder stands in for the user's own folders.
    files.folders = vec![dir.path().to_path_buf()];
    let files = Arc::new(files);
    let voice_output = Arc::new(Mutex::new(None));
    let settings_opened = Arc::new(Mutex::new(Vec::new()));
    let uris = Arc::new(Mutex::new(Vec::new()));
    let options = Arc::new(RwLock::new(ToolsConfig::default()));
    let caps = Arc::new(RwLock::new(CapabilitySettings::default()));
    let vision = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut apps = AppRegistry::core();
    let mut entry: crate::appreg::AppEntry = toml::from_str(
        "id = \"kivo-test-app\"\nname = \"KIVO Test App\"\naliases = [\"test app\"]\n[match]\nexe = [\"kivo-test-app.exe\"]\n[uia.hints]\nexport = { automationId = \"107\" }\nname = { automationId = \"101\" }\n",
    )
    .unwrap();
    entry.origin = "test".into();
    apps.add(entry);
    let (vo, so, ur, op, ca, vi) = (
        Arc::clone(&voice_output),
        Arc::clone(&settings_opened),
        Arc::clone(&uris),
        Arc::clone(&options),
        Arc::clone(&caps),
        Arc::clone(&vision),
    );
    let c = Arc::new(Controls {
        uia: uia.clone(),
        input: input.clone(),
        ocr: ocr.clone(),
        screen: Arc::new(GreyScreen),
        clipboard: clipboard.clone(),
        files,
        commands: commands.clone(),
        displays: displays.clone(),
        power: power.clone(),
        windows: windows.clone(),
        browser: browser.clone(),
        managed: Some(Arc::new(FakeManaged::default())),
        apps: Arc::new(apps),
        shell_home: workspace,
        language: "en-US".into(),
        voice_output: Arc::new(move |d| *vo.lock().unwrap() = d),
        output_devices: Arc::new(|| {
            vec![
                ("dev-1".into(), "Speakers (Realtek)".into(), true),
                ("dev-2".into(), "Headphones (USB)".into(), false),
            ]
        }),
        open_settings: Arc::new(move |p| {
            so.lock().unwrap().push(p.into());
            Ok(())
        }),
        open_uri: Arc::new(move |u| {
            ur.lock().unwrap().push(u.into());
            Ok(())
        }),
        window_cache: RwLock::new(Vec::new()),
        secrets: secrets.clone(),
        settings: Arc::new(move || op.read().unwrap().clone()),
        caps: Arc::new(move || ca.read().unwrap().clone()),
        vision: Arc::new(move || vi.load(std::sync::atomic::Ordering::SeqCst)),
    });
    let tools = controls(&c);
    Rig {
        dir,
        tools,
        uia,
        input,
        ocr,
        clipboard,
        commands,
        displays,
        power,
        windows,
        browser,
        secrets,
        voice_output,
        settings_opened,
        uris,
        options,
        caps,
        vision,
    }
}

/// A screen that captures a small grey image.
struct GreyScreen;

impl kivo_platform::Screen for GreyScreen {
    fn capture(
        &self,
        _: kivo_platform::CaptureTarget,
    ) -> kivo_platform::PlatformResult<kivo_platform::Image> {
        Ok(kivo_platform::Image {
            width: 64,
            height: 32,
            rgba: vec![128; 64 * 32 * 4],
        })
    }
}

impl Rig {
    pub(crate) fn tool(&self, id: &str) -> Arc<dyn Tool> {
        self.tools
            .iter()
            .find(|t| t.spec().id == id)
            .cloned()
            .unwrap_or_else(|| panic!("no tool {id}"))
    }

    /// A copy of `testenv/files` in the temp folder.
    pub(crate) fn tree(&self) -> PathBuf {
        let root = self.dir.path().join("files");
        if !root.exists() {
            copy(&kivo_testkit::testenv::root().join("files"), &root);
        }
        root
    }

    pub(crate) fn bin(&self) -> PathBuf {
        self.dir.path().join("bin")
    }

    pub(crate) fn secret(&self, provider: &str, name: &str, value: &str) {
        self.secrets
            .set(
                &SecretHandle {
                    provider: provider.into(),
                    name: name.into(),
                },
                kivo_core::Secret::new(value.to_owned()),
            )
            .unwrap();
    }

    pub(crate) fn set_options(&self, f: impl FnOnce(&mut ToolsConfig)) {
        f(&mut self.options.write().unwrap());
    }

    pub(crate) fn enable(&self, caps: &[Capability]) {
        let mut c = self.caps.write().unwrap();
        for cap in caps {
            c.set(*cap, true);
        }
    }

    pub(crate) fn allow_cloud_vision(&self) {
        self.vision.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub(crate) fn app_window(&self) -> String {
        APP_WINDOW.0.to_string()
    }

    /// Another app's window comes to the front.
    pub(crate) fn bring_other_window_to_front(&self) {
        let other = WindowInfo {
            id: WindowId(99),
            title: "Other".into(),
            app_id: r"C:\other.exe".into(),
            bounds: Rect {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            },
            minimized: false,
        };
        self.windows.windows.lock().unwrap().insert(0, other);
    }

    /// The dummy app, titled like a Chrome window.
    pub(crate) fn make_app_a_browser(&self, title: &str) {
        self.windows.windows.lock().unwrap()[0].title = title.into();
        self.uia
            .values
            .lock()
            .unwrap()
            .insert(self.uia.element("110"), "https://example.test/page".into());
    }
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let dest = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy(&e.path(), &dest);
        } else {
            std::fs::copy(e.path(), dest).unwrap();
        }
    }
}
