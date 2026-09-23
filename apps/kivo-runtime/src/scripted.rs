//! A scripted KIVO for end-to-end tests and the `e2e` benchmark (BENCH-08): the real turn engine,
//! speech worker and models, with a scripted microphone (Windows' own voice speaking the
//! request) and fake apps, windows and system controls, so a journey like "mute" never touches
//! the real desktop.

use crate::core::Core;
use crate::engine::{self, Engine};
use crate::infer::{self, Infer};
use crate::{activity, speaker, voice};
use kivo_ipc::protocol::SpeechStatus;
use kivo_platform::{AppEntry, SpeechSynth};
use kivo_store::Database;
use kivo_testkit::{
    FakeApps, FakeAudio, FakeClipboard, FakeCommands, FakeDisplays, FakeFiles, FakeInput,
    FakeNotifications, FakeOcr, FakePower, FakeSystemControl, FakeUia, FakeVerifier, FakeWindows,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

/// Windows' own voice, at 16 kHz mono, as the microphone would hear it.
pub fn spoken(text: &str) -> Vec<f32> {
    let audio = kivo_platform_windows::WindowsSpeech
        .synthesize(text, None)
        .expect("Windows has a voice");
    let mut clip = kivo_audio::RateConverter::convert_all(audio.rate, 16_000, &audio.samples);
    // A moment of quiet at the end, so the pipeline sees the sentence finish.
    clip.extend(std::iter::repeat_n(0.0, 16_000));
    clip
}

/// Windows' voice stand-in: records what KIVO says in-process (a failure, ARCH-09).
#[derive(Default)]
pub struct Heard(pub Mutex<Vec<String>>);

impl SpeechSynth for Heard {
    fn voices(&self) -> kivo_platform::PlatformResult<Vec<kivo_platform::SystemVoice>> {
        Ok(Vec::new())
    }
    fn synthesize(
        &self,
        text: &str,
        _voice: Option<&str>,
    ) -> kivo_platform::PlatformResult<kivo_platform::SynthAudio> {
        self.0.lock().unwrap().push(text.to_owned());
        Ok(kivo_platform::SynthAudio {
            rate: 16_000,
            samples: vec![0.0; 160],
        })
    }
}

/// A whole KIVO turn engine on fakes: the real speech worker and models, a scripted microphone,
/// and apps, windows and system controls that only record what they were asked.
pub struct Rig {
    pub engine: Arc<Engine>,
    pub infer: Infer,
    pub heard: Arc<Heard>,
    pub listener: Arc<voice::Listener>,
    pub system: Arc<kivo_testkit::FakeSystemInfo>,
    pub windows: Arc<FakeWindows>,
    pub core: Arc<Core>,
    pub apps: Arc<FakeApps>,
    pub recorder: activity::Recorder,
    pub db: Arc<Mutex<Database>>,
    /// The scripted microphone: queue what the user says next with `say_next`.
    pub audio: Arc<FakeAudio>,
    /// Where "take a screenshot" saves: a folder of this rig's own under the temp folder.
    pub screenshots: PathBuf,
    /// The system controls (volume, power): fakes that only record what they were asked.
    pub control: Arc<FakeSystemControl>,
    pub brains: Arc<crate::brains::Brains>,
    pub agents: Arc<crate::agents::Agents>,
    /// The computer-control fakes (M4): UI Automation shaped like the dummy app, input that
    /// only records, a command runner that records, Windows Hello that answers as told.
    pub uia: Arc<FakeUia>,
    pub input: Arc<FakeInput>,
    pub commands: Arc<FakeCommands>,
    pub verifier: Arc<FakeVerifier>,
    pub clipboard: Arc<FakeClipboard>,
    /// Background tasks (M5): watchers on fake processes, a notifier that toasts into
    /// `notifications`, and a downloads folder of this rig's own.
    pub tasks: Arc<crate::tasks::Tasks>,
    pub processes: Arc<kivo_testkit::FakeProcesses>,
    pub notifications: Arc<FakeNotifications>,
    pub downloads: PathBuf,
    pub registry: Arc<kivo_tools::Registry>,
    pub routines: Arc<crate::routines::Routines>,
    pub terminals: Arc<kivo_testkit::FakeTerminals>,
    pub workspaces: Arc<crate::workspaces::Workspaces>,
    /// The M5 requests the Control Center makes (Tasks, Routines, Agents, Bypass), as the app
    /// sends them.
    pub rpc: Arc<crate::tasks_rpc::TasksRpc>,
    /// M6: MCP servers (secrets in a fake Credential Manager), skills and connectors, with
    /// "other apps" and the skills folders in this rig's own folder, and the Extensions requests.
    pub mcp: Arc<crate::mcp::Mcp>,
    pub skills: Arc<crate::skills::Skills>,
    pub connectors: Arc<crate::connectors::Connectors>,
    pub extensions: Arc<crate::extensions_rpc::ExtensionsRpc>,
    pub secrets: Arc<kivo_testkit::FakeSecrets>,
    /// The rig's pretend user profile folder (`~`) and `%APPDATA%`, for other apps' setups.
    pub home: PathBuf,
    pub appdata: PathBuf,
    /// Pages a connector's sign-in opened in "the browser" (which follows them, like a user who
    /// signs in at once).
    pub opened: Arc<Mutex<Vec<String>>>,
}

impl Drop for Rig {
    fn drop(&mut self) {
        // The detection thread may still be loading its model: a test must not end inside it.
        self.listener.shutdown(Duration::from_secs(10));
        let _ = std::fs::remove_dir_all(&self.screenshots);
    }
}

/// A rig speaking with the Windows voices. `worker` is the `kivo-infer` program.
pub fn rig(
    worker: PathBuf,
    clip: Vec<f32>,
    model_dir: PathBuf,
) -> (
    Rig,
    tokio::task::JoinHandle<()>,
    tokio::task::JoinHandle<()>,
) {
    rig_with_voice(worker, clip, model_dir, ("system".to_owned(), None))
}

/// A rig with a chosen voice (`("kokoro-82m", Some(dir))`).
pub fn rig_with_voice(
    worker: PathBuf,
    clip: Vec<f32>,
    model_dir: PathBuf,
    voice: (String, Option<PathBuf>),
) -> (
    Rig,
    tokio::task::JoinHandle<()>,
    tokio::task::JoinHandle<()>,
) {
    let core = Arc::new(Core::with_config(kivo_core::KivoConfig::default(), None));
    let db = Arc::new(Mutex::new(Database::in_memory().unwrap()));
    let recorder = activity::Recorder::new(Arc::clone(&db), true);
    let (infer, mut events, sender) = Infer::new(worker);
    infer.configure(
        infer::Engines {
            stt: model_dir
                .is_dir()
                .then(|| (kivo_voice::moonshine::MODEL_ID.to_owned(), model_dir)),
            stt_fallback: None,
            tts: Some(voice),
            threads: 4,
            language: "en-US".into(),
        },
        Duration::from_secs(600),
    );
    let chrome = AppEntry {
        id: "Chrome".into(),
        name: "Google Chrome".into(),
        aliases: vec!["Chrome".into()],
        exe: None,
    };
    // A desktop AI app, for the Agents page (DISC-06).
    let claude_desktop = AppEntry {
        id: "Claude_pzs8sxrjxfjjc!Claude".into(),
        name: "Claude".into(),
        aliases: Vec::new(),
        exe: None,
    };
    let apps = Arc::new(FakeApps {
        installed: vec![chrome.clone(), claude_desktop],
        ..Default::default()
    });
    let windows = Arc::new(FakeWindows::default());
    let audio = Arc::new(FakeAudio::microphone(clip));
    let speaker = Arc::new(speaker::Speaker::new(audio.clone(), None));
    let mic = Arc::clone(&audio);
    let catalog = Arc::new(RwLock::new(vec![chrome]));
    static RIGS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let screenshots = std::env::temp_dir().join(format!(
        "kivo-scripted-{}-{}",
        std::process::id(),
        RIGS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let control = Arc::new(FakeSystemControl::default());
    let notifications = Arc::new(FakeNotifications::default());
    let env = Arc::new(kivo_tools::Env {
        apps: apps.clone(),
        windows: windows.clone(),
        control: control.clone(),
        screen: Arc::new(PlainScreen),
        notifications: notifications.clone(),
        catalog: Arc::clone(&catalog),
        screenshots: screenshots.clone(),
    });
    // The computer-control tools on fakes (M4).
    let uia = Arc::new(FakeUia::default());
    let input = Arc::new(FakeInput::default());
    let commands = Arc::new(FakeCommands::default());
    let verifier = Arc::new(FakeVerifier::default());
    let clipboard = Arc::new(FakeClipboard::default());
    let terminals = Arc::new(kivo_testkit::FakeTerminals::default());
    let vision = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut files = FakeFiles::new(&screenshots.join("bin"));
    files.folders = vec![screenshots.clone()];
    let controls = crate::controls::build(
        crate::controls::Platform {
            uia: uia.clone(),
            input: input.clone(),
            ocr: Arc::new(FakeOcr::default()),
            screen: Arc::new(PlainScreen),
            clipboard: clipboard.clone(),
            files: Arc::new(files),
            commands: commands.clone(),
            displays: Arc::new(FakeDisplays::default()),
            power: Arc::new(FakePower::default()),
            windows: windows.clone(),
            secrets: Arc::new(kivo_testkit::FakeSecrets::default()),
            browser: Arc::new(kivo_tools::NoExtension),
            managed: None,
            open_settings: Arc::new(|_| Ok(())),
            open_uri: Arc::new(|_| Ok(())),
            output_devices: Arc::new(Vec::new),
            voice_output: Arc::new(|_| {}),
            terminals: terminals.clone(),
            workspace_folder: {
                let db = Arc::clone(&db);
                Arc::new(move |name| crate::workspaces::folder_named(&db, name))
            },
        },
        &core,
        crate::controls::app_registry(None),
        Arc::clone(&vision),
        screenshots.clone(),
    );
    let app_registry = Arc::clone(&controls.apps);
    let task_handle: crate::task_tools::TaskHandle = Arc::default();
    let mut tools = kivo_tools::builtin(&env);
    tools.extend(kivo_tools::controls(&controls));
    tools.extend(crate::task_tools::tools(&task_handle));
    let registry = Arc::new(kivo_tools::Registry::new(tools));
    let heard = Arc::new(Heard::default());
    // Brains are put in by each test (scripted brains, a fake agent); none by default.
    let brains = Arc::new(crate::brains::Brains::new(
        Arc::clone(&db),
        Arc::new(kivo_testkit::FakeSecrets::default()),
        0,
    ));
    let agents = crate::agents::Agents::new(Arc::clone(&db), std::env::temp_dir());
    let system = Arc::new(kivo_testkit::FakeSystemInfo::default());
    let engine = Arc::new(Engine::new(engine::Parts {
        core: Arc::clone(&core),
        infer: infer.clone(),
        speaker: Arc::clone(&speaker),
        registry: Arc::clone(&registry),
        recorder: recorder.clone(),
        apps: apps.clone(),
        windows: windows.clone(),
        app_catalog: catalog,
        router: kivo_intent::IntentRouter::new(kivo_intent::Grammar::bundled("en").unwrap()),
        system: system.clone(),
        fallback_voice: Some(heard.clone()),
        brains: Arc::clone(&brains),
        agents: Arc::clone(&agents),
        verifier: verifier.clone(),
        commands: Some(commands.clone()),
        input_abort: None,
        vision,
    }));
    agents.set_permissions(Arc::new(engine::EnginePermissions(Arc::downgrade(&engine))));
    engine.refresh_apps();
    engine.set_app_registry(app_registry);
    engine.set_uia(uia.clone());
    // Background tasks (M5) on fakes.
    let processes = Arc::new(kivo_testkit::FakeProcesses::default());
    let downloads = screenshots.join("downloads");
    let _ = std::fs::create_dir_all(&downloads);
    let watchers = Arc::new(crate::watchers::Watchers::new(
        processes.clone(),
        windows.clone(),
        uia.clone(),
        downloads.clone(),
    ));
    let notifier =
        crate::notifier::Notifier::new(Arc::clone(&core), system.clone(), notifications.clone(), 0);
    let voice: Arc<dyn crate::notifier::Voice> = engine.clone();
    notifier.set_voice(Arc::downgrade(&voice));
    let tasks = crate::tasks::Tasks::new(crate::tasks::TaskParts {
        core: Arc::clone(&core),
        recorder: recorder.clone(),
        registry: Arc::clone(&registry),
        watchers,
        notifier,
        brains: Arc::clone(&brains),
        agents: Arc::clone(&agents),
        windows: windows.clone(),
    });
    let _ = task_handle.set(Arc::downgrade(&tasks));
    engine.set_tasks(Arc::clone(&tasks));
    let routines = crate::routines::Routines::new(
        Arc::clone(&core),
        Arc::clone(&db),
        Arc::clone(&tasks),
        Arc::clone(&registry),
    );
    engine.set_routines(Arc::clone(&routines));
    let workspaces = crate::workspaces::Workspaces::new(
        Arc::clone(&core),
        Arc::clone(&db),
        screenshots.join("kivo-data"),
    );
    engine.set_workspaces(Arc::clone(&workspaces));
    // What KIVO remembers, for brains and for agents through KIVO's MCP server (CONV-24).
    registry.set_live("memory.", crate::memory_tools::tools(&db, &workspaces));
    // M6 on fakes, in this rig's own folder.
    let home = screenshots.join("home");
    let appdata = screenshots.join("appdata");
    let _ = std::fs::create_dir_all(&home);
    let _ = std::fs::create_dir_all(&appdata);
    let secrets = Arc::new(kivo_testkit::FakeSecrets::default());
    let mcp = crate::mcp::Mcp::new(
        Arc::clone(&core),
        Arc::clone(&db),
        Arc::clone(&registry),
        secrets.clone(),
        kivo_mcp::imports::Places {
            home: home.clone(),
            appdata: appdata.clone(),
            local_appdata: screenshots.join("localappdata"),
            projects: Vec::new(),
        },
    );
    let skills = crate::skills::Skills::new(
        Arc::clone(&core),
        Arc::clone(&db),
        appdata.join("KIVO").join("skills"),
        home.clone(),
    );
    registry.set_live("skills.", crate::skills::tools(&skills));
    engine.set_skills(Arc::clone(&skills));
    let opened: Arc<Mutex<Vec<String>>> = Arc::default();
    let connectors = crate::connectors::Connectors::new(
        Arc::clone(&core),
        Arc::clone(&mcp),
        Some(commands.clone() as Arc<dyn kivo_platform::CommandRunner>),
        {
            let engine = Arc::clone(&engine);
            Arc::new(move || engine.installed_apps())
        },
        Arc::clone(&controls.apps),
        None,
        {
            let opened = Arc::clone(&opened);
            Arc::new(move |page: &str| {
                opened
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(page.to_owned());
                // The "user" signs in at once: the browser follows the page's redirects.
                let page = page.to_owned();
                tokio::spawn(async move {
                    let _ = reqwest::get(page).await;
                });
                Ok(())
            })
        },
    );
    let extensions = Arc::new(crate::extensions_rpc::ExtensionsRpc {
        core: Arc::clone(&core),
        engine: Arc::clone(&engine),
        mcp: Arc::clone(&mcp),
        connectors: Arc::clone(&connectors),
        skills: Arc::clone(&skills),
    });
    let rpc = Arc::new(crate::tasks_rpc::TasksRpc {
        core: Arc::clone(&core),
        engine: Arc::clone(&engine),
        tasks: Arc::clone(&tasks),
        routines: Arc::clone(&routines),
        workspaces: Arc::clone(&workspaces),
        verifier: verifier.clone(),
        terminals: terminals.clone(),
        terminal_sessions: Arc::clone(&controls.terminal_sessions),
        apps: Arc::clone(&controls.apps),
        uia: uia.clone(),
        clipboard: clipboard.clone(),
        db: Arc::clone(&db),
    });
    let (signals, mut voice_signals) = tokio::sync::mpsc::unbounded_channel();
    let (levels, _levels_rx) = tokio::sync::watch::channel(0.0);
    let listener = Arc::new(voice::start(voice::Pipeline {
        audio,
        device: None,
        // The end-of-turn model if this PC has it (VOICE-33); silence decides otherwise.
        turn_model: kivo_platform::Paths::user()
            .map(|p| p.models().join(kivo_store::models::SMART_TURN))
            .unwrap_or_default(),
        vad_model: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/models/silero_vad.onnx"),
        infer: infer.clone(),
        speaker,
        levels,
        signals,
        qos: Arc::new(kivo_testkit::FakeThreadQos::default()),
    }));
    engine.set_listener(Arc::clone(&listener));
    core.set_speech_status(SpeechStatus::Ready);

    let worker_task = tokio::spawn(infer::supervise(infer.clone(), sender, core.shutdown()));
    let pump = {
        let engine = Arc::clone(&engine);
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    signal = voice_signals.recv() => match signal {
                        Some(signal) => engine::handle_signal(&engine, signal).await,
                        None => break,
                    },
                    event = events.recv() => match event {
                        Some(event) => engine::handle_infer_event(&engine, event).await,
                        None => break,
                    },
                }
            }
        })
    };
    (
        Rig {
            engine,
            infer,
            heard,
            listener,
            system,
            windows,
            core,
            apps,
            recorder,
            db,
            audio: mic,
            screenshots,
            control,
            brains,
            agents,
            uia,
            input,
            commands,
            verifier,
            clipboard,
            tasks,
            processes,
            notifications,
            downloads,
            registry,
            routines,
            terminals,
            workspaces,
            rpc,
            mcp,
            skills,
            connectors,
            extensions,
            secrets,
            home,
            appdata,
            opened,
        },
        worker_task,
        pump,
    )
}

/// A screen that is one small grey picture, so a screenshot never captures this PC's desktop.
pub struct PlainScreen;

impl kivo_platform::Screen for PlainScreen {
    fn capture(
        &self,
        _target: kivo_platform::CaptureTarget,
    ) -> kivo_platform::PlatformResult<kivo_platform::Image> {
        Ok(kivo_platform::Image {
            width: 16,
            height: 9,
            rgba: [128, 128, 128, 255].repeat(16 * 9),
        })
    }
}
