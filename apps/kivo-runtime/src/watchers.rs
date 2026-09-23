//! Watchers (TOOL-28, TOOL-29): each waits for one event from the system — a moment, a process
//! exiting, a change in a folder, a download finishing, a window opening or closing — with no brain
//! call and no busy polling while it waits. Cancelling a wait (the task was cancelled, paused or
//! KIVO is quitting) ends it at once.

use kivo_core::task::{WatchSpec, WindowEvent};
use kivo_core::text;
use kivo_platform::{Processes, UiAutomation, UiEventKind, UiSubscription, Windows};
use notify::{EventKind, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// A download counts as finished once its file has been quiet this long.
const DOWNLOAD_SETTLED: Duration = Duration::from_secs(2);
/// A time watcher that fires this late was missed (KIVO was off).
const LATE: i64 = 60_000;

/// Files browsers and download managers write while a download is still running.
const PARTIAL: &[&str] = &[
    "crdownload",
    "part",
    "partial",
    "tmp",
    "download",
    "opdownload",
];

/// What happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fired {
    /// In the user's words: "cargo finished", "report.pdf finished downloading".
    pub what: String,
    /// A time watcher fired late because KIVO was off at the time (reported, TOOL-28).
    pub late: bool,
    /// A process's exit code, when it ended.
    pub exit_code: Option<i32>,
}

pub struct Watchers {
    processes: Arc<dyn Processes>,
    windows: Arc<dyn Windows>,
    uia: Arc<dyn UiAutomation>,
    downloads: PathBuf,
}

impl Watchers {
    pub fn new(
        processes: Arc<dyn Processes>,
        windows: Arc<dyn Windows>,
        uia: Arc<dyn UiAutomation>,
        downloads: PathBuf,
    ) -> Self {
        Self {
            processes,
            windows,
            uia,
            downloads,
        }
    }

    /// Waits for `spec`; `Err` in plain words when it can't be watched.
    pub async fn wait(
        &self,
        spec: &WatchSpec,
        cancel: &CancellationToken,
    ) -> Result<Fired, String> {
        match spec {
            WatchSpec::Time { at_ms } => wait_until(*at_ms, cancel).await,
            WatchSpec::ProcessExit { pid, name } => {
                self.wait_processes(*pid, name.as_deref(), cancel).await
            }
            WatchSpec::Folder { path, pattern } => {
                wait_folder(Path::new(path), pattern.as_deref(), cancel).await
            }
            WatchSpec::Download { folder } => {
                let folder = folder
                    .as_deref()
                    .map_or_else(|| self.downloads.clone(), PathBuf::from);
                wait_download(&folder, cancel).await
            }
            WatchSpec::Window { app, title, event } => {
                self.wait_window(app.as_deref(), title.as_deref(), *event, cancel)
                    .await
            }
        }
    }

    /// The running processes whose program is `name` (`cargo` or `cargo.exe`).
    pub fn find(&self, name: &str) -> Vec<u32> {
        let wanted = program_name(name);
        self.processes
            .list()
            .unwrap_or_default()
            .into_iter()
            .filter(|p| program_name(&p.name) == wanted)
            .map(|p| p.pid)
            .collect()
    }

    /// The first known build or test process running now ("tell me when the build finishes").
    pub fn running_build(&self) -> Option<(u32, String)> {
        const BUILDS: &[&str] = &[
            "cargo",
            "msbuild",
            "dotnet",
            "gradle",
            "gradlew",
            "mvn",
            "make",
            "ninja",
            "cmake",
            "pnpm",
            "npm",
            "yarn",
            "bun",
            "go",
            "tsc",
            "vite",
            "webpack",
            "pytest",
            "bazel",
            "devenv",
            "xcodebuild",
        ];
        let list = self.processes.list().unwrap_or_default();
        BUILDS.iter().find_map(|b| {
            list.iter()
                .find(|p| program_name(&p.name) == *b)
                .map(|p| (p.pid, program_name(&p.name)))
        })
    }

