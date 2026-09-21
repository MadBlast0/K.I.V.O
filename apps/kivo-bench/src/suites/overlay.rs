//! `overlay` (BENCHMARKS §1, UX-05): the real Island, driven the way a user drives it. KIVO must
//! be running. Each run holds the push-to-talk keys with synthetic input and watches the screen:
//!
//! - hotkey → Island visible: until the capsule's black shows at the top center (the whole path:
//!   hotkey thread → runtime state → IPC → app → window shown → WebView2 frame);
//! - white flash: any near-white pixel in the Island's area before or while it appears, measured
//!   over a mid-grey backdrop so a flash cannot hide;
//! - release → Island gone (includes the designed exit animation);
//! - GPU 3D load and CPU package power (RAPL) with the Island hidden vs animating;
//! - the window's styles (never activates, click-through, topmost) and that the Island never
//!   becomes the foreground window.

use crate::harness::{Sample, Suite};
use crate::win;
use std::time::{Duration, Instant};

const TITLE: &str = "KIVO Island";
const GREY: u8 = 128;
const TIMEOUT: Duration = Duration::from_secs(3);
/// How long each cost window lasts.
const MEASURE: Duration = Duration::from_secs(2);

pub struct Overlay {
    /// Keys to hold (modifiers first).
    keys: Vec<u16>,
    chord: String,
    /// Screen strip inside the Island's capsule: x, y, width.
    strip: (i32, i32, i32),
    backdrop: (i32, i32, i32, i32),
    counters: win::Counters,
    styles: Option<String>,
}

fn virtual_keys(chord: &kivo_platform::Chord) -> Result<Vec<u16>, String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN};
    let (mods, key) = kivo_platform_windows::hotkeys::parse(chord).map_err(|e| e.to_string())?;
    let mut keys = Vec::new();
    for (m, vk) in [
        (MOD_CONTROL, 0x11),
        (MOD_ALT, 0x12),
        (MOD_SHIFT, 0x10),
        (MOD_WIN, 0x5B),
    ] {
        if mods.contains(m) {
            keys.push(vk);
        }
    }
    keys.push(key.0);
    Ok(keys)
}

impl Overlay {
    pub fn start() -> Result<Self, String> {
        if win::processes("kivo-runtime.exe").is_empty() || win::window_style(TITLE).is_none() {
            return Err(
                "start KIVO first (`pnpm dev` or KIVO.exe); this suite measures the real Island"
                    .into(),
            );
        }
        let paths = kivo_platform::Paths::user().ok_or("no AppData folders")?;
        let config = kivo_store::config::load(&paths.config_file())
            .map(|l| l.config)
            .unwrap_or_default();
        let chord = kivo_platform::Chord(config.voice.push_to_talk);
        let keys = virtual_keys(&chord)?;

        // The Island opens on the monitor under the pointer; measure on the primary monitor.
        let (width, _height, scale) = win::primary_screen();
        let px = |css: f64| {
            #[allow(clippy::cast_possible_truncation, reason = "pixel coordinates")]
            let v = (css * scale).round() as i32;
            v
        };
        let center = width / 2;
        // 5 px into the 36 px capsule (8 px from the top edge, 1 px padding): pure black there.
        let strip = (center - px(50.0), px(8.0 + 1.0 + 5.0), px(100.0));
        let backdrop = (center - px(300.0), 0, px(600.0), px(120.0));
        let counters = win::Counters::open(&[
            r"\GPU Engine(*engtype_3D)\Utilization Percentage",
            r"\Energy Meter(*_pkg)\Power",
        ])?;
        Ok(Self {
            chord: chord.to_string(),
            keys,
            strip,
            backdrop,
            counters,
            styles: None,
        })
    }

    fn grab(&self) -> Result<Vec<[u8; 3]>, String> {
        let (x, y, w) = self.strip;
        win::grab(x, y, w, 2)
    }
}

fn black(pixels: &[[u8; 3]]) -> bool {
    let dark = pixels.iter().filter(|p| p.iter().all(|&c| c <= 20)).count();
    dark * 10 >= pixels.len() * 9
}

fn backdrop_only(pixels: &[[u8; 3]]) -> bool {
    let grey = pixels
        .iter()
        .filter(|p| p.iter().all(|&c| c.abs_diff(GREY) <= 12))
        .count();
    grey * 10 >= pixels.len() * 9
}

