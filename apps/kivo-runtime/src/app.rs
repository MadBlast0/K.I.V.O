//! Launching and supervising the desktop app (ARCHITECTURE §1, ARCH-03). The app is expected to
//! be running for as long as KIVO is on: closing its window only hides it, so if its connection
//! drops and doesn't come back, it crashed or was killed, and it is relaunched in the background
//! with backoff. After repeated quick crashes the supervisor stops trying until the user asks for
//! the Control Center (tray click, "Open KIVO").
//!
//! In low-memory mode (Settings → General, ARCH-08) the app isn't started in the background at all:
//! the supervisor waits until the Island first has something to show (KIVO hears the wake word,
//! push-to-talk, a live activity, an offer) or the Control Center is asked for, and launches it
//! then. That trades the first show's latency for the app's idle memory.

use crate::core::Core;
use kivo_core::Received;
use kivo_core::SessionState;
use kivo_core::event::{EventKind, UiEvent};
use kivo_ipc::APP_CLIENT;
use kivo_ipc::protocol::StateSnapshot;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio::time::Instant;

/// How the app is launched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Launch {
    /// Show the Control Center, optionally on a page.
    Show(Option<String>),
    /// Start hidden, with the overlay preloaded (autostart, relaunch after a crash).
    Background,
}

impl Launch {
    pub fn args(&self) -> Vec<String> {
        match self {
            Self::Show(None) => Vec::new(),
            Self::Show(Some(page)) => vec!["--page".into(), page.clone()],
            Self::Background => vec!["--background".into()],
        }
    }
}

/// Timings, adjustable for tests.
#[derive(Clone, Copy, Debug)]
pub struct Timing {
    /// How long the app may be disconnected before it is considered gone (it reconnects on its
    /// own after a blip).
    pub grace: Duration,
    /// How long a launched app has to connect.
    pub connect_timeout: Duration,
    /// An app that disconnects sooner than this after launch crashed quickly.
    pub stable_after: Duration,
    /// Quick crashes in a row before the supervisor gives up.
    pub max_quick_crashes: u32,
    pub first_backoff: Duration,
    pub max_backoff: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            grace: Duration::from_secs(3),
            connect_timeout: Duration::from_secs(30),
            stable_after: Duration::from_secs(60),
            max_quick_crashes: 5,
            first_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(30),
        }
    }
}

/// Starts the app process. Returns false if it could not be started.
pub trait Launcher: Send + Sync + 'static {
    fn launch(&self, how: &Launch) -> bool;
}

/// Launches `KIVO.exe` (installed) or `kivo-app.exe` (a development build) from the runtime's
/// own folder, where the installer puts both.
pub struct ProcessLauncher {
    exe: Option<PathBuf>,
}

impl ProcessLauncher {
    pub fn beside_runtime() -> Self {
        let dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(PathBuf::from));
        let exe = dir.and_then(|dir| {
            ["KIVO.exe", "kivo-app.exe", "kivo-app"]
                .into_iter()
                .map(|name| dir.join(name))
                .find(|p| p.is_file())
        });
        if exe.is_none() {
            tracing::warn!("the KIVO app was not found next to the runtime; it can't be launched");
        }
        Self { exe }
    }
}

impl Launcher for ProcessLauncher {
    fn launch(&self, how: &Launch) -> bool {
        let Some(exe) = &self.exe else { return false };
        match tokio::process::Command::new(exe).args(how.args()).spawn() {
            Ok(mut child) => {
                tracing::info!(?how, pid = child.id(), "launched the app");
                // Reap it and record how it ended; supervision itself follows the IPC connection.
                tokio::spawn(async move {
                    if let Ok(status) = child.wait().await {
                        tracing::info!(%status, "the app exited");
                    }
                });
                true
            }
            Err(e) => {
                tracing::error!(%e, exe = %exe.display(), "couldn't launch the app");
                false
            }
        }
    }
}

fn app_connected(clients: &[String]) -> bool {
    clients.iter().any(|c| c == APP_CLIENT)
}

