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
use kivo_runtime::lifecycle::Lifecycle;
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

    if args.health {
        return health(&paths);
    }

    // Crashes of this process are written to the crashes folder from here on (ARCH-10).
    #[cfg(windows)]
    kivo_platform_windows::crash::install(&paths.crashes(), "kivo-runtime");
    // Window positions and captures in physical pixels (TOOL-07, TOOL-14, the Island's anchor).
    kivo_platform_windows::dpi_aware();

    // Startup order (plan §126): single instance, lightweight config, logging, the event bus and
    // state (`Core`), audio and voice detection, then OS registrations (tray, hotkeys). Speech
    // models load only when a request needs them (VOICE-34).
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
    // A new settings file: the installer's startup choice may need adopting (DIST-05).
    let first_run = matches!(&loaded, Ok(l) if matches!(l.notice, Some(Notice::Created)));
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
    let code = tokio.block_on(run(args, &paths, config, writable, first_run));
    tracing::info!("KIVO runtime stopped");
    code
}

#[cfg(windows)]
fn claim_instance() -> Result<Option<kivo_platform_windows::instance::InstanceGuard>, String> {
    let sid = kivo_ipc::transport::current_user_sid().map_err(|e| e.to_string())?;
    kivo_platform_windows::instance::claim(&format!("Local\\KIVO.Runtime.{sid}"))
        .map_err(|e| e.to_string())
}

/// `--health`: exit 0 if this user's runtime answers a ping within 30 s (REL-06). The exit code is
/// the answer; release builds have no console for the message.
fn health(paths: &Paths) -> ExitCode {
    const WAIT: std::time::Duration = std::time::Duration::from_secs(30);
    let run = paths.run();
    let answer = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())
        .and_then(|rt| {
            let endpoint = kivo_ipc::transport::endpoint(&run).map_err(|e| e.to_string())?;
            let token_file = run.join("session.token");
            let read = || kivo_ipc::SessionToken::read(&token_file).map(|t| t.as_str().to_owned());
            rt.block_on(kivo_ipc::health(&endpoint, read, WAIT))
                .map_err(|e| match e {
                    kivo_ipc::ClientError::Cancelled => format!("no answer within {WAIT:?}"),
                    e => e.to_string(),
                })
        });
    match answer {
        Ok(version) => {
            println!("KIVO runtime {version} is running");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("kivo-runtime: not healthy: {e}");
            ExitCode::FAILURE
        }
    }
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

