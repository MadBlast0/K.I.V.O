//! `idle` (BENCHMARKS §1, VOICE §10): what KIVO costs while nothing happens. KIVO must be
//! running; leave the PC alone during the run. By default the suite watches for 30 minutes in
//! total (split across the runs); `KIVO_BENCH_IDLE_SECONDS` sets another total for a quick look,
//! and the report says which was used.
//!
//! Per window: CPU as a share of the whole machine (VOICE §10: ≤ 2%), RAM as the private working
//! set, Task Manager's "Memory" (runtime ≤ 150 MB; UI with its WebView2 processes ≤ 120 MB), the
//! committed private memory for reference (it also counts what Windows has trimmed), wakeups (context switches per second of
//! the runtime's and app's own threads) and the CPU package power (RAPL) of the whole machine.

use crate::harness::{Sample, Suite};
use crate::win;
use std::time::{Duration, Instant};

const DEFAULT_TOTAL: Duration = Duration::from_secs(30 * 60);

pub struct Idle {
    window: Duration,
    total: Duration,
    logical_cpus: f64,
    runtime: u32,
    app: Option<u32>,
    counters: win::Counters,
    runtime_switches: win::RawCounts,
    app_switches: win::RawCounts,
    /// Whether the runtime had the microphone open, checked at the start and end of each window.
    listening: Vec<bool>,
}