    async fn wait_processes(
        &self,
        pid: Option<u32>,
        name: Option<&str>,
        cancel: &CancellationToken,
    ) -> Result<Fired, String> {
        let (pids, label) = match (pid, name) {
            (Some(pid), name) => (
                vec![pid],
                name.map_or_else(|| pid.to_string(), program_name),
            ),
            (None, Some(name)) => (self.find(name), program_name(name)),
            (None, None) => return Err(text::t("watch.nothingToWatch")),
        };
        if pids.is_empty() {
            return Err(text::tf("watch.notRunning", &[("name", &label)]));
        }
        let mut last_code = None;
        for pid in pids {
            let wait = match self.processes.watch(pid) {
                Ok(w) => Arc::new(w),
                // It ended between the list and the watch: that one is done.
                Err(_) => continue,
            };
            let waiter = Arc::clone(&wait);
            let blocking = tokio::task::spawn_blocking(move || waiter.wait());
            tokio::pin!(blocking);
            tokio::select! {
                done = &mut blocking => last_code = done.ok().flatten().or(last_code),
                () = cancel.cancelled() => {
                    wait.cancel();
                    let _ = blocking.await;
                    return Err(text::t("reply.cancelled"));
                }
            }
        }
        let what = match last_code {
            Some(0) | None => text::tf("watch.processDone", &[("name", &label)]),
            Some(code) => text::tf(
                "watch.processFailed",
                &[("name", &label), ("code", &code.to_string())],
            ),
        };
        Ok(Fired {
            what,
            late: false,
            exit_code: last_code,
        })
    }

    async fn wait_window(
        &self,
        app: Option<&str>,
        title: Option<&str>,
        event: WindowEvent,
        cancel: &CancellationToken,
    ) -> Result<Fired, String> {
        if app.is_none() && title.is_none() {
            return Err(text::t("watch.nothingToWatch"));
        }
        let label = title.or(app).unwrap_or_default().to_owned();
        let open = || {
            self.windows.list().unwrap_or_default().iter().any(|w| {
                app.is_none_or(|a| {
                    let a = program_name(a);
                    program_name(&w.app_id) == a || w.title.to_lowercase().contains(&a)
                }) && title.is_none_or(|t| w.title.to_lowercase().contains(&t.to_lowercase()))
            })
        };
        let wanted = |is_open: bool| match event {
            WindowEvent::Opened => is_open,
            WindowEvent::Closed => !is_open,
        };
        let done = || {
            let key = match event {
                WindowEvent::Opened => "watch.windowOpened",
                WindowEvent::Closed => "watch.windowClosed",
            };
            Ok(Fired {
                what: text::tf(key, &[("name", &label)]),
                late: false,
                exit_code: None,
            })
        };
        if wanted(open()) {
            return done();
        }
        // Every window opening or closing wakes the check; nothing runs in between.
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut subscriptions = Vec::new();
        for kind in [UiEventKind::WindowOpened, UiEventKind::WindowClosed] {
            let tx = tx.clone();
            match self.uia.subscribe(
                &UiSubscription { kind, window: None },
                Box::new(move |_| {
                    let _ = tx.send(());
                }),
            ) {
                Ok(id) => subscriptions.push(id),
                Err(e) => tracing::debug!(%e, "window events unavailable"),
            }
        }
        let result = loop {
            tokio::select! {
                got = rx.recv() => {
                    if got.is_none() {
                        break Err(text::t("watch.nothingToWatch"));
                    }
                    // A closing window disappears from the list a moment after its event.
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    if wanted(open()) {
                        break done();
                    }
                }
                () = cancel.cancelled() => break Err(text::t("reply.cancelled")),
            }
        };
        for id in subscriptions {
            let _ = self.uia.unsubscribe(id);
        }
        result
    }
}

/// `C:\Tools\Cargo.EXE` → `cargo`.
pub fn program_name(name: &str) -> String {
    let file = name
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(name)
        .to_lowercase();
    file.strip_suffix(".exe").map(str::to_owned).unwrap_or(file)
}