/// Supervises the app until KIVO quits. `initial` is launched first (`None` when the app started
/// the runtime and is already running). `clients` is the IPC server's connected-client list.
pub async fn supervise(
    core: Arc<Core>,
    launcher: Arc<dyn Launcher>,
    initial: Option<Launch>,
    mut clients: watch::Receiver<Vec<String>>,
    timing: Timing,
) {
    let shutdown = core.shutdown();
    let mut quick_crashes = 0_u32;
    let mut launched_at: Option<Instant> = None;
    // With no initial launch, the app started the runtime and is connecting now.
    let mut awaiting = initial.is_none();
    let mut next: Option<Launch> = initial;

    loop {
        // 1. Launch if needed, then wait for the app to connect. In low-memory mode a background
        // launch waits until the Island is needed.
        if let Some(mut how) = next.take() {
            if how == Launch::Background && core.config().general.low_memory_mode {
                tracing::info!("low-memory mode: the app starts when the Island is first needed");
                let mut requests = core.bus.subscribe();
                let Some(needed) = wait_for_need(&core, &mut requests).await else {
                    return;
                };
                how = needed;
            }
            awaiting = launcher.launch(&how);
        }
        if std::mem::take(&mut awaiting) {
            launched_at = Some(Instant::now());
            let connected = tokio::select! {
                r = tokio::time::timeout(timing.connect_timeout, clients.wait_for(|c| app_connected(c))) => matches!(r, Ok(Ok(_))),
                () = shutdown.cancelled() => return,
            };
            if !connected {
                tracing::warn!("the app didn't connect in time");
            }
        }

        // 2. While the app is up, nothing to do; Control Center requests go to it directly.
        tokio::select! {
            r = clients.wait_for(|c| !app_connected(c)) => if r.is_err() { return },
            () = shutdown.cancelled() => return,
        }
        // From here on, a Control Center request means the user wants the app back now. (While
        // it was up, requests were the app's to handle, so older ones don't count.)
        let mut requests = core.bus.subscribe();

        // 3. It's gone. Give it a moment to come back on its own.
        let back = tokio::select! {
            r = tokio::time::timeout(timing.grace, clients.wait_for(|c| app_connected(c))) => matches!(r, Ok(Ok(_))),
            () = shutdown.cancelled() => return,
        };
        if back {
            continue;
        }

        // 4. Relaunch it, with backoff after quick crashes; give up after too many.
        let quick = launched_at.is_some_and(|t| t.elapsed() < timing.stable_after);
        quick_crashes = if quick { quick_crashes + 1 } else { 0 };
        if quick_crashes >= timing.max_quick_crashes {
            tracing::error!(
                quick_crashes,
                "the app keeps crashing; not relaunching it until the Control Center is requested"
            );
            let Some(page) = wait_for_open(&core, &mut requests).await else {
                return;
            };
            quick_crashes = 0;
            next = Some(Launch::Show(page));
            continue;
        }
        let delay = if quick_crashes == 0 {
            Duration::ZERO
        } else {
            (timing.first_backoff * 2_u32.pow(quick_crashes - 1)).min(timing.max_backoff)
        };
        tracing::warn!(
            ?delay,
            quick_crashes,
            "the app disconnected; relaunching it"
        );
        // A Control Center request during the wait launches it right away, shown.
        next = Some(tokio::select! {
            () = tokio::time::sleep(delay) => Launch::Background,
            page = wait_for_open(&core, &mut requests) => match page {
                Some(page) => Launch::Show(page),
                None => return,
            },
        });
    }
}

/// Whether the Island has something to show: KIVO is listening or busy, or a live activity, an
/// offer or a selection is waiting (low-memory mode launches the app for it).
pub fn needs_overlay(state: &StateSnapshot) -> bool {
    !matches!(state.session, SessionState::Idle | SessionState::Paused)
        || !state.activities.is_empty()
        || state.offer.is_some()
        || state.has_selection
}

