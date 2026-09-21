//! The app's connection to the runtime (ARCHITECTURE §1, §3). On start it connects to the
//! runtime's pipe, starting `kivo-runtime` first if it isn't running. It keeps a `Link` (status,
//! version, last state) for the UI, acts on the runtime's requests (show the window, quit), and
//! reconnects with backoff when the runtime goes away. This is the app's only way to reach KIVO.

use kivo_core::Event;
use kivo_core::event::{EventKind, SystemEvent, UiEvent};
use kivo_ipc::{
    APP_CLIENT, Client, ClientError, Link, LinkStatus, RpcError, SessionToken, StateSnapshot,
    connect, connect_with_backoff, method,
};
use kivo_platform::Paths;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tokio_util::sync::CancellationToken;

/// Webview events.
pub const LINK_EVENT: &str = "runtime://link";
/// The microphone level (0–1) for the Island's waveform.
pub const LEVEL_EVENT: &str = "runtime://level";
pub const NAVIGATE_EVENT: &str = "runtime://navigate";

/// Where the runtime listens, from the user's run folder.
struct Endpoint {
    pipe: String,
    token_file: PathBuf,
}

impl Endpoint {
    fn locate() -> Result<Self, String> {
        let paths = Paths::user().ok_or("couldn't find the user's AppData folders")?;
        let run = paths.run();
        Ok(Self {
            pipe: kivo_ipc::transport::endpoint(&run).map_err(|e| e.to_string())?,
            token_file: run.join("session.token"),
        })
    }

    fn read_token(&self) -> std::io::Result<String> {
        SessionToken::read(&self.token_file).map(|t| t.as_str().to_owned())
    }
}

#[derive(Default)]
pub struct Runtime {
    link: Mutex<Option<Link>>,
    client: Mutex<Option<Client>>,
    /// Stops the connection loop when the app exits.
    stop: CancellationToken,
}

impl Runtime {
    pub fn link(&self) -> Link {
        self.link
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .unwrap_or_else(Link::connecting)
    }

    fn set_link(&self, app: &AppHandle, update: impl FnOnce(&mut Link)) {
        let (link, previous_mode) = {
            let mut guard = self.link.lock().unwrap_or_else(|e| e.into_inner());
            let link = guard.get_or_insert_with(Link::connecting);
            let previous_mode = link.snapshot.as_ref().map(|s| s.mode);
            update(link);
            (link.clone(), previous_mode)
        };
        let snapshot = link
            .snapshot
            .as_ref()
            .filter(|_| link.status == LinkStatus::Connected);
        // While the Island is hidden for an hour, it shows nothing at all.
        let island_hidden = snapshot.is_some_and(|s| s.island_hidden);
        crate::overlay::apply(
            app,
            snapshot.map(|s| s.session).filter(|_| !island_hidden),
            snapshot.is_some_and(|s| s.turn.is_some()),
        );
        // A change of mode while connected (not the first state seen) shows the Island's notice.
        if let (Some(before), Some(now)) = (previous_mode, snapshot.map(|s| s.mode))
            && before != now
            && !island_hidden
        {
            crate::overlay::notice(app);
        }
        let _ = app.emit(LINK_EVENT, link);
    }

