//! Running commands (TOOL-30): every command starts suspended, joins its own Job Object
//! (kill-on-close, a memory cap and a CPU cap), then runs. Output is captured up to a limit;
//! timeouts, cancellation and the emergency stop kill the whole process tree. Commands never run
//! elevated: they inherit KIVO's own unelevated token.

use kivo_platform::{
    CommandOutput, CommandRunner, CommandSpec, PlatformError, PlatformResult, ShellKind,
};
use std::collections::HashMap;
use std::io::Read;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_CPU_RATE_CONTROL_ENABLE,
    JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP, JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION,
    JOB_OBJECT_LIMIT_JOB_MEMORY, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_CPU_RATE_CONTROL_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectCpuRateControlInformation, JobObjectExtendedLimitInformation, SetInformationJobObject,
    TerminateJobObject,
};
use windows::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_SUSPENDED, OpenThread, ResumeThread, THREAD_SUSPEND_RESUME,
};

/// Most output kept per stream.
const MAX_OUTPUT: usize = 64 * 1024;

/// A Job Object handle, closed (killing what's left in it) when dropped.
struct Job(HANDLE);

// SAFETY: a kernel handle; the Job Object calls used are thread-safe.
unsafe impl Send for Job {}
// SAFETY: as above.
unsafe impl Sync for Job {}