/// Waits until the app is needed: a Control Center request shows it, the Island having something
/// to show starts it in the background. `None` if KIVO is quitting.
async fn wait_for_need(core: &Core, requests: &mut kivo_core::Subscription) -> Option<Launch> {
    let mut state = core.state();
    let shutdown = core.shutdown();
    tokio::select! {
        page = wait_for_open(core, requests) => page.map(Launch::Show),
        r = state.wait_for(needs_overlay) => r.ok().map(|_| Launch::Background),
        () = shutdown.cancelled() => None,
    }
}

/// Waits for a Control Center request (tray click, "Open KIVO", "Settings"). `None` if KIVO is
/// quitting.
async fn wait_for_open(
    core: &Core,
    requests: &mut kivo_core::Subscription,
) -> Option<Option<String>> {
    let shutdown = core.shutdown();
    loop {
        let received = tokio::select! {
            r = requests.recv() => r,
            () = shutdown.cancelled() => return None,
        };
        match received {
            Received::Event(event) => {
                if let EventKind::Ui(UiEvent::ControlCenterRequested { page }) = &event.kind {
                    return Some(page.clone());
                }
            }
            Received::Missed(_) => {}
            Received::Closed => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Records launches and, like a real app, connects (or crashes) right after.
    struct FakeApp {
        launches: Mutex<Vec<Launch>>,
        clients: watch::Sender<Vec<String>>,
        /// Whether a launched app connects.
        works: Mutex<bool>,
    }

    impl Launcher for FakeApp {
        fn launch(&self, how: &Launch) -> bool {
            self.launches.lock().unwrap().push(how.clone());
            if *self.works.lock().unwrap() {
                self.clients.send_modify(|c| c.push(APP_CLIENT.into()));
            }
            true
        }
    }

    impl FakeApp {
        fn crash(&self) {
            self.clients.send_modify(|c| c.retain(|n| n != APP_CLIENT));
        }
        fn launches(&self) -> Vec<Launch> {
            self.launches.lock().unwrap().clone()
        }
    }

    fn fast() -> Timing {
        Timing {
            grace: Duration::from_millis(100),
            connect_timeout: Duration::from_millis(300),
            stable_after: Duration::from_secs(60),
            max_quick_crashes: 3,
            first_backoff: Duration::from_millis(20),
            max_backoff: Duration::from_millis(80),
        }
    }

    fn setup(initial: Option<Launch>) -> (Arc<Core>, Arc<FakeApp>, tokio::task::JoinHandle<()>) {
        let core = Arc::new(Core::default());
        let (clients, rx) = watch::channel(Vec::new());
        let app = Arc::new(FakeApp {
            launches: Mutex::new(Vec::new()),
            clients,
            works: Mutex::new(true),
        });
        let task = tokio::spawn(supervise(
            Arc::clone(&core),
            Arc::clone(&app) as Arc<dyn Launcher>,
            initial,
            rx,
            fast(),
        ));
        (core, app, task)
    }

    async fn settle() {
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    /// ARCH-08: in low-memory mode the app isn't started at sign-in or after a crash; it starts
    /// when the Island is first needed, or shown when the Control Center is asked for.
    #[tokio::test]
    async fn low_memory_mode_starts_the_app_only_when_the_island_is_needed() {
        let core = Arc::new(Core::default());
        core.update_config(|c| c.general.low_memory_mode = true);
        let (clients, rx) = watch::channel(Vec::new());
        let app = Arc::new(FakeApp {
            launches: Mutex::new(Vec::new()),
            clients,
            works: Mutex::new(true),
        });
        let task = tokio::spawn(supervise(
            Arc::clone(&core),
            Arc::clone(&app) as Arc<dyn Launcher>,
            Some(Launch::Background),
            rx,
            fast(),
        ));
        settle().await;
        assert!(app.launches().is_empty(), "not at sign-in");

        // "Hey Kivo": the Island is needed, so the app starts (in the background; it shows the
        // Island from the state it reads on connecting).
        core.advance(kivo_core::SessionInput::Activate);
        settle().await;
        assert_eq!(app.launches(), [Launch::Background]);

        // A crash while idle: nothing is relaunched until it's needed again.
        core.advance(kivo_core::SessionInput::Cancel);
        core.advance(kivo_core::SessionInput::InterruptionHandled { listen: false });
        settle().await;
        app.crash();
        settle().await;
        assert_eq!(app.launches().len(), 1);
        // The tray's "Open KIVO" shows it at once.
        core.open_control_center(Some("settings"));
        settle().await;
        assert_eq!(
            app.launches(),
            [Launch::Background, Launch::Show(Some("settings".into()))]
        );
        core.quit();
        task.await.unwrap();
    }

    #[test]
    fn the_island_is_needed_while_kivo_listens_or_something_waits() {
        let mut state = Core::default().state().borrow().clone();
        assert!(!needs_overlay(&state));
        state.session = SessionState::Paused;
        assert!(!needs_overlay(&state));
        state.session = SessionState::Listening;
        assert!(needs_overlay(&state));
        state.session = SessionState::Idle;
        state.has_selection = true;
        assert!(needs_overlay(&state));
    }

    #[tokio::test]
    async fn the_app_is_launched_first_and_relaunched_in_the_background_after_a_crash() {
        let (core, app, task) = setup(Some(Launch::Show(None)));
        settle().await;
        assert_eq!(app.launches(), [Launch::Show(None)]);
        app.crash();
        settle().await;
        assert_eq!(app.launches(), [Launch::Show(None), Launch::Background]);
        core.quit();
        task.await.unwrap();
    }

    #[tokio::test]
    async fn a_brief_disconnect_is_not_a_crash() {
        let (core, app, task) = setup(Some(Launch::Background));
        settle().await;
        app.crash();
        app.clients.send_modify(|c| c.push(APP_CLIENT.into())); // reconnected within the grace
        settle().await;
        assert_eq!(app.launches(), [Launch::Background]);
        core.quit();
        task.await.unwrap();
    }

    #[tokio::test]
    async fn an_app_that_started_the_runtime_is_supervised_but_not_launched() {
        let (core, app, task) = setup(None);
        app.clients.send_modify(|c| c.push(APP_CLIENT.into()));
        settle().await;
        assert!(app.launches().is_empty());
        app.crash();
        settle().await;
        assert_eq!(app.launches(), [Launch::Background]);
        core.quit();
        task.await.unwrap();
    }

    #[tokio::test]
    async fn repeated_quick_crashes_stop_relaunching_until_the_user_asks() {
        let (core, app, task) = setup(Some(Launch::Background));
        for _ in 0..4 {
            settle().await;
            app.crash();
        }
        settle().await;
        let before = app.launches().len();
        assert_eq!(
            before, 3,
            "the initial launch and two relaunches, then it gives up"
        );
        settle().await;
        assert_eq!(
            app.launches().len(),
            before,
            "no more relaunches on its own"
        );

        core.open_control_center(Some("settings"));
        settle().await;
        assert_eq!(
            app.launches().last(),
            Some(&Launch::Show(Some("settings".into())))
        );
        core.quit();
        task.await.unwrap();
    }

    #[tokio::test]
    async fn an_app_that_never_connects_counts_as_a_quick_crash() {
        let (core, app, task) = setup(None);
        *app.works.lock().unwrap() = false;
        tokio::time::sleep(Duration::from_millis(1800)).await;
        assert_eq!(
            app.launches(),
            [Launch::Background, Launch::Background],
            "retried with backoff, then given up (the app that started KIVO was the first try)"
        );
        core.quit();
        task.await.unwrap();
    }

    #[test]
    fn launch_modes_map_to_app_arguments() {
        assert!(Launch::Show(None).args().is_empty());
        assert_eq!(
            Launch::Show(Some("settings".into())).args(),
            ["--page", "settings"]
        );
        assert_eq!(Launch::Background.args(), ["--background"]);
    }
}