async fn run(
    args: Args,
    paths: &Paths,
    config: kivo_core::KivoConfig,
    writable: bool,
    first_run: bool,
) -> ExitCode {
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
    models.load_measurements(&db.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
    let (signals, mut voice_signals) = tokio::sync::mpsc::unbounded_channel();

    #[cfg(windows)]
    let (platform, toast_answers) = windows_platform(paths);
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
    speaker.configure(&config.sounds);
    let grammar = match Grammar::bundled(&config.general.language) {
        Ok(grammar) => grammar,
        Err(e) => {
            tracing::warn!(%e, "no command grammar for this language; falling back to English");
            Grammar::bundled("en").expect("the English grammar ships with KIVO")
        }
    };
    // The brains (BRAINS §3–5): built from the user's connections, keys read from Credential
    // Manager inside the runtime only (SEC-17).
    let brains = Arc::new(kivo_runtime::brains::Brains::new(
        Arc::clone(&db),
        Arc::new(kivo_platform_windows::WindowsSecrets),
        kivo_platform_windows::utc_offset_minutes(),
    ));
    brains.reload(&config);
    let agents = kivo_runtime::agents::Agents::new(
        Arc::clone(&db),
        dirs::home_dir().unwrap_or_else(std::env::temp_dir),
    );
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
        system: Arc::clone(&platform.system),
        fallback_voice: Some(Arc::new(kivo_platform_windows::WindowsSpeech)),
        brains: Arc::clone(&brains),
        agents: Arc::clone(&agents),
    }));
    // Agents' permission requests go to the turn engine's permission flow (BRAIN-15).
    agents.set_permissions(Arc::new(engine::EnginePermissions(Arc::downgrade(&engine))));
    // What's already on this PC: CLI agents and local model servers (DISCOVERY §1.1).
    let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
    let discovery = kivo_runtime::discovery::Discovery::new(
        Arc::clone(&db),
        vec![
            Arc::new(kivo_runtime::discovery::CliDetector {
                path: {
                    let home = home.clone();
                    Arc::new(move || {
                        let mut dirs = kivo_platform_windows::environment::current_path();
                        dirs.extend(kivo_runtime::discovery::install_folders(&home));
                        dirs
                    })
                },
                home,
            }),
            Arc::new(kivo_runtime::discovery::LocalServerDetector::default()),
        ],
        Arc::clone(&brains),
    );
    let brains_rpc = Arc::new(kivo_runtime::brains_rpc::BrainsRpc::new(
        Arc::clone(&core),
        Arc::clone(&engine),
        discovery,
        recorder.clone(),
        Arc::clone(&platform.control),
        Arc::new(kivo_platform_windows::environment::open_in_terminal),
    ));
    brains_rpc.start_background(core.shutdown());
    brains_rpc.start_pruning(core.shutdown());
    // Paraphrased commands (BRAIN-03): when the small embedding model is on this PC, and as soon
    // as the user downloads it.
    {
        let engine = Arc::clone(&engine);
        let models = Arc::clone(&models);
        let core = Arc::clone(&core);
        let mut events = core.bus.subscribe();
        tokio::spawn(async move {
            let load = |engine: Arc<Engine>, models: Arc<Models>, language: String| async move {
                let dir = models.installed_dir(kivo_store::models::EMBEDDING_MODEL);
                let semantic = match dir {
                    Some(dir) => tokio::task::spawn_blocking(move || {
                        kivo_runtime::semantic::load(&dir, &language)
                    })
                    .await
                    .ok()
                    .flatten(),
                    None => None,
                };
                engine.set_semantic(semantic);
            };
            load(
                Arc::clone(&engine),
                Arc::clone(&models),
                core.config().general.language,
            )
            .await;
            loop {
                let event = match events.recv().await {
                    kivo_core::Received::Event(event) => event,
                    kivo_core::Received::Missed(_) => continue,
                    kivo_core::Received::Closed => break,
                };
                if let kivo_core::EventKind::System(kivo_core::event::SystemEvent::ModelChanged {
                    id,
                    percent: None,
                    ..
                }) = &event.kind
                    && id == kivo_store::models::EMBEDDING_MODEL
                {
                    load(
                        Arc::clone(&engine),
                        Arc::clone(&models),
                        core.config().general.language,
                    )
                    .await;
                }
            }
        });
    }
    // A program installed while KIVO runs changes PATH: look for CLIs again (DISC-15).
    let _environment = {
        let rpc = Arc::clone(&brains_rpc);
        let handle = tokio::runtime::Handle::current();
        kivo_platform_windows::environment::watch(move || {
            let rpc = Arc::clone(&rpc);
            handle.spawn(async move { rpc.environment_changed() });
        })
    };
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
        vad_model: models.vad_model(),
        turn_model: models
            .installed_dir(kivo_store::models::SMART_TURN)
            .unwrap_or_else(|| paths.models().join(kivo_store::models::SMART_TURN)),
        infer: infer.clone(),
        speaker: Arc::clone(&speaker),
        levels,
        signals,
        qos: Arc::new(kivo_platform_windows::WindowsThreadQos),
    }));
    engine.set_listener(Arc::clone(&listener));

    // Recognizing the owner's voice (VOICE §5): the model and profile load when first needed.
    let voice_id = {
        let models = Arc::clone(&models);
        Arc::new(kivo_runtime::voiceid::VoiceId::new(
            paths.voice(),
            Arc::new(kivo_platform_windows::WindowsSecrets),
            Arc::clone(&db),
            move || {
                let dir = models.installed_dir(kivo_store::models::SPEAKER_MODEL)?;
                kivo_voice::speaker::CamPlusPlus::load(&dir)
                    .map_err(|e| {
                        tracing::warn!(detail = e.detail(), "voice recognition unavailable")
                    })
                    .ok()
                    .map(|c| Arc::new(c) as Arc<dyn kivo_voice::traits::SpeakerVerifier>)
            },
        ))
    };
    engine.set_voice_id(Arc::clone(&voice_id));
    // The voice pipeline removes KIVO's own output while it plays (VOICE-30).
    engine.set_echo_cancelled(true);

    // Hands-free listening: wake words and the stop words (VOICE §4).
    let wake = kivo_runtime::wake::Wake::new(
        Arc::clone(&core),
        Arc::clone(&models),
        Arc::clone(&db),
        Arc::clone(&listener),
    );
    wake.start();
    let wake_task = tokio::spawn(Arc::clone(&wake).run());

    // What this PC can do, for the speech engines' threads (PLAN-01).
    models.set_system(Arc::clone(&platform.system));
    match platform.system.snapshot() {
        Ok(machine) => {
            let advice = models.recommend(
                &machine,
                &config,
                kivo_voice::recommend::Priority::default(),
            );
            tracing::info!(
                cpu = machine.cpu_name,
                threads = machine.logical_cpus,
                ram_mb = machine.ram_mb,
                load = machine.cpu_load_percent,
                on_battery = machine.on_battery,
                tier = ?advice.tier,
                model_threads = advice.threads,
                "hardware"
            );
            models.recommend_threads(advice.threads);
        }
        Err(e) => tracing::warn!(%e, "couldn't read the hardware"),
    }
    models.note_speech(&config);

    // Startup entry, crash reports and notification buttons (UX §1, ARCH-06, ARCH-10, UX-57).
    let lifecycle = Arc::new(Lifecycle::new(
        Arc::clone(&core),
        Arc::clone(&platform.notifications),
        Arc::clone(&platform.control),
        Arc::clone(&platform.autostart),
        recorder.clone(),
        paths.crashes(),
    ));
    if first_run {
        lifecycle.adopt_installer_startup();
    }
    lifecycle.apply_autostart(&core.config());
    lifecycle.report_crashes();
    #[cfg(windows)]
    {
        let lifecycle = Arc::clone(&lifecycle);
        std::thread::Builder::new()
            .name("kivo-toasts".into())
            .spawn(move || {
                while let Ok(answer) = toast_answers.recv() {
                    lifecycle.answered(&answer.action);
                }
            })
            .ok();
    }

    let ipc = tokio::spawn(
        server.run(
            Arc::new(
                rpc::Rpc::new(
                    Arc::clone(&core),
                    Arc::clone(&engine),
                    Arc::clone(&models),
                    recorder.clone(),
                    Arc::clone(&lifecycle),
                )
                .with_voice(Arc::new(kivo_runtime::voice_rpc::VoiceRpc::new(
                    Arc::clone(&core),
                    Arc::clone(&engine),
                    Arc::clone(&models),
                    Arc::clone(&db),
                    Arc::clone(&wake),
                    recorder.clone(),
                    Arc::new(kivo_platform_windows::WindowsSecrets),
                    paths.voice(),
                )))
                .with_brains(Arc::clone(&brains_rpc)),
            ),
            core.bus.clone(),
            core.state(),
            core.shutdown(),
        ),
    );

    // Models load when a request starts and unload after going unused (VOICE-34).
    let cooling = tokio::spawn(infer.clone().cool_down(core.shutdown()));
    let worker = tokio::spawn(infer::supervise(
        infer.clone(),
        infer_sender,
        core.shutdown(),
    ));

    // The voice pipeline and the speech worker drive the turn.
    let turns = {
        let engine = Arc::clone(&engine);
        let lifecycle = Arc::clone(&lifecycle);
        let shutdown = core.shutdown();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    signal = voice_signals.recv() => match signal {
                        Some(signal) => {
                            if signal == voice::VoiceSignal::MicrophoneUnavailable {
                                lifecycle.microphone_blocked();
                            }
                            engine::handle_signal(&engine, signal).await;
                        }
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
    let _ = wake_task.await;
    drop(wake);
    drop(listener);
    infer.shutdown().await;
    let _ = worker.await;
    let _ = cooling.await;
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
    system: Arc<dyn kivo_platform::SystemInfo>,
    autostart: Arc<dyn kivo_platform::Autostart>,
}

#[cfg(windows)]
fn windows_platform(
    paths: &Paths,
) -> (
    Platform,
    std::sync::mpsc::Receiver<kivo_platform_windows::ToastAnswer>,
) {
    use kivo_platform_windows::{
        WindowsApps, WindowsAudio, WindowsAutostart, WindowsControl, WindowsScreen,
        WindowsSystemInfo, WindowsWindows,
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
    (
        Platform {
            audio: Arc::new(WindowsAudio),
            apps: Arc::new(WindowsApps),
            windows: Arc::new(WindowsWindows),
            control: Arc::new(WindowsControl),
            screen: Arc::new(WindowsScreen),
            notifications,
            system: Arc::new(WindowsSystemInfo),
            autostart: Arc::new(WindowsAutostart::default()),
        },
        answered,
    )
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

/// Where screenshots go: the user's Pictures\Screenshots folder, like Windows itself uses.
fn screenshots_folder() -> PathBuf {
    dirs::picture_dir()
        .map(|p| p.join("Screenshots"))
        .unwrap_or_else(std::env::temp_dir)
}
