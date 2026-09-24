//! What the runtime and the app share about KIVO's own updates (DISTRIBUTION §2, DIST-08/09):
//! version order, and the pending-update record that makes a failed update roll back.
//!
//! `updates/pending.json` is written just before the installer runs: the version it came from and
//! goes to, the installer, and the previous version's installer kept for a rollback. After the
//! update:
//!
//! - the runtime counts its own starts (`runtime_started`) and, once it has been serving for a
//!   moment, marks the update healthy (`healthy`): the record becomes `installed.json` for the
//!   "What's new" dialog and older installers are removed;
//! - the app, which starts the runtime, counts the starts it had to make (`app_started_runtime`);
//!   when the new version has failed to start the runtime twice, it reinstalls the previous one
//!   (`rollback_due`).

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::path::{Path, PathBuf};

/// The file kept while an update hasn't proven itself.
pub const PENDING: &str = "pending.json";
/// The last update that did, for "What's new".
pub const INSTALLED: &str = "installed.json";
/// Starts of the new version that may fail before KIVO goes back.
pub const MAX_FAILED_STARTS: u32 = 2;

/// SemVer order with pre-releases (`1.2.0-beta.2` < `1.2.0-beta.10` < `1.2.0`); `None` when either
/// isn't a version.
pub fn compare(a: &str, b: &str) -> Option<Ordering> {
    let (a_core, a_pre) = split(a)?;
    let (b_core, b_pre) = split(b)?;
    Some(a_core.cmp(&b_core).then_with(|| match (a_pre, b_pre) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(x), Some(y)) => pre_cmp(x, y),
    }))
}

/// `candidate` is newer than `current`.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    compare(candidate, current) == Some(Ordering::Greater)
}

fn split(v: &str) -> Option<([u64; 3], Option<&str>)> {
    let v = v.trim().trim_start_matches('v');
    let v = v.split('+').next()?;
    let (core, pre) = match v.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (v, None),
    };
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let out = [parts.next()??, parts.next()??, parts.next()??];
    if parts.next().is_some() {
        return None;
    }
    Some((out, pre))
}

fn pre_cmp(a: &str, b: &str) -> Ordering {
    let mut x = a.split('.');
    let mut y = b.split('.');
    loop {
        match (x.next(), y.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(p), Some(q)) => {
                let o = match (p.parse::<u64>(), q.parse::<u64>()) {
                    (Ok(m), Ok(n)) => m.cmp(&n),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => p.cmp(q),
                };
                if o != Ordering::Equal {
                    return o;
                }
            }
        }
    }
}

/// An update on its way in, until it has proven itself.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Pending {
    pub from: String,
    pub to: String,
    /// The new version's installer (kept: it is the next update's rollback).
    pub installer: PathBuf,
    /// The previous version's installer, run again if the new one fails.
    pub previous: Option<PathBuf>,
    /// `nsis` or `msi`, as installed.
    pub kind: String,
    /// The notes shown in "What's new".
    pub notes: String,
    /// Epoch milliseconds.
    pub at: i64,
    /// Times the new runtime has started.
    pub runtime_starts: u32,
    /// Times the app had to start the runtime for the new version.
    pub app_starts: u32,
    pub healthy: bool,
    /// A rollback was started; nothing more is tried.
    pub rolled_back: bool,
}

/// The update that proved itself, until the user has seen what's new.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Installed {
    pub version: String,
    pub from: String,
    pub notes: String,
    pub seen: bool,
}

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn write<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value).unwrap_or_default())?;
    std::fs::rename(&tmp, path)
}

pub fn pending(dir: &Path) -> Option<Pending> {
    read(&dir.join(PENDING))
}

pub fn save_pending(dir: &Path, pending: &Pending) -> std::io::Result<()> {
    write(&dir.join(PENDING), pending)
}

pub fn installed(dir: &Path) -> Option<Installed> {
    read(&dir.join(INSTALLED))
}

pub fn save_installed(dir: &Path, installed: &Installed) -> std::io::Result<()> {
    write(&dir.join(INSTALLED), installed)
}

