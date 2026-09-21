//! Crash reports on this PC (ARCHITECTURE §5, ARCH-10). The processes write them into
//! `%LOCALAPPDATA%\KIVO\crashes\`; on the next start the runtime lists the ones it hasn't reported
//! yet and marks them reported. Nothing here uploads anything (DIST-15).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Remembers which reports were already shown.
const MARKER: &str = ".reported";
/// Old reports are removed after this many are kept.
const KEEP: usize = 20;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashReport {
    pub path: PathBuf,
    /// The process that crashed (`kivo-runtime`, `kivo-infer`, `kivo-app`).
    pub process: String,
    /// Unix seconds.
    pub at: u64,
    /// A native crash dump (`.dmp`) or a panic report (`.txt`).
    pub native: bool,
    /// The first line of a panic report.
    pub summary: Option<String>,
}

fn parse(path: &Path) -> Option<CrashReport> {
    let name = path.file_name()?.to_str()?;
    let (stem, extension) = name.rsplit_once('.')?;
    if extension != "dmp" && extension != "txt" {
        return None;
    }
    let (process, at) = stem.rsplit_once('-')?;
    let summary = (extension == "txt")
        .then(|| std::fs::read_to_string(path).ok())
        .flatten()
        .and_then(|t| t.lines().nth(1).map(str::to_owned));
    Some(CrashReport {
        path: path.to_path_buf(),
        process: process.to_owned(),
        at: at.parse().ok()?,
        native: extension == "dmp",
        summary,
    })
}

/// Every report in `dir`, newest first.
pub fn all(dir: &Path) -> Vec<CrashReport> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut reports: Vec<CrashReport> = entries
        .filter_map(Result::ok)
        .filter_map(|e| parse(&e.path()))
        .collect();
    reports.sort_by_key(|r| std::cmp::Reverse(r.at));
    reports
}

/// Reports newer than the last call, and marks them reported. Also trims old reports.
pub fn take_new(dir: &Path) -> Vec<CrashReport> {
    let marker = dir.join(MARKER);
    let seen: u64 = std::fs::read_to_string(&marker)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    let reports = all(dir);
    let new: Vec<CrashReport> = reports.iter().filter(|r| r.at > seen).cloned().collect();
    if let Some(latest) = reports.first() {
        let _ = std::fs::write(&marker, latest.at.to_string());
    }
    for old in reports.iter().skip(KEEP) {
        let _ = std::fs::remove_file(&old.path);
    }
    new
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_reports_are_listed_once_then_marked() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("kivo-runtime-100.txt"),
            "KIVO 0.0.0 panicked\nboom at x.rs:1\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("kivo-infer-200.dmp"), b"MDMP").unwrap();
        std::fs::write(dir.path().join("notes.log"), b"ignored").unwrap();
        let new = take_new(dir.path());
        assert_eq!(new.len(), 2);
        assert_eq!(
            (new[0].process.as_str(), new[0].native),
            ("kivo-infer", true)
        );
        assert_eq!(new[1].summary.as_deref(), Some("boom at x.rs:1"));
        assert!(take_new(dir.path()).is_empty(), "already reported");
        std::fs::write(dir.path().join("kivo-app-300.txt"), "x\ny\n").unwrap();
        assert_eq!(take_new(dir.path()).len(), 1);
    }

    #[test]
    fn only_the_newest_reports_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..25 {
            std::fs::write(
                dir.path().join(format!("kivo-runtime-{}.dmp", 1000 + i)),
                b"x",
            )
            .unwrap();
        }
        take_new(dir.path());
        assert_eq!(all(dir.path()).len(), KEEP);
        assert_eq!(all(dir.path())[0].at, 1024);
        assert!(all(Path::new("missing-folder")).is_empty());
    }
}