    fn client(&self) -> Option<Client> {
        self.client
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn set_client(&self, client: Option<Client>) {
        *self.client.lock().unwrap_or_else(|e| e.into_inner()) = client;
    }

    pub fn connected(&self) -> bool {
        self.link().status == LinkStatus::Connected
    }

    /// Sends a request to the runtime; the error text is shown to the user.
    pub async fn request(&self, name: &str, params: Value) -> Result<Value, String> {
        let client = self
            .client()
            .ok_or("KIVO isn't running. Start it and try again.")?;
        client.request(name, params).await.map_err(|e| match e {
            ClientError::Rejected(RpcError { message, .. }) => message,
            other => other.to_string(),
        })
    }

    pub fn stop(&self) {
        self.stop.cancel();
    }
}

/// Starts `kivo-runtime` from the app's own folder (the installer puts it there; so does a
/// development build). A second runtime exits at once, so starting one too many is harmless.
pub fn start_runtime() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or("the app has no folder")?;
    let runtime = ["kivo-runtime.exe", "kivo-runtime"]
        .into_iter()
        .map(|name| dir.join(name))
        .find(|p| p.is_file())
        .ok_or("kivo-runtime was not found next to KIVO; reinstall KIVO")?;
    let mut command = std::process::Command::new(runtime);
    command.arg("--from-app");
    // A development app lives only as long as `pnpm dev`; the runtime must not relaunch it after
    // the dev server stops. (Run `cargo run -p kivo-runtime` to try supervision.)
    if cfg!(debug_assertions) {
        command.arg("--no-app");
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Its own hidden console: no window, and Ctrl+C in a terminal that ran the app doesn't
        // reach it.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.spawn().map(drop).map_err(|e| e.to_string())
}

/// Keeps the app connected to the runtime until the app exits.
pub async fn maintain(app: AppHandle) {
    let runtime = app.state::<Runtime>();
    let endpoint = match Endpoint::locate() {
        Ok(endpoint) => endpoint,
        Err(message) => {
            runtime.set_link(&app, |l| l.message = Some(message));
            return;
        }
    };

    // Not running yet (a manual launch): start it. (If the runtime launched this app, the first
    // try succeeds.)
    let mut first = match endpoint.read_token() {
        Ok(token) => connect(&endpoint.pipe, &token, APP_CLIENT).await.ok(),
        Err(_) => None,
    };
    if first.is_none()
        && let Err(message) = start_runtime()
    {
        runtime.set_link(&app, |l| l.message = Some(message));
    }

    loop {
        let connection = match first.take() {
            Some(connection) => Ok(connection),
            None => {
                connect_with_backoff(
                    &endpoint.pipe,
                    || endpoint.read_token(),
                    APP_CLIENT,
                    &runtime.stop,
                )
                .await
            }
        };
        let mut connection = match connection {
            Ok(connection) => connection,
            Err(ClientError::Cancelled) => return,
            Err(ClientError::Rejected(e)) => {
                runtime.set_link(&app, |l| {
                    l.status = LinkStatus::Incompatible;
                    l.message = Some(e.message);
                });
                return;
            }
            Err(e) => {
                runtime.set_link(&app, |l| l.message = Some(e.to_string()));
                return;
            }
        };

        runtime.set_client(Some(connection.client.clone()));
        let welcome = connection.welcome;
        runtime.set_link(&app, |l| {
            *l = Link {
                status: LinkStatus::Connected,
                runtime_version: Some(welcome.runtime_version),
                snapshot: Some(welcome.snapshot),
                message: None,
            };
        });

        loop {
            let note = tokio::select! {
                note = connection.notifications.recv() => note,
                () = runtime.stop.cancelled() => return,
            };
            let Some(note) = note else { break };
            match note.method.as_str() {
                method::EVENT => match serde_json::from_value::<Event>(note.params) {
                    Ok(event) => on_event(&app, &event),
                    Err(e) => eprintln!("KIVO: unreadable event from the runtime: {e}"),
                },
                method::SNAPSHOT => match serde_json::from_value::<StateSnapshot>(note.params) {
                    Ok(snapshot) => runtime.set_link(&app, |l| l.snapshot = Some(snapshot)),
                    Err(e) => eprintln!("KIVO: unreadable state from the runtime: {e}"),
                },
                // The mic level goes to the Island only (its waveform), ~30 times a second.
                method::LEVELS => {
                    if let Some(level) = note.params.get("level").and_then(Value::as_f64) {
                        let _ = app.emit_to(crate::overlay::LABEL, LEVEL_EVENT, level);
                    }
                }
                _ => {}
            }
        }

        // The runtime went away (crashed, or was killed). Keep the last state on screen.
        runtime.set_client(None);
        runtime.set_link(&app, |l| {
            l.status = LinkStatus::Reconnecting;
            l.message = None;
        });
    }
}

/// Runtime events the app acts on.
fn on_event(app: &AppHandle, event: &Event) {
    match &event.kind {
        // Ctrl+Shift+Space asks for the Island's text field rather than the Control Center.
        EventKind::Ui(UiEvent::ControlCenterRequested { page })
            if page.as_deref() == Some("type") =>
        {
            crate::overlay::start_typing(app);
        }
        EventKind::Ui(UiEvent::ControlCenterRequested { page }) => {
            crate::show_main_window(app, page.as_deref());
        }
        EventKind::System(SystemEvent::ShuttingDown) => {
            // KIVO is quitting (tray or Control Center): the app goes with it.
            app.state::<Runtime>().stop();
            app.exit(0);
        }
        _ => {}
    }
}
