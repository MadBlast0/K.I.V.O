//! The safe test ground (TOOLS_AND_CONTROL §10): the dummy app and the fixture files under
//! `testenv/`. Computer-control tests act only on these, never on the user's own apps.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

/// The repository's `testenv/` folder.
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testenv")
        .canonicalize()
        .expect("testenv/ exists")
}

/// The dummy app's program, built by `cargo test --workspace` (or `cargo build -p
/// kivo-testenv-app`) next to the test binary.
pub fn app_exe() -> PathBuf {
    let exe = std::env::current_exe().expect("test binary path");
    // target/<profile>/deps/<test>.exe → target/<profile>/kivo-test-app.exe
    let dir = exe
        .parent()
        .and_then(Path::parent)
        .expect("target profile dir");
    let name = if cfg!(windows) {
        "kivo-test-app.exe"
    } else {
        "kivo-test-app"
    };
    let path = dir.join(name);
    assert!(
        path.exists(),
        "{} is missing: run `cargo build -p kivo-testenv-app` (cargo test --workspace builds it)",
        path.display()
    );
    path
}

/// A running dummy app, killed when dropped.
pub struct TestApp {
    child: Child,
    pub title: String,
    /// Where its Export button writes (a fresh temp folder).
    pub out: tempfile_dir::Dir,
}

impl TestApp {
    /// Starts the dummy app with a title unique to this test.
    pub fn launch(title: &str) -> Self {
        let out = tempfile_dir::Dir::new();
        let child = Command::new(app_exe())
            .arg("--title")
            .arg(title)
            .arg("--out")
            .arg(out.path())
            .spawn()
            .expect("start the dummy app");
        Self {
            child,
            title: title.to_owned(),
            out,
        }
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for TestApp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Polls `f` until it returns `Some`, for at most `timeout` (test helper only).
pub fn wait_for<T>(timeout: Duration, mut f: impl FnMut() -> Option<T>) -> Option<T> {
    let start = Instant::now();
    loop {
        if let Some(v) = f() {
            return Some(v);
        }
        if start.elapsed() > timeout {
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A temporary folder removed on drop (kept dependency-free for the testkit).
pub mod tempfile_dir {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};

    pub struct Dir(PathBuf);

    impl Dir {
        pub fn new() -> Self {
            static N: AtomicU32 = AtomicU32::new(0);
            let path = std::env::temp_dir().join(format!(
                "kivo-testenv-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).expect("temp dir");
            Self(path)
        }

        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Default for Dir {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
