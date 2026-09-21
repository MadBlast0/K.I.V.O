//! `audio` (VOICE-01, VOICE §10): the cost of keeping the microphone open, as KIVO does while
//! listening, and of the playback path. The default devices through `kivo-platform-windows`'
//! WASAPI streams; this process does nothing else meanwhile, so its CPU time is the audio path's.

use crate::harness::{Sample, Suite};
use crate::win;
use kivo_platform::{AudioIo, StreamFormat};
use kivo_platform_windows::WindowsAudio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(10);

pub struct Audio {
    logical_cpus: f64,
    format: Option<StreamFormat>,
    input: String,
}

impl Audio {
    pub fn start() -> Result<Self, String> {
        let inputs = WindowsAudio.input_devices().map_err(|e| e.to_string())?;
        let input = inputs
            .iter()
            .find(|d| d.is_default)
            .map(|d| d.name.clone())
            .ok_or("no default microphone")?;
        #[allow(clippy::cast_precision_loss, reason = "a CPU count")]
        let logical_cpus = std::thread::available_parallelism().map_or(1, |n| n.get()) as f64;
        Ok(Self {
            logical_cpus,
            format: None,
            input,
        })
    }

    fn cpu() -> Result<Duration, String> {
        win::process_usage(std::process::id()).map(|(cpu, _)| cpu)
    }
}

/// Callback arrival times and frame counts, recorded on the audio thread.
#[derive(Default)]
struct Log {
    at: Vec<Instant>,
    frames: usize,
}

impl Suite for Audio {
    fn name(&self) -> &'static str {
        "audio"
    }

    fn run(&mut self) -> Result<Vec<Sample>, String> {
        // Capture: hold the default microphone open for the window.
        let log = Arc::new(Mutex::new(Log {
            at: Vec::with_capacity(4096),
            frames: 0,
        }));
        let sink_log = Arc::clone(&log);
        let cpu0 = Self::cpu()?;
        let opened = Instant::now();
        let stream = WindowsAudio
            .open_capture(
                None,
                Box::new(move |samples, format| {
                    if let Ok(mut log) = sink_log.lock() {
                        log.at.push(Instant::now());
                        log.frames += samples.len() / usize::from(format.channels);
                    }
                }),
            )
            .map_err(|e| e.to_string())?;
        let format = stream.format();
        std::thread::sleep(WINDOW);
        let wall = opened.elapsed();
        drop(stream);
        let cpu = Self::cpu()?.saturating_sub(cpu0);
        self.format = Some(format);

        let log = log.lock().map_err(|_| "audio log poisoned")?;
        let first = log.at.first().ok_or("the microphone delivered nothing")?;
        let start_ms = first.duration_since(opened).as_secs_f64() * 1000.0;
        let mut periods: Vec<f64> = log
            .at
            .windows(2)
            .map(|w| w[1].duration_since(w[0]).as_secs_f64() * 1000.0)
            .collect();
        periods.sort_by(f64::total_cmp);
        let period = periods.get(periods.len() / 2).copied().unwrap_or(0.0);
        let jitter = periods
            .get(periods.len() * 99 / 100)
            .copied()
            .unwrap_or(0.0);
        // Frames delivered versus what the device clock produced since the first packet.
        let expected = wall
            .saturating_sub(first.duration_since(opened))
            .as_secs_f64()
            * f64::from(format.sample_rate);
        #[allow(clippy::cast_precision_loss, reason = "frame counts")]
        let delivered = log.frames as f64 / expected * 100.0;

        // Playback: silence for a short while (the path TTS and earcons use).
        let pb_log = Arc::new(Mutex::new(Option::<Instant>::None));
        let first_fill = Arc::clone(&pb_log);
        let cpu1 = Self::cpu()?;
        let started = Instant::now();
        let playback = WindowsAudio
            .open_playback(
                None,
                Box::new(move |buffer, _| {
                    buffer.fill(0.0);
                    if let Ok(mut first) = first_fill.lock() {
                        first.get_or_insert_with(Instant::now);
                    }
                }),
            )
            .map_err(|e| e.to_string())?;
        std::thread::sleep(Duration::from_secs(2));
        let pb_wall = started.elapsed();
        drop(playback);
        let pb_cpu = Self::cpu()?.saturating_sub(cpu1);
        let pb_start = pb_log
            .lock()
            .map_err(|_| "playback log poisoned")?
            .map_or(0.0, |t| t.duration_since(started).as_secs_f64() * 1000.0);

        let share = |cpu: Duration, wall: Duration| {
            cpu.as_secs_f64() / (wall.as_secs_f64() * self.logical_cpus) * 100.0
        };
        Ok(vec![
            Sample::cost(
                "capture CPU (share of the whole machine)",
                "%",
                share(cpu, wall),
            ),
            Sample::cost("capture open → first packet", "ms", start_ms),
            Sample::cost("capture packet period (median)", "ms", period),
            Sample::cost("capture packet period (p99)", "ms", jitter),
            Sample {
                metric: "capture frames delivered vs device clock".into(),
                unit: "%",
                lower_is_better: false,
                value: delivered,
            },
            Sample::cost("playback open → first fill", "ms", pb_start),
            Sample::cost(
                "playback CPU (share of the whole machine)",
                "%",
                share(pb_cpu, pb_wall),
            ),
        ])
    }

    fn notes(&self) -> Vec<String> {
        let mut notes = vec![format!(
            "Default microphone \"{}\" held open for {} s per run (shared mode, event-driven, \
             32-bit float, MMCSS \"Audio\" thread); the callback only counts frames.",
            self.input,
            WINDOW.as_secs()
        )];
        if let Some(f) = self.format {
            notes.push(format!(
                "Device format: {} Hz, {} channel(s).",
                f.sample_rate, f.channels
            ));
        }
        notes.push("Playback: 2 s of silence on the default output per run.".into());
        notes
    }
}