impl Job {
    fn new(spec: &CommandSpec) -> PlatformResult<Self> {
        // SAFETY: an anonymous job; the info structs outlive the calls.
        unsafe {
            let handle = CreateJobObjectW(None, None).map_err(|e| crate::com::os_error(&e))?;
            let job = Self(handle);
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
                | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION
                | JOB_OBJECT_LIMIT_JOB_MEMORY;
            limits.JobMemoryLimit =
                usize::try_from(spec.max_memory_mb.saturating_mul(1 << 20)).unwrap_or(usize::MAX);
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>()).unwrap_or(0),
            )
            .map_err(|e| crate::com::os_error(&e))?;
            let mut cpu = JOBOBJECT_CPU_RATE_CONTROL_INFORMATION {
                ControlFlags: JOB_OBJECT_CPU_RATE_CONTROL_ENABLE
                    | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP,
                ..Default::default()
            };
            // In 1/100ths of a percent.
            cpu.Anonymous.CpuRate = u32::from(spec.max_cpu_percent.clamp(1, 100)) * 100;
            // CPU rate control can be unavailable (older Windows, nested jobs): not fatal.
            let _ = SetInformationJobObject(
                job.0,
                JobObjectCpuRateControlInformation,
                (&raw const cpu).cast(),
                u32::try_from(size_of::<JOBOBJECT_CPU_RATE_CONTROL_INFORMATION>()).unwrap_or(0),
            );
            Ok(job)
        }
    }

    fn kill(&self) {
        // SAFETY: a live job handle.
        let _ = unsafe { TerminateJobObject(self.0, 1) };
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: closing our own handle; KILL_ON_JOB_CLOSE ends anything still running.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Resumes the (single) suspended thread of a process that was created suspended.
fn resume(pid: u32) -> PlatformResult<()> {
    // SAFETY: a thread snapshot walked with a correctly sized entry; handles are closed.
    unsafe {
        let snap =
            CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0).map_err(|e| crate::com::os_error(&e))?;
        let mut entry = THREADENTRY32 {
            dwSize: u32::try_from(size_of::<THREADENTRY32>()).unwrap_or(0),
            ..Default::default()
        };
        let mut resumed = false;
        if Thread32First(snap, &raw mut entry).is_ok() {
            loop {
                if entry.th32OwnerProcessID == pid
                    && let Ok(thread) = OpenThread(THREAD_SUSPEND_RESUME, false, entry.th32ThreadID)
                {
                    ResumeThread(thread);
                    let _ = CloseHandle(thread);
                    resumed = true;
                }
                if Thread32Next(snap, &raw mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
        if resumed {
            Ok(())
        } else {
            Err(PlatformError::Os {
                code: 0,
                message: "no thread to resume".into(),
            })
        }
    }
}

/// Reads a pipe to its end on a thread, keeping at most `MAX_OUTPUT` bytes.
fn drain(mut pipe: impl Read + Send + 'static) -> std::thread::JoinHandle<(Vec<u8>, bool)> {
    std::thread::spawn(move || {
        let mut kept = Vec::new();
        let mut truncated = false;
        let mut buf = [0u8; 8192];
        while let Ok(n) = pipe.read(&mut buf) {
            if n == 0 {
                break;
            }
            let room = MAX_OUTPUT.saturating_sub(kept.len());
            kept.extend_from_slice(&buf[..n.min(room)]);
            truncated |= n > room;
        }
        (kept, truncated)
    })
}

/// Console output as text: UTF-8 when valid, else the OEM code page's usual Latin-1 subset.
fn text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => bytes.iter().map(|&b| char::from(b)).collect(),
    }
}

#[derive(Default)]
pub struct WindowsCommands {
    live: Arc<Mutex<HashMap<u64, Arc<Job>>>>,
    next: AtomicU64,
}

impl WindowsCommands {
    fn command(spec: &CommandSpec) -> Command {
        let mut cmd = match spec.shell {
            ShellKind::Pwsh => {
                // PowerShell 7 when installed, else Windows PowerShell.
                let exe = if crate::environment::current_path()
                    .iter()
                    .any(|dir| dir.join("pwsh.exe").is_file())
                {
                    "pwsh.exe"
                } else {
                    "powershell.exe"
                };
                let mut c = Command::new(exe);
                c.args([
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-Command",
                    &spec.command,
                ]);
                c
            }
            ShellKind::Cmd => {
                let mut c = Command::new("cmd.exe");
                // `/S /C "<command>"`: cmd keeps the command text as written.
                c.raw_arg(format!("/D /S /C \"{}\"", spec.command));
                c
            }
        };
        cmd.current_dir(&spec.cwd)
            .envs(spec.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_SUSPENDED.0 | CREATE_NO_WINDOW.0);
        cmd
    }
}

impl CommandRunner for WindowsCommands {
    fn run(
        &self,
        spec: &CommandSpec,
        cancelled: &(dyn Fn() -> bool + Sync),
    ) -> PlatformResult<CommandOutput> {
        if !spec.cwd.is_dir() {
            return Err(PlatformError::NotFound(spec.cwd.display().to_string()));
        }
        let job = Arc::new(Job::new(spec)?);
        let mut child = Self::command(spec).spawn().map_err(|e| PlatformError::Os {
            code: i64::from(e.raw_os_error().unwrap_or(0)),
            message: e.to_string(),
        })?;
        // Into the job before it runs a single instruction, so nothing it starts escapes.
        // SAFETY: the child's process handle is valid while `child` lives.
        let assigned = unsafe { AssignProcessToJobObject(job.0, HANDLE(child.as_raw_handle())) };
        if let Err(e) = assigned
            .map_err(|e| crate::com::os_error(&e))
            .and_then(|()| resume(child.id()))
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.live
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(id, Arc::clone(&job));
        let stdout = drain(child.stdout.take().ok_or(PlatformError::Unsupported)?);
        let stderr = drain(child.stderr.take().ok_or(PlatformError::Unsupported)?);
        let started = Instant::now();
        let mut out = CommandOutput::default();
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    out.exit_code = status.code();
                    break;
                }
                Ok(None) => {}
                Err(_) => break,
            }
            let killed_elsewhere = !self
                .live
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .contains_key(&id);
            if cancelled() || killed_elsewhere {
                out.cancelled = true;
            } else if started.elapsed() > spec.timeout {
                out.timed_out = true;
            }
            if out.cancelled || out.timed_out {
                job.kill();
                let _ = child.wait();
                break;
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        // The whole tree goes with the job, including anything the command left running.
        job.kill();
        let stopped_elsewhere = self
            .live
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&id)
            .is_none();
        out.cancelled |= stopped_elsewhere;
        let (o, to) = stdout.join().unwrap_or_default();
        let (e, te) = stderr.join().unwrap_or_default();
        out.stdout = text(&o);
        out.stderr = text(&e);
        out.truncated = to || te;
        Ok(out)
    }

    fn kill_all(&self) -> usize {
        let jobs: Vec<Arc<Job>> = self
            .live
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .drain()
            .map(|(_, j)| j)
            .collect();
        for job in &jobs {
            job.kill();
        }
        jobs.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(command: &str, shell: ShellKind, dir: &std::path::Path) -> CommandSpec {
        CommandSpec {
            command: command.into(),
            shell,
            cwd: dir.to_path_buf(),
            timeout: Duration::from_secs(20),
            env: vec![("KIVO_TEST_VALUE".into(), "from-env".into())],
            max_memory_mb: 512,
            max_cpu_percent: 50,
        }
    }

    #[test]
    fn cmd_runs_in_its_folder_with_per_call_env() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("hello.txt"), "hi").unwrap();
        let out = WindowsCommands::default()
            .run(
                &spec(
                    "dir /b & echo %KIVO_TEST_VALUE%",
                    ShellKind::Cmd,
                    dir.path(),
                ),
                &|| false,
            )
            .unwrap();
        assert_eq!(out.exit_code, Some(0));
        assert!(out.stdout.contains("hello.txt"), "{out:?}");
        assert!(out.stdout.contains("from-env"));
    }

    #[test]
    fn powershell_reports_exit_codes_and_stderr() {
        let dir = tempfile::tempdir().unwrap();
        let out = WindowsCommands::default()
            .run(
                &spec(
                    "Write-Output ok; [Console]::Error.WriteLine('bad'); exit 3",
                    ShellKind::Pwsh,
                    dir.path(),
                ),
                &|| false,
            )
            .unwrap();
        assert_eq!(out.exit_code, Some(3), "{out:?}");
        assert!(out.stdout.contains("ok"));
        assert!(out.stderr.contains("bad"));
    }

    #[test]
    fn timeouts_and_cancellation_kill_the_whole_tree() {
        let dir = tempfile::tempdir().unwrap();
        let runner = WindowsCommands::default();
        // A child that starts its own long-running child.
        let mut s = spec(
            "start /b ping -n 30 127.0.0.1 >nul & ping -n 30 127.0.0.1 >nul",
            ShellKind::Cmd,
            dir.path(),
        );
        s.timeout = Duration::from_millis(600);
        let started = Instant::now();
        let out = runner.run(&s, &|| false).unwrap();
        assert!(out.timed_out);
        assert!(started.elapsed() < Duration::from_secs(5));

        let s = spec("ping -n 30 127.0.0.1", ShellKind::Cmd, dir.path());
        let t0 = Instant::now();
        let out = runner
            .run(&s, &|| t0.elapsed() > Duration::from_millis(300))
            .unwrap();
        assert!(out.cancelled);
        assert!(t0.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn kill_all_ends_running_commands() {
        let dir = tempfile::tempdir().unwrap();
        let runner = Arc::new(WindowsCommands::default());
        let r = Arc::clone(&runner);
        let path = dir.path().to_path_buf();
        let t = std::thread::spawn(move || {
            r.run(
                &spec("ping -n 30 127.0.0.1", ShellKind::Cmd, &path),
                &|| false,
            )
        });
        let started = Instant::now();
        while runner.live.lock().unwrap().is_empty() && started.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(runner.kill_all(), 1);
        let out = t.join().unwrap().unwrap();
        assert!(out.cancelled);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn output_is_capped() {
        let dir = tempfile::tempdir().unwrap();
        let out = WindowsCommands::default()
            .run(
                &spec(
                    "1..20000 | ForEach-Object { 'line of output number ' + $_ }",
                    ShellKind::Pwsh,
                    dir.path(),
                ),
                &|| false,
            )
            .unwrap();
        assert!(out.truncated);
        assert!(out.stdout.len() <= MAX_OUTPUT);
    }
}
