//! Where KIVO keeps its files (ARCHITECTURE §5).

use std::path::{Path, PathBuf};

/// All of KIVO's on-disk locations. Roaming data (settings) and local data (database, logs,
/// models) live apart, like other Windows apps: settings can roam, big local data doesn't.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    /// `%APPDATA%\KIVO` (roaming).
    pub roaming: PathBuf,
    /// `%LOCALAPPDATA%\KIVO`.
    pub local: PathBuf,
}

impl Paths {
    /// The standard per-user locations, or `None` if the OS doesn't report them.
    pub fn user() -> Option<Self> {
        Some(Self {
            roaming: dirs::config_dir()?.join("KIVO"),
            local: dirs::data_local_dir()?.join("KIVO"),
        })
    }

    /// Everything under one folder (tests, portable use).
    pub fn under(root: &Path) -> Self {
        Self {
            roaming: root.join("roaming"),
            local: root.join("local"),
        }
    }

    pub fn config_file(&self) -> PathBuf {
        self.roaming.join("config").join("kivo.toml")
    }

    pub fn database(&self) -> PathBuf {
        self.local.join("data").join("kivo.db")
    }

    pub fn logs(&self) -> PathBuf {
        self.local.join("logs")
    }

    pub fn crashes(&self) -> PathBuf {
        self.local.join("crashes")
    }

    pub fn models(&self) -> PathBuf {
        self.local.join("models")
    }

    /// The runtime's session token and other per-run files (ARCHITECTURE §3).
    pub fn run(&self) -> PathBuf {
        self.local.join("run")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_matches_the_spec() {
        let p = Paths::under(Path::new("x"));
        assert!(p.config_file().ends_with("roaming/config/kivo.toml"));
        assert!(p.database().ends_with("local/data/kivo.db"));
        assert!(p.logs().ends_with("local/logs"));
        assert!(p.run().ends_with("local/run"));
    }

    #[test]
    fn user_paths_are_named_kivo_and_files_never_collide() {
        let p = Paths::user().expect("per-user folders exist");
        assert!(p.roaming.ends_with("KIVO") && p.local.ends_with("KIVO"));
        // macOS uses one folder for both roots; the subfolders keep settings and data apart.
        let files = [
            p.config_file(),
            p.database(),
            p.logs(),
            p.run(),
            p.models(),
            p.crashes(),
        ];
        for (i, a) in files.iter().enumerate() {
            assert!(
                files[i + 1..]
                    .iter()
                    .all(|b| a != b && !a.starts_with(b) && !b.starts_with(a))
            );
        }
        // On Windows, settings roam (%APPDATA%) and data stays local (%LOCALAPPDATA%).
        if cfg!(windows) {
            assert_ne!(p.roaming, p.local);
        }
    }
}