/// The runtime of `version` has started: counted while its update is unproven. An update that
/// never arrived (the installer was cancelled: still the old version) is forgotten.
pub fn runtime_started(dir: &Path, version: &str) -> Option<Pending> {
    let mut p = pending(dir)?;
    if p.to != version {
        if p.from == version && !p.rolled_back {
            tracing::warn!(to = p.to, "an update didn't install; still on {version}");
        }
        let _ = std::fs::remove_file(dir.join(PENDING));
        return None;
    }
    if p.healthy {
        return None;
    }
    p.runtime_starts += 1;
    let _ = save_pending(dir, &p);
    Some(p)
}

/// The new version runs: "What's new" is ready, the pending record goes, and only the installers
/// of this version and the one before stay.
pub fn healthy(dir: &Path, version: &str) -> Option<Installed> {
    let p = pending(dir).filter(|p| p.to == version && !p.healthy)?;
    let installed = Installed {
        version: p.to.clone(),
        from: p.from.clone(),
        notes: p.notes.clone(),
        seen: false,
    };
    let _ = save_installed(dir, &installed);
    let _ = std::fs::remove_file(dir.join(PENDING));
    let keep: Vec<&Path> = [Some(p.installer.as_path()), p.previous.as_deref()]
        .into_iter()
        .flatten()
        .filter_map(Path::parent)
        .collect();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let path = e.path();
            if path.is_dir() && !keep.iter().any(|k| *k == path) {
                let _ = std::fs::remove_dir_all(&path);
            }
        }
    }
    Some(installed)
}

/// The app is about to start the runtime of `version`: counted while its update is unproven.
/// Returns the previous installer to run instead when the new version has already failed to
/// start the runtime twice.
pub fn app_started_runtime(dir: &Path, version: &str) -> Option<PathBuf> {
    let mut p = pending(dir).filter(|p| p.to == version && !p.healthy && !p.rolled_back)?;
    if let Some(previous) = rollback_due(&p) {
        p.rolled_back = true;
        let _ = save_pending(dir, &p);
        return Some(previous);
    }
    p.app_starts += 1;
    let _ = save_pending(dir, &p);
    None
}

/// The previous installer, when the new version has failed to start the runtime twice.
pub fn rollback_due(p: &Pending) -> Option<PathBuf> {
    let failed = p.app_starts.max(p.runtime_starts);
    (!p.healthy && !p.rolled_back && failed >= MAX_FAILED_STARTS)
        .then(|| p.previous.clone())
        .flatten()
        .filter(|path| path.is_file())
}