/// Whether a `kivo-runtime.exe` has the microphone open now: Windows' privacy settings record
/// each desktop app's microphone use, with no stop time while it is in use (the same data behind
/// the microphone indicator in the taskbar).
fn runtime_listening() -> bool {
    const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone\NonPackaged";
    let Ok(out) = std::process::Command::new("reg")
        .args(["query", KEY, "/s", "/v", "LastUsedTimeStop"])
        .output()
    else {
        return false;
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut in_runtime = false;
    for line in text.lines() {
        if line.starts_with("HKEY_") {
            in_runtime = line.to_ascii_lowercase().ends_with("#kivo-runtime.exe");
        } else if in_runtime
            && line.trim_start().starts_with("LastUsedTimeStop")
            && line.split_whitespace().last() == Some("0x0")
        {
            return true;
        }
    }
    false
}

impl Idle {
    pub fn start(runs: u32, warmup: u32) -> Result<Self, String> {
        let runtime = *win::processes("kivo-runtime.exe").first().ok_or(
            "start KIVO first (`pnpm dev` or KIVO.exe); this suite measures the real runtime",
        )?;
        // The dev app is kivo-app.exe; a release build is KIVO.exe.
        let (app, app_name) = match win::processes("kivo-app.exe").first() {
            Some(&pid) => (Some(pid), "kivo-app"),
            None => (win::processes("KIVO.exe").first().copied(), "KIVO"),
        };
        let total = std::env::var("KIVO_BENCH_IDLE_SECONDS")
            .ok()
            .and_then(|s| s.parse().ok())
            .map_or(DEFAULT_TOTAL, Duration::from_secs);
        let window = total / (runs + warmup).max(1);
        let counters = win::Counters::open(&[r"\Energy Meter(*_pkg)\Power"])?;
        // Context switches are read raw: threads come and go, and a reused thread name would
        // make a computed rate negative.
        // Thread instances are "<process>/<n>"; the slash keeps "KIVO" from matching "kivo-runtime".
        let runtime_switches =
            win::RawCounts::open(r"\Thread(kivo-runtime/*)\Context Switches/sec")?;
        let app_switches =
            win::RawCounts::open(&format!(r"\Thread({app_name}/*)\Context Switches/sec"))?;
        let logical_cpus = std::thread::available_parallelism().map_or(1, |n| n.get());
        #[allow(clippy::cast_precision_loss, reason = "a CPU count")]
        Ok(Self {
            window,
            total,
            logical_cpus: logical_cpus as f64,
            runtime,
            app,
            counters,
            runtime_switches,
            app_switches,
            listening: Vec::new(),
        })
    }

    /// CPU time and private memory, summed over processes.
    fn usage(pids: &[u32]) -> win::Usage {
        let mut total = win::Usage::default();
        for &pid in pids {
            // A WebView2 helper can exit mid-run; what is left still counts.
            if let Ok(u) = win::process_usage(pid) {
                total.cpu += u.cpu;
                total.ram += u.ram;
                total.committed += u.committed;
            }
        }
        total
    }
}

fn percent(cpu: Duration, wall: Duration, cpus: f64) -> f64 {
    cpu.as_secs_f64() / (wall.as_secs_f64() * cpus) * 100.0
}

#[allow(clippy::cast_precision_loss, reason = "memory in MB")]
fn mb(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

impl Suite for Idle {
    fn name(&self) -> &'static str {
        "idle"
    }

    fn run(&mut self) -> Result<Vec<Sample>, String> {
        let app: Vec<u32> = self.app.map(win::with_descendants).unwrap_or_default();
        let runtime = [self.runtime];
        let (rt0, app0) = (Self::usage(&runtime), Self::usage(&app));
        self.counters.sample()?;
        let (rt_sw0, app_sw0) = (self.runtime_switches.read()?, self.app_switches.read()?);
        self.listening.push(runtime_listening());
        let started = Instant::now();
        std::thread::sleep(self.window);
        let wall = started.elapsed();
        self.listening.push(runtime_listening());
        let power = self.counters.sample()?[0];
        let seconds = wall.as_secs_f64();
        let runtime_wakeups = win::rate(&rt_sw0, &self.runtime_switches.read()?, seconds);
        let app_wakeups = win::rate(&app_sw0, &self.app_switches.read()?, seconds);
        let (rt1, app1) = (Self::usage(&runtime), Self::usage(&app));

        let mut samples = vec![
            Sample::cost(
                "runtime CPU (share of the whole machine)",
                "%",
                percent(rt1.cpu.saturating_sub(rt0.cpu), wall, self.logical_cpus),
            ),
            Sample::cost("runtime RAM (private working set)", "MB", mb(rt1.ram)),
            Sample::cost("runtime committed memory", "MB", mb(rt1.committed)),
            Sample::cost("runtime wakeups", "/s", runtime_wakeups),
            Sample::cost("CPU package power (whole machine)", "W", power / 1000.0),
        ];
        if !app.is_empty() {
            samples.extend([
                Sample::cost(
                    "app + WebView2 CPU (share of the whole machine)",
                    "%",
                    percent(app1.cpu.saturating_sub(app0.cpu), wall, self.logical_cpus),
                ),
                Sample::cost(
                    "app + WebView2 RAM (private working set)",
                    "MB",
                    mb(app1.ram),
                ),
                Sample::cost("app + WebView2 committed memory", "MB", mb(app1.committed)),
                Sample::cost("app wakeups (its own threads)", "/s", app_wakeups),
            ]);
        }
        Ok(samples)
    }

    fn notes(&self) -> Vec<String> {
        vec![
            format!(
                "Watched for {} s in total ({} s per window; the full suite is 1800 s).",
                self.total.as_secs(),
                self.window.as_secs()
            ),
            format!(
                "The app was {}. Its WebView2 processes are counted with it for CPU and memory, \
                 but not for wakeups (their thread counters can't be told apart from other apps').",
                if self.app.is_some() {
                    "running"
                } else {
                    "not running"
                }
            ),
            if self.listening.iter().all(|l| *l) {
                "Listening: the runtime had the microphone open throughout (\"Hey Kivo\" on; \
                 checked in Windows' microphone privacy records at the start and end of every \
                 window)."
                    .into()
            } else if self.listening.iter().any(|l| *l) {
                "Listening for only part of the run: the microphone was closed at some checks."
                    .into()
            } else {
                "Not listening: the runtime didn't have the microphone open (\"Hey Kivo\" off, \
                 or no microphone)."
                    .into()
            },
            "Package power covers everything running on the machine; compare runs on a quiet \
             desktop."
                .into(),
        ]
    }
}
