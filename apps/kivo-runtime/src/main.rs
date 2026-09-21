//! The KIVO runtime: the always-on core process (ARCHITECTURE §1). It owns the state, the tray and
//! the IPC server, and launches and supervises the desktop app. Audio, voice, routing, tools and
//! permissions join it milestone by milestone.

// No console window when started by the app or at sign-in.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use kivo_intent::{Grammar, IntentRouter};
use kivo_ipc::{Server, ServerConfig, SessionToken};
use kivo_platform::Paths;
use kivo_runtime::activity::Recorder;
use kivo_runtime::app::{self, Launch, ProcessLauncher, Timing};
use kivo_runtime::args::Args;
use kivo_runtime::core::Core;
use kivo_runtime::engine::{self, Engine};
use kivo_runtime::infer::{self, Infer};
use kivo_runtime::models::Models;
use kivo_runtime::speaker::Speaker;
#[cfg(windows)]
use kivo_runtime::{hotkeys, tray};
use kivo_runtime::{rpc, voice};
use kivo_store::{Database, Notice};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex, RwLock};

fn main() -> ExitCode {
    let args = match Args::parse(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("kivo-runtime: {e}");
            return ExitCode::from(2);
        }
    };
    let Some(paths) = Paths::user() else {
        eprintln!("kivo-runtime: couldn't find the user's AppData folders");
        return ExitCode::FAILURE;
    };

    // Startup order (plan §126): single instance, lightweight config, logging, then the rest.
    #[cfg(windows)]
    let _instance = match claim_instance() {
        Ok(Some(guard)) => guard,
        Ok(None) => return ExitCode::SUCCESS, // KIVO is already running for this user
        Err(e) => {
            eprintln!("kivo-runtime: {e}");
            return ExitCode::FAILURE;
        }
    };

    let loaded = kivo_store::config::load(&paths.config_file());
    let config = match &loaded {
        Ok(l) => l.config.clone(),
        Err(_) => kivo_core::KivoConfig::default(),
    };
    // A settings file from a newer KIVO is used read-only: nothing here may overwrite it.
    let writable = !matches!(
        &loaded,
        Ok(l) if matches!(l.notice, Some(Notice::FromNewerVersion { .. }))
    ) && loaded.is_ok();
    let _log = kivo_store::logging::init(&paths.logs(), "info", config.privacy.debug_transcripts)
        .map_err(|e| eprintln!("kivo-runtime: logging is off: {e}"))
        .ok();
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        ?args,
        "KIVO runtime starting"
    );
    match loaded {
        Ok(l) => log_config_notice(l.notice.as_ref()),
        Err(e) => tracing::error!(%e, "couldn't read the settings; using the defaults"),
    }

    let tokio = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("kivo-rt")
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!(%e, "couldn't start the async runtime");
            return ExitCode::FAILURE;
        }
    };
    let code = tokio.block_on(run(args, &paths, config, writable));
    tracing::info!("KIVO runtime stopped");
    code
}

#[cfg(windows)]
fn claim_instance() -> Result<Option<kivo_platform_windows::instance::InstanceGuard>, String> {
    let sid = kivo_ipc::transport::current_user_sid().map_err(|e| e.to_string())?;
    kivo_platform_windows::instance::claim(&format!("Local\\KIVO.Runtime.{sid}"))
        .map_err(|e| e.to_string())
}

fn log_config_notice(notice: Option<&Notice>) {
    match notice {
        None => {}
        Some(Notice::Created) => tracing::info!("created the settings file"),
        Some(Notice::Migrated { from, backup }) => {
            tracing::info!(from, backup = %backup.display(), "settings migrated");
        }
        Some(Notice::Invalid { reason, kept_at }) => tracing::warn!(
            reason,
            kept_at = %kept_at.display(),
            "the settings file was invalid; using the defaults"
        ),
        Some(Notice::FromNewerVersion { version }) => tracing::warn!(
            version,
            "the settings file is from a newer KIVO; using it read-only"
        ),
    }
}

