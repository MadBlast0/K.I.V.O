//! The KIVO runtime: the always-on core process (ARCHITECTURE §1). It owns the state, the tray and
//! the IPC server, and launches and supervises the desktop app. Audio, voice, routing, tools and
//! permissions join it milestone by milestone.

// No console window when started by the app or at sign-in.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod args;
mod core;
#[cfg(windows)]
mod hotkeys;
mod rpc;
#[cfg(windows)]
mod tray;

use crate::app::{Launch, ProcessLauncher, Timing};
use crate::args::Args;
use crate::core::Core;
use kivo_ipc::{Server, ServerConfig, SessionToken};
use kivo_platform::Paths;
use kivo_store::{Database, Notice};
use std::process::ExitCode;
use std::sync::Arc;

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
    let core = Arc::new(Core::with_config(
        config,
        writable.then(|| paths.config_file()),
    ));
    let clients = server.connected_clients();
    let ipc = tokio::spawn(server.run(
        Arc::new(rpc::Rpc::new(Arc::clone(&core))),
        core.bus.clone(),
        core.state(),
        core.shutdown(),
    ));

    #[cfg(windows)]
    let tray = show_tray.then(|| tokio::spawn(tray::run(Arc::clone(&core))));

    #[cfg(windows)]
    let push_to_talk = tokio::spawn(hotkeys::run(
        Arc::clone(&core),
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
    let signals = {
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
    signals.abort();
    let mut code = ExitCode::SUCCESS;
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
