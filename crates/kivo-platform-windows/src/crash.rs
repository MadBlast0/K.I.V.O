//! Crash dumps (ARCHITECTURE §1, §5; ARCH-10). Each KIVO process installs this at startup:
//!
//! - a **panic** writes `<process>-<time>.txt` with the message, the location and a backtrace;
//! - a **native crash** (access violation and the like) writes a minidump `<process>-<time>.dmp`.
//!
//! Both stay in `%LOCALAPPDATA%\KIVO\crashes\` and are never uploaded (DIST-15); the runtime
//! reports new ones on its next start (`kivo_store::crashes`).

use std::io::Write as _;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Diagnostics::Debug::{
    EXCEPTION_POINTERS, MINIDUMP_EXCEPTION_INFORMATION, MiniDumpNormal, MiniDumpWithThreadInfo,
    MiniDumpWriteDump, SetUnhandledExceptionFilter,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId,
};

/// `EXCEPTION_CONTINUE_SEARCH`: let Windows finish the crash after the dump is written.
const CONTINUE_SEARCH: i32 = 0;

struct Target {
    dir: PathBuf,
    process: String,
}

static TARGET: OnceLock<Target> = OnceLock::new();

/// A file name that sorts by time: `kivo-runtime-<unix seconds>.dmp`.
fn file_name(process: &str, extension: &str) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format!("{process}-{now}.{extension}")
}

/// Starts recording crashes of this process into `dir`. Call once, early in `main`.
pub fn install(dir: &Path, process: &str) {
    let _ = std::fs::create_dir_all(dir);
    if TARGET
        .set(Target {
            dir: dir.to_path_buf(),
            process: process.to_owned(),
        })
        .is_err()
    {
        return; // already installed
    }

    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(target) = TARGET.get() {
            let path = target.dir.join(file_name(&target.process, "txt"));
            if let Ok(mut file) = std::fs::File::create(&path) {
                let backtrace = std::backtrace::Backtrace::force_capture();
                let thread = std::thread::current();
                let _ = writeln!(
                    file,
                    "KIVO {} ({}) panicked on thread '{}'\n{info}\n\n{backtrace}",
                    env!("CARGO_PKG_VERSION"),
                    target.process,
                    thread.name().unwrap_or("unnamed"),
                );
            }
        }
        previous(info);
    }));

    // SAFETY: installs a process-wide filter; `write_minidump` only touches statics and files.
    unsafe {
        SetUnhandledExceptionFilter(Some(write_minidump));
    }
}

unsafe extern "system" fn write_minidump(info: *const EXCEPTION_POINTERS) -> i32 {
    let Some(target) = TARGET.get() else {
        return CONTINUE_SEARCH;
    };
    let path = target.dir.join(file_name(&target.process, "dmp"));
    let Ok(file) = std::fs::File::create(&path) else {
        return CONTINUE_SEARCH;
    };
    let exception = MINIDUMP_EXCEPTION_INFORMATION {
        // SAFETY: plain queries of the current process and thread.
        ThreadId: unsafe { GetCurrentThreadId() },
        ExceptionPointers: info.cast_mut(),
        ClientPointers: false.into(),
    };
    // SAFETY: the file handle is open for writing and outlives the call; the exception pointers
    // come from Windows for this crash.
    unsafe {
        let _ = MiniDumpWriteDump(
            GetCurrentProcess(),
            GetCurrentProcessId(),
            HANDLE(file.as_raw_handle()),
            MiniDumpNormal | MiniDumpWithThreadInfo,
            Some(&raw const exception),
            None,
            None,
        );
    }
    CONTINUE_SEARCH
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_leaves_a_readable_report() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), "kivo-test");
        let _ = std::thread::Builder::new()
            .name("crashing".into())
            .spawn(|| panic!("something broke on purpose"))
            .unwrap()
            .join();
        let report = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .find(|e| e.file_name().to_string_lossy().ends_with(".txt"))
            .expect("a report was written");
        let text = std::fs::read_to_string(report.path()).unwrap();
        assert!(
            text.contains("something broke on purpose") && text.contains("'crashing'"),
            "{text}"
        );
        assert!(
            report
                .file_name()
                .to_string_lossy()
                .starts_with("kivo-test-")
        );
    }
}
