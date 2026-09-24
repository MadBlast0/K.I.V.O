//! `open` (BENCHMARKS §3, BENCH-12): the Control Center's warm open. With KIVO running and its
//! window closed to the tray (and so suspended, `src-tauri/src/memory.rs`), KIVO is launched again
//! — what the Start menu or the taskbar does — and the suite times the window's return: shown, and
//! drawn as it was before it closed (its pixels match a capture taken while it was open). Nothing
//! is typed or clicked: the window is closed with its own close message.
//!
//! `KIVO_APP_EXE` names the app to launch (default `target/release/KIVO.exe`).

use crate::harness::{Sample, Suite};
use crate::win;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// The Control Center's title.
const TITLE: &str = "KIVO";
/// How long a closed window rests before it is opened again (it is suspended when hidden).
const REST: Duration = Duration::from_secs(3);
/// Pixels sampled along the window's middle row, and how close counts as "drawn".
const SAMPLES: i32 = 64;
const SAME: f64 = 12.0;

pub struct Open {
    exe: PathBuf,
    hwnd: isize,
    rect: (i32, i32, i32, i32),
    reference: Vec<[u8; 3]>,
}

fn strip(rect: (i32, i32, i32, i32)) -> Result<Vec<[u8; 3]>, String> {
    let (x, y, w, h) = rect;
    // A row across the middle of the page area (right of the sidebar).
    let left = x + w / 4;
    let width = (w / 2).max(SAMPLES);
    let row = win::grab(left, y + h / 2, width, 1)?;
    let step = (row.len() / SAMPLES as usize).max(1);
    Ok(row
        .into_iter()
        .step_by(step)
        .take(SAMPLES as usize)
        .collect())
}

fn distance(a: &[[u8; 3]], b: &[[u8; 3]]) -> f64 {
    let total: u32 = a
        .iter()
        .zip(b)
        .map(|(p, q)| {
            p.iter()
                .zip(q)
                .map(|(x, y)| u32::from(x.abs_diff(*y)))
                .sum::<u32>()
        })
        .sum();
    #[allow(clippy::cast_precision_loss, reason = "a small average")]
    let n = (a.len().max(1) * 3) as f64;
    f64::from(total) / n
}

impl Open {
    pub fn start() -> Result<Self, String> {
        let exe = std::env::var_os("KIVO_APP_EXE")
            .map_or_else(|| PathBuf::from("target/release/KIVO.exe"), PathBuf::from);
        if !exe.is_file() {
            return Err(format!(
                "{} isn't there: build the release app, or set KIVO_APP_EXE",
                exe.display()
            ));
        }
        let hwnd = win::find_window(TITLE).ok_or("start KIVO first and open its Control Center")?;
        if !win::is_visible(hwnd) {
            return Err("open the Control Center first: its picture is the reference".into());
        }
        let rect = win::window_rect(hwnd).ok_or("couldn't read the window's position")?;
        let reference = strip(rect)?;
        Ok(Self {
            exe,
            hwnd,
            rect,
            reference,
        })
    }
}

impl Suite for Open {
    fn name(&self) -> &'static str {
        "open"
    }

    fn run(&mut self) -> Result<Vec<Sample>, String> {
        win::post_close(self.hwnd)?;
        let closing = Instant::now();
        while win::is_visible(self.hwnd) {
            if closing.elapsed() > Duration::from_secs(5) {
                return Err("the Control Center didn't close".into());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(REST);

        let started = Instant::now();
        std::process::Command::new(&self.exe)
            .spawn()
            .map_err(|e| format!("{}: {e}", self.exe.display()))?;
        let mut shown = None;
        loop {
            if started.elapsed() > Duration::from_secs(10) {
                return Err("the Control Center didn't come back within 10 s".into());
            }
            if shown.is_none() && win::is_visible(self.hwnd) {
                shown = Some(started.elapsed());
            }
            if shown.is_some() && distance(&strip(self.rect)?, &self.reference) < SAME {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        let drawn = started.elapsed();
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        Ok(vec![
            Sample::cost("launch → window shown", "ms", ms(shown.unwrap_or(drawn))),
            Sample::cost("launch → page drawn (warm open)", "ms", ms(drawn)),
        ])
    }

    fn notes(&self) -> Vec<String> {
        vec![format!(
            "{} launched again while KIVO runs with its Control Center closed to the tray (suspended) \
             for {} s; \"drawn\" is when a row of the page matches a capture taken while it was open. \
             The time includes starting the second app process, which hands over to the running \
             one and exits, as a Start-menu launch does.",
            self.exe.display(),
            REST.as_secs()
        )]
    }
}