async fn run(args: Args, paths: &Paths, config: kivo_core::KivoConfig, writable: bool) -> ExitCode {
    #[cfg(windows)]
    tracing::info!(capabilities = ?kivo_platform_windows::detect_capabilities(), "platform");

    let db = match Database::open(&paths.database()) {
        Ok(db) => db,
        Err(e) => {
            tracing::error!(%e, "couldn't open the database");
            return ExitCode::FAILURE;
        }
    };

    // IPC: a fresh session token each start, then the user-only pipe.
    let token_file = paths.run().join("session.token");
    let token = match SessionToken::generate().and_then(|t| t.write(&token_file).map(|()| t)) {
        Ok(token) => token,
        Err(e) => {
            tracing::error!(%e, "couldn't write the session token");
            return ExitCode::FAILURE;
        }
    };
    let endpoint = match kivo_ipc::transport::endpoint(&paths.run()) {
        Ok(endpoint) => endpoint,
        Err(e) => {
            tracing::error!(%e, "couldn't name the IPC endpoint");
            return ExitCode::FAILURE;
        }
    };
    let server = match Server::bind(ServerConfig::new(
        endpoint,
        token,
        env!("CARGO_PKG_VERSION").into(),
    )) {
        Ok(server) => server,
        Err(e) => {
            // Another process holds the pipe name: refuse to run beside it (SECURITY §9).
            tracing::error!(%e, "couldn't create the IPC endpoint");
            return ExitCode::FAILURE;
        }
    };

    let push_to_talk_keys = config.voice.push_to_talk.clone();
    let emergency_stop_keys = config.permissions.emergency_stop.clone();
    let show_tray = config.general.tray_icon;
    let keep_history = config.privacy.retention_days > 0;
    let core = Arc::new(Core::with_config(
        config.clone(),
        writable.then(|| paths.config_file()),
    ));
    let clients = server.connected_clients();
    // The mic level for the Island's waveform (ARCH-20), published only while listening.
    let (levels, levels_rx) = tokio::sync::watch::channel(0.0_f32);
    let server = server.with_levels(levels_rx);

    // The pieces of a turn: the speech worker, the speaker, the tools and what KIVO records.
    let db = Arc::new(Mutex::new(db));
    let recorder = Recorder::new(Arc::clone(&db), keep_history);
    let (infer, mut infer_events, infer_sender) = Infer::new(worker_program());
    let models = Arc::new(Models::new(
        paths.models(),
        Arc::clone(&core),
        infer.clone(),
    ));
    let (signals, mut voice_signals) = tokio::sync::mpsc::unbounded_channel();

    #[cfg(windows)]
    let platform = windows_platform(paths);
    #[cfg(not(windows))]
    let platform = return ExitCode::FAILURE;

    let app_catalog = Arc::new(RwLock::new(Vec::new()));
    let tools_env = Arc::new(kivo_tools::Env {
        apps: Arc::clone(&platform.apps),
        windows: Arc::clone(&platform.windows),
        control: Arc::clone(&platform.control),
        screen: Arc::clone(&platform.screen),
        notifications: Arc::clone(&platform.notifications),
        catalog: Arc::clone(&app_catalog),
        screenshots: screenshots_folder(),
    });
    let registry = Arc::new(kivo_tools::Registry::new(kivo_tools::builtin(&tools_env)));
    let speaker = Arc::new(Speaker::new(
        Arc::clone(&platform.audio),
        config
            .voice
            .output_device
            .clone()
            .map(kivo_platform::DeviceId),
    ));
    speaker.set_sounds(config.sounds.enabled, config.sounds.volume);
    let grammar = match Grammar::bundled(&config.general.language) {
        Ok(grammar) => grammar,
        Err(e) => {
            tracing::warn!(%e, "no command grammar for this language; falling back to English");
            Grammar::bundled("en").expect("the English grammar ships with KIVO")
        }
    };
    let engine = Arc::new(Engine::new(engine::Parts {
        core: Arc::clone(&core),
        infer: infer.clone(),
        speaker: Arc::clone(&speaker),
        registry: Arc::clone(&registry),
        recorder: recorder.clone(),
        apps: Arc::clone(&platform.apps),
        windows: Arc::clone(&platform.windows),
        app_catalog: Arc::clone(&app_catalog),
        router: IntentRouter::new(grammar),
    }));
    {
        // The app index is read once at startup, off the startup path.
        let engine = Arc::clone(&engine);
        tokio::task::spawn_blocking(move || engine.refresh_apps());
    }
    let listener = Arc::new(voice::start(voice::Pipeline {
        audio: Arc::clone(&platform.audio),
        device: config
            .voice
            .input_device
            .clone()
            .map(kivo_platform::DeviceId),
        vad_model: vad_model_path(),
        infer: infer.clone(),
        speaker: Arc::clone(&speaker),
        levels,
        signals,
    }));
    engine.set_listener(Arc::clone(&listener));
    models.ensure_speech(&config);

    let ipc = tokio::spawn(server.run(
        Arc::new(rpc::Rpc::new(
            Arc::clone(&core),
            Arc::clone(&engine),
            Arc::clone(&models),
            recorder.clone(),
        )),
        core.bus.clone(),
        core.state(),
        core.shutdown(),
    ));

    let worker = tokio::spawn(infer::supervise(
        infer.clone(),
        infer_sender,
        core.shutdown(),
    ));

    // The voice pipeline and the speech worker drive the turn.
    let turns = {
        let engine = Arc::clone(&engine);
        let shutdown = core.shutdown();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    signal = voice_signals.recv() => match signal {
                        Some(signal) => engine::handle_signal(&engine, signal).await,
                        None => break,
                    },
                    event = infer_events.recv() => match event {
                        Some(event) => engine::handle_infer_event(&engine, event).await,
                        None => break,
                    },
                    () = shutdown.cancelled() => break,
                }
            }
        })
    };

    #[cfg(windows)]
    let tray = show_tray.then(|| tokio::spawn(tray::run(Arc::clone(&core), Arc::clone(&engine))));

    #[cfg(windows)]
    let push_to_talk = tokio::spawn(hotkeys::run(
        Arc::clone(&core),
        Arc::clone(&engine),
        push_to_talk_keys,
        emergency_stop_keys,
    ));

    let supervisor = (!args.no_app).then(|| {
        let initial = if args.from_app {
            None
        } else if args.autostart {
            Some(Launch::Background)
        } else {
            Some(Launch::Show(None))
        };
        tokio::spawn(app::supervise(
            Arc::clone(&core),
            Arc::new(ProcessLauncher::beside_runtime()),
            initial,
            clients,
            Timing::default(),
        ))
    });

    // Ctrl+C (development) quits like the tray's Quit.
    let signals_task = {
        let core = Arc::clone(&core);
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                core.quit();
            }
        })
    };

    tracing::info!("KIVO is ready");
    core.shutdown().cancelled().await;

    // Shutdown order (plan §127): the UIs were told (`ShuttingDown`) and every task watches the
    // shutdown token; wait for them, then close the store and remove the token.
    signals_task.abort();
    let mut code = ExitCode::SUCCESS;
    // Stop new work, then the voice pipeline and the worker (plan §127).
    engine.cancel(kivo_core::event::CancelReason::Shutdown);
    drop(listener);
    infer.shutdown().await;
    let _ = worker.await;
    turns.abort();
    match ipc.await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            tracing::error!(%e, "the IPC server failed");
            code = ExitCode::FAILURE;
        }
        Err(e) => tracing::error!(%e, "the IPC server panicked"),
    }
    #[cfg(windows)]
    {
        if let Some(tray) = tray {
            let _ = tray.await;
        }
        let _ = push_to_talk.await;
    }
    if let Some(supervisor) = supervisor {
        let _ = supervisor.await;
    }
    drop(db);
    if let Err(e) = std::fs::remove_file(&token_file) {
        tracing::debug!(%e, "couldn't remove the session token file");
    }
    code
}