/// The arguments an installer runs with for an update: the NSIS template's passive mode, restart
/// and update flags (as `tauri-plugin-updater` passes them), or msiexec's passive install with the
/// app relaunched.
pub fn installer_args(kind: &str, installer: &Path) -> (PathBuf, Vec<String>) {
    if kind == "msi" {
        let system = std::env::var("SYSTEMROOT").unwrap_or_else(|_| r"C:\Windows".into());
        (
            PathBuf::from(system).join(r"System32\msiexec.exe"),
            vec![
                "/i".into(),
                installer.display().to_string(),
                "/passive".into(),
                "/promptrestart".into(),
                "AUTOLAUNCHAPP=True".into(),
            ],
        )
    } else {
        (
            installer.to_path_buf(),
            vec!["/P".into(), "/R".into(), "/UPDATE".into()],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_order_like_semver() {
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("1.0.0", "1.0.0-beta.9"));
        assert!(is_newer("1.0.0-beta.10", "1.0.0-beta.2"));
        assert!(is_newer("v1.0.0-exp.2", "1.0.0-exp.1"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("0.9.9", "1.0.0-beta.1"));
        assert_eq!(compare("1.2", "1.2.0"), None);
        assert_eq!(compare("1.2.0+build.5", "1.2.0"), Some(Ordering::Equal));
    }

    fn setup(dir: &Path, previous: bool) -> Pending {
        let old = dir.join("0.1.0");
        let new = dir.join("0.2.0");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        std::fs::create_dir_all(dir.join("0.0.9")).unwrap();
        std::fs::write(old.join("KIVO_0.1.0_x64-setup.exe"), b"old").unwrap();
        std::fs::write(new.join("KIVO_0.2.0_x64-setup.exe"), b"new").unwrap();
        let p = Pending {
            from: "0.1.0".into(),
            to: "0.2.0".into(),
            installer: new.join("KIVO_0.2.0_x64-setup.exe"),
            previous: previous.then(|| old.join("KIVO_0.1.0_x64-setup.exe")),
            kind: "nsis".into(),
            notes: "- Faster wake word".into(),
            ..Pending::default()
        };
        save_pending(dir, &p).unwrap();
        p
    }

    #[test]
    fn a_healthy_update_leaves_whats_new_and_two_installers() {
        let dir = tempfile::tempdir().unwrap();
        setup(dir.path(), true);
        assert_eq!(
            runtime_started(dir.path(), "0.2.0").unwrap().runtime_starts,
            1
        );
        let installed = healthy(dir.path(), "0.2.0").unwrap();
        assert_eq!(
            (installed.version.as_str(), installed.seen),
            ("0.2.0", false)
        );
        assert!(pending(dir.path()).is_none());
        assert_eq!(
            super::installed(dir.path()).unwrap().notes,
            "- Faster wake word"
        );
        assert!(dir.path().join("0.2.0").is_dir() && dir.path().join("0.1.0").is_dir());
        assert!(!dir.path().join("0.0.9").exists(), "older installers go");
        // Nothing more to count once it's proven.
        assert!(runtime_started(dir.path(), "0.2.0").is_none());
        assert!(app_started_runtime(dir.path(), "0.2.0").is_none());
    }

    #[test]
    fn two_failed_starts_roll_back_to_the_previous_installer_once() {
        let dir = tempfile::tempdir().unwrap();
        let p = setup(dir.path(), true);
        assert!(
            app_started_runtime(dir.path(), "0.2.0").is_none(),
            "first start"
        );
        runtime_started(dir.path(), "0.2.0");
        assert!(
            app_started_runtime(dir.path(), "0.2.0").is_none(),
            "second start"
        );
        runtime_started(dir.path(), "0.2.0");
        assert_eq!(app_started_runtime(dir.path(), "0.2.0"), p.previous);
        assert!(pending(dir.path()).unwrap().rolled_back);
        assert!(
            app_started_runtime(dir.path(), "0.2.0").is_none(),
            "only once"
        );
    }

    #[test]
    fn without_a_kept_installer_there_is_nothing_to_roll_back_to() {
        let dir = tempfile::tempdir().unwrap();
        setup(dir.path(), false);
        for _ in 0..3 {
            runtime_started(dir.path(), "0.2.0");
            assert!(app_started_runtime(dir.path(), "0.2.0").is_none());
        }
    }

    #[test]
    fn an_update_that_never_installed_is_forgotten() {
        let dir = tempfile::tempdir().unwrap();
        setup(dir.path(), true);
        assert!(runtime_started(dir.path(), "0.1.0").is_none());
        assert!(pending(dir.path()).is_none());
    }

    #[test]
    fn installers_run_as_the_updater_runs_them() {
        let (exe, args) = installer_args("nsis", Path::new(r"C:\u\KIVO_0.2.0_x64-setup.exe"));
        assert_eq!(exe, Path::new(r"C:\u\KIVO_0.2.0_x64-setup.exe"));
        assert_eq!(args, ["/P", "/R", "/UPDATE"]);
        let (exe, args) = installer_args("msi", Path::new(r"C:\u\KIVO.msi"));
        assert!(exe.ends_with(r"System32\msiexec.exe"));
        assert_eq!(args[..2], ["/i".to_owned(), r"C:\u\KIVO.msi".to_owned()]);
        assert!(args.contains(&"AUTOLAUNCHAPP=True".to_owned()));
    }
}