fn white(pixels: &[[u8; 3]]) -> bool {
    pixels.iter().any(|p| p.iter().all(|&c| c >= 235))
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

impl Suite for Overlay {
    fn name(&self) -> &'static str {
        "overlay"
    }

    fn run(&mut self) -> Result<Vec<Sample>, String> {
        let (x, y, w, h) = self.backdrop;
        let backdrop = win::Backdrop::show(x, y, w, h, GREY)?;
        std::thread::sleep(Duration::from_millis(300));
        backdrop.pump();
        if !backdrop_only(&self.grab()?) {
            return Err(
                "the Island area isn't clear before the run (is the Island showing?)".into(),
            );
        }

        // Cost with the Island hidden.
        self.counters.sample()?;
        std::thread::sleep(MEASURE);
        let hidden = self.counters.sample()?;

        // Hold the keys and wait for the capsule.
        let island = win::find_window(TITLE).ok_or("the Island window is gone")?;
        let mut took_focus = false;
        let pressed = Instant::now();
        win::keys(&self.keys, false);
        let mut flash = false;
        let shown = loop {
            let pixels = self.grab()?;
            flash |= white(&pixels);
            took_focus |= win::foreground() == island;
            if black(&pixels) {
                break pressed.elapsed();
            }
            if pressed.elapsed() > TIMEOUT {
                win::keys(&self.keys.iter().rev().copied().collect::<Vec<_>>(), true);
                return Err(format!(
                    "the Island didn't appear within 3 s of {}",
                    self.chord
                ));
            }
        };
        let (ex, visible) = win::window_style(TITLE).ok_or("the Island window is gone")?;

        // Cost while the Island animates (listening waveform).
        self.counters.sample()?;
        let until = Instant::now() + MEASURE;
        while Instant::now() < until {
            flash |= white(&self.grab()?);
            took_focus |= win::foreground() == island;
            std::thread::sleep(Duration::from_millis(50));
        }
        let animating = self.counters.sample()?;
        // The Island must never become the foreground window. (Other focus changes are the
        // user's own, if they use the PC during the run.)
        let focus_kept = !took_focus;

        let released = Instant::now();
        win::keys(&self.keys.iter().rev().copied().collect::<Vec<_>>(), true);
        let gone = loop {
            if backdrop_only(&self.grab()?) {
                break released.elapsed();
            }
            if released.elapsed() > TIMEOUT {
                return Err("the Island didn't go away within 3 s of release".into());
            }
        };
        backdrop.pump();

        let styles = format!(
            "Window checks: visible {visible}; never activates {}; click-through {}; topmost {}; \
             never took focus {focus_kept}.",
            ex & win::EX_NOACTIVATE != 0,
            ex & win::EX_TRANSPARENT != 0,
            ex & win::EX_TOPMOST != 0,
        );
        let required = win::EX_NOACTIVATE | win::EX_TRANSPARENT | win::EX_TOPMOST;
        if !(visible && focus_kept && ex & required == required) {
            return Err(styles);
        }
        self.styles = Some(styles);

        Ok(vec![
            Sample::cost("hotkey → Island visible", "ms", ms(shown)),
            Sample::cost(
                "release → Island gone (incl. exit animation)",
                "ms",
                ms(gone),
            ),
            Sample::cost("white flash seen", "runs", if flash { 1.0 } else { 0.0 }),
            Sample::cost("GPU 3D load, Island hidden", "%", hidden[0]),
            Sample::cost("GPU 3D load, Island animating", "%", animating[0]),
            Sample::cost("CPU package power, Island hidden", "W", hidden[1] / 1000.0),
            Sample::cost(
                "CPU package power, Island animating",
                "W",
                animating[1] / 1000.0,
            ),
        ])
    }

    fn notes(&self) -> Vec<String> {
        let mut notes = vec![
            format!(
                "Drives the running KIVO: holds {} with synthetic input and samples the screen at \
                 the top center of the primary monitor (a grey backdrop sits behind the Island).",
                self.chord
            ),
            "GPU load is the sum of all 3D engines on the machine and power is the CPU package \
             (RAPL energy meter), so both include everything else running; compare hidden with \
             animating rather than reading them as KIVO's alone."
                .into(),
        ];
        notes.extend(self.styles.clone());
        notes
    }
}