fn now_ms() -> i64 {
    kivo_store::brains::now_ms()
}

async fn wait_until(at_ms: i64, cancel: &CancellationToken) -> Result<Fired, String> {
    let now = now_ms();
    let late = now - at_ms > LATE;
    if at_ms > now {
        let wait = Duration::from_millis(u64::try_from(at_ms - now).unwrap_or(0));
        tokio::select! {
            () = tokio::time::sleep(wait) => {}
            () = cancel.cancelled() => return Err(text::t("reply.cancelled")),
        }
    }
    Ok(Fired {
        what: text::t("watch.timeUp"),
        late,
        exit_code: None,
    })
}

/// A notify watcher delivering into a tokio channel.
fn watch_dir(
    dir: &Path,
) -> Result<
    (
        notify::RecommendedWatcher,
        tokio::sync::mpsc::UnboundedReceiver<notify::Event>,
    ),
    String,
> {
    if !dir.is_dir() {
        return Err(text::tf(
            "watch.noFolder",
            &[("path", &dir.display().to_string())],
        ));
    }
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if let Ok(event) = event {
            let _ = tx.send(event);
        }
    })
    .map_err(|e| e.to_string())?;
    watcher
        .watch(dir, RecursiveMode::NonRecursive)
        .map_err(|e| e.to_string())?;
    Ok((watcher, rx))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

async fn wait_folder(
    dir: &Path,
    pattern: Option<&str>,
    cancel: &CancellationToken,
) -> Result<Fired, String> {
    let (_watcher, mut events) = watch_dir(dir)?;
    loop {
        tokio::select! {
            event = events.recv() => {
                let Some(event) = event else { return Err(text::t("watch.nothingToWatch")) };
                if matches!(event.kind, EventKind::Access(_) | EventKind::Other | EventKind::Any) {
                    continue;
                }
                let hit = event.paths.iter().find(|p| {
                    pattern.is_none_or(|pat| kivo_security::glob(pat, &file_name(p)))
                });
                if let Some(path) = hit {
                    return Ok(Fired {
                        what: text::tf("watch.folderChanged", &[("name", &file_name(path))]),
                        late: false,
                        exit_code: None,
                    });
                }
            }
            () = cancel.cancelled() => return Err(text::t("reply.cancelled")),
        }
    }
}

fn is_partial(path: &Path) -> bool {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .is_some_and(|e| PARTIAL.contains(&e.as_str()))
}

