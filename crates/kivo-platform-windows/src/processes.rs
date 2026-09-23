//! Processes for watchers (TOOL-28): the process list from a ToolHelp snapshot, and a wait for
//! one process to exit on its handle together with a cancel event, so the waiting thread sleeps in
//! the kernel until either fires.

use kivo_platform::{PlatformError, PlatformResult, ProcessInfo, ProcessWait, Processes};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    CreateEventW, GetExitCodeProcess, INFINITE, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE, SetEvent, WaitForMultipleObjects,
};

#[derive(Default)]
pub struct WindowsProcesses;

/// An owned kernel handle.
struct Owned(HANDLE);

// SAFETY: kernel handles may be used from any thread.
unsafe impl Send for Owned {}
// SAFETY: as above; waits and SetEvent are thread-safe.
unsafe impl Sync for Owned {}

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: the handle is owned and closed once.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

struct Wait {
    process: Owned,
    cancel: Owned,
}

impl ProcessWait for Wait {
    fn wait(&self) -> Option<i32> {
        let handles = [self.process.0, self.cancel.0];
        // SAFETY: both handles are valid for the life of `self`.
        let r = unsafe { WaitForMultipleObjects(&handles, false, INFINITE) };
        if r != WAIT_OBJECT_0 {
            return None;
        }
        let mut code = 0u32;
        // SAFETY: a process handle opened with query rights.
        unsafe { GetExitCodeProcess(self.process.0, &raw mut code) }.ok()?;
        #[allow(
            clippy::cast_possible_wrap,
            reason = "exit codes are reported as signed"
        )]
        Some(code as i32)
    }

    fn cancel(&self) {
        // SAFETY: an event handle owned by `self`.
        unsafe {
            let _ = SetEvent(self.cancel.0);
        }
    }
}

impl Processes for WindowsProcesses {
    fn list(&self) -> PlatformResult<Vec<ProcessInfo>> {
        // SAFETY: a snapshot of every process, closed below.
        let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
            .map_err(|e| crate::com::os_error(&e))?;
        let snap = Owned(snap);
        let mut entry = PROCESSENTRY32W {
            dwSize: u32::try_from(std::mem::size_of::<PROCESSENTRY32W>()).unwrap_or(0),
            ..Default::default()
        };
        let mut out = Vec::new();
        // SAFETY: `entry` has its size set; the snapshot handle is valid.
        let mut more = unsafe { Process32FirstW(snap.0, &raw mut entry) }.is_ok();
        while more {
            let len = entry
                .szExeFile
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(entry.szExeFile.len());
            out.push(ProcessInfo {
                pid: entry.th32ProcessID,
                name: String::from_utf16_lossy(&entry.szExeFile[..len]),
                parent: entry.th32ParentProcessID,
            });
            // SAFETY: as above.
            more = unsafe { Process32NextW(snap.0, &raw mut entry) }.is_ok();
        }
        Ok(out)
    }

    fn watch(&self, pid: u32) -> PlatformResult<Box<dyn ProcessWait>> {
        // SAFETY: opening with only the rights the wait needs; checked below.
        let process = unsafe {
            OpenProcess(
                PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
                false,
                pid,
            )
        }
        .map_err(|_| PlatformError::NotFound(format!("process {pid}")))?;
        let process = Owned(process);
        // SAFETY: an unnamed manual-reset event, initially unset.
        let cancel = unsafe { CreateEventW(None, true, false, None) }
            .map_err(|e| crate::com::os_error(&e))?;
        Ok(Box::new(Wait {
            process,
            cancel: Owned(cancel),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::process::CommandExt;
    use std::time::{Duration, Instant};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    #[test]
    fn a_process_is_listed_and_its_exit_is_waited_for() {
        // A process of the test's own, which ends by itself after about a second.
        let mut child = std::process::Command::new("cmd")
            .args(["/c", "ping -n 2 127.0.0.1 >nul & exit 3"])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .unwrap();
        let pid = child.id();
        let listed = WindowsProcesses.list().unwrap();
        assert!(
            listed
                .iter()
                .any(|p| p.pid == pid && p.name.eq_ignore_ascii_case("cmd.exe"))
        );
        assert!(listed.iter().any(|p| p.pid == std::process::id()));
        let wait = WindowsProcesses.watch(pid).unwrap();
        let started = Instant::now();
        assert_eq!(wait.wait(), Some(3));
        assert!(started.elapsed() < Duration::from_secs(10));
        // Reap it (it has already exited).
        let _ = child.wait();
    }

    #[test]
    fn a_wait_can_be_cancelled_and_a_gone_process_is_not_found() {
        let wait = std::sync::Arc::new(WindowsProcesses.watch(std::process::id()).unwrap());
        let w = std::sync::Arc::clone(&wait);
        let t = std::thread::spawn(move || w.wait());
        std::thread::sleep(Duration::from_millis(50));
        wait.cancel();
        assert_eq!(t.join().unwrap(), None, "cancelled, not exited");
        assert!(matches!(
            WindowsProcesses.watch(0xFFFF_FFF0).err(),
            Some(PlatformError::NotFound(_))
        ));
    }
}