/// Everything the runtime touches the operating system through.
#[cfg(windows)]
struct Platform {
    audio: Arc<dyn kivo_platform::AudioIo>,
    apps: Arc<dyn kivo_platform::Apps>,
    windows: Arc<dyn kivo_platform::Windows>,
    control: Arc<dyn kivo_platform::SystemControl>,
    screen: Arc<dyn kivo_platform::Screen>,
    notifications: Arc<dyn kivo_platform::Notifications>,
}

#[cfg(windows)]
fn windows_platform(paths: &Paths) -> Platform {
    use kivo_platform_windows::{
        WindowsApps, WindowsAudio, WindowsControl, WindowsScreen, WindowsWindows,
    };
    let (answers, answered) = std::sync::mpsc::channel();
    let icon = paths.local.join("icon.png");
    let notifications: Arc<dyn kivo_platform::Notifications> =
        match kivo_platform_windows::WindowsNotifications::new(
            icon.exists().then_some(icon.as_path()),
            answers,
        ) {
            Ok(n) => Arc::new(n),
            Err(e) => {
                tracing::warn!(%e, "notifications are unavailable");
                Arc::new(NoNotifications)
            }
        };
    // Toast buttons come back on their own thread; they are handled in `app`/`core` (UX-57).
    std::thread::Builder::new()
        .name("kivo-toasts".into())
        .spawn(move || {
            while let Ok(answer) = answered.recv() {
                tracing::info!(action = answer.action, "notification answered");
            }
        })
        .ok();
    Platform {
        audio: Arc::new(WindowsAudio),
        apps: Arc::new(WindowsApps),
        windows: Arc::new(WindowsWindows),
        control: Arc::new(WindowsControl),
        screen: Arc::new(WindowsScreen),
        notifications,
    }
}

/// Used when Windows won't give KIVO notifications.
#[cfg(windows)]
struct NoNotifications;

#[cfg(windows)]
impl kivo_platform::Notifications for NoNotifications {
    fn show(
        &self,
        _notification: &kivo_platform::Notification,
    ) -> kivo_platform::PlatformResult<()> {
        Err(kivo_platform::PlatformError::Unsupported)
    }
}

/// `kivo-infer.exe` beside the runtime (installed) or in the same build folder (development).
fn worker_program() -> PathBuf {
    let name = if cfg!(windows) {
        "kivo-infer.exe"
    } else {
        "kivo-infer"
    };
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(name)))
        .unwrap_or_else(|| PathBuf::from(name))
}

/// The bundled voice-activity model: beside the runtime when installed, in `assets/models` when
/// running from the repository.
fn vad_model_path() -> PathBuf {
    const NAME: &str = "silero_vad.onnx";
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("resources").join(NAME)));
    if let Some(path) = beside.filter(|p| p.is_file()) {
        return path;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/models")
        .join(NAME)
}

/// Where screenshots go: the user's Pictures\Screenshots folder, like Windows itself uses.
fn screenshots_folder() -> PathBuf {
    dirs::picture_dir()
        .map(|p| p.join("Screenshots"))
        .unwrap_or_else(std::env::temp_dir)
}