async fn wait_download(dir: &Path, cancel: &CancellationToken) -> Result<Fired, String> {
    let (_watcher, mut events) = watch_dir(dir)?;
    // The finished file, waiting to be quiet.
    let mut candidate: Option<PathBuf> = None;
    loop {
        let settle = async {
            match &candidate {
                Some(_) => tokio::time::sleep(DOWNLOAD_SETTLED).await,
                None => std::future::pending::<()>().await,
            }
        };
        tokio::select! {
            event = events.recv() => {
                let Some(event) = event else { return Err(text::t("watch.nothingToWatch")) };
                if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                    continue;
                }
                // The newest path in the event (a rename reports old and new).
                if let Some(path) = event.paths.last() {
                    if is_partial(path) {
                        // Still downloading: whatever was settling isn't the end yet.
                        candidate = None;
                    } else if path.is_file() {
                        candidate = Some(path.clone());
                    }
                }
            }
            () = settle => {
                if let Some(path) = candidate.take()
                    && path.is_file()
                {
                    return Ok(Fired {
                        what: text::tf("watch.downloadDone", &[("name", &file_name(&path))]),
                        late: false,
                        exit_code: None,
                    });
                }
            }
            () = cancel.cancelled() => return Err(text::t("reply.cancelled")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kivo_testkit::{FakeProcesses, FakeUia, FakeWindows};

    fn watchers(processes: Arc<FakeProcesses>, downloads: PathBuf) -> Watchers {
        Watchers::new(
            processes,
            Arc::new(FakeWindows::default()),
            Arc::new(FakeUia::default()),
            downloads,
        )
    }

    #[tokio::test]
    async fn a_build_is_found_and_its_end_is_heard() {
        let processes = Arc::new(FakeProcesses::default());
        processes.start(10, "explorer.exe");
        processes.start(42, "cargo.exe");
        let w = Arc::new(watchers(Arc::clone(&processes), std::env::temp_dir()));
        assert_eq!(w.running_build(), Some((42, "cargo".into())));
        let cancel = CancellationToken::new();
        let waiting = {
            let w = Arc::clone(&w);
            let cancel = cancel.clone();
            tokio::spawn(async move {
                w.wait(
                    &WatchSpec::ProcessExit {
                        pid: None,
                        name: Some("cargo".into()),
                    },
                    &cancel,
                )
                .await
            })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!waiting.is_finished(), "waits until the process ends");
        processes.exit(42, 101);
        let fired = waiting.await.unwrap().unwrap();
        assert_eq!(fired.exit_code, Some(101));
        assert!(fired.what.contains("cargo"), "{}", fired.what);
    }

    #[tokio::test]
    async fn waits_end_when_cancelled_and_missing_things_say_so() {
        let processes = Arc::new(FakeProcesses::default());
        processes.start(7, "msbuild.exe");
        let w = Arc::new(watchers(Arc::clone(&processes), std::env::temp_dir()));
        let cancel = CancellationToken::new();
        let waiting = {
            let (w, cancel) = (Arc::clone(&w), cancel.clone());
            tokio::spawn(async move {
                w.wait(
                    &WatchSpec::ProcessExit {
                        pid: Some(7),
                        name: None,
                    },
                    &cancel,
                )
                .await
            })
        };
        tokio::time::sleep(Duration::from_millis(30)).await;
        cancel.cancel();
        assert!(
            tokio::time::timeout(Duration::from_secs(2), waiting)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
        let e = w
            .wait(
                &WatchSpec::ProcessExit {
                    pid: None,
                    name: Some("gradle".into()),
                },
                &CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert!(e.contains("gradle"), "{e}");
    }

    #[tokio::test]
    async fn a_time_in_the_past_fires_late() {
        let fired = wait_until(now_ms() - 3_600_000, &CancellationToken::new())
            .await
            .unwrap();
        assert!(fired.late);
        let soon = wait_until(now_ms() + 30, &CancellationToken::new())
            .await
            .unwrap();
        assert!(!soon.late);
    }

    #[tokio::test]
    async fn folder_changes_and_finished_downloads_are_heard() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let cancel = CancellationToken::new();
        let folder = {
            let (path, cancel) = (path.clone(), cancel.clone());
            tokio::spawn(async move { wait_folder(&path, Some("*.txt"), &cancel).await })
        };
        tokio::time::sleep(Duration::from_millis(200)).await;
        std::fs::write(path.join("ignored.bin"), b"x").unwrap();
        std::fs::write(path.join("notes.txt"), b"x").unwrap();
        let fired = tokio::time::timeout(Duration::from_secs(5), folder)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(fired.what.contains("notes.txt"), "{}", fired.what);

        // A download: the partial file first, then the finished one, which settles.
        let download = {
            let (path, cancel) = (path.clone(), cancel.clone());
            tokio::spawn(async move { wait_download(&path, &cancel).await })
        };
        tokio::time::sleep(Duration::from_millis(200)).await;
        std::fs::write(path.join("report.pdf.crdownload"), b"part").unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!download.is_finished());
        std::fs::rename(path.join("report.pdf.crdownload"), path.join("report.pdf")).unwrap();
        let fired = tokio::time::timeout(Duration::from_secs(10), download)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(fired.what.contains("report.pdf"), "{}", fired.what);
    }

    #[test]
    fn program_names_compare_without_path_case_or_exe() {
        assert_eq!(program_name(r"C:\Tools\Cargo.EXE"), "cargo");
        assert_eq!(program_name("msbuild"), "msbuild");
    }
}
