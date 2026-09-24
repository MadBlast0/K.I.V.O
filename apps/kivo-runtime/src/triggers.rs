//! Schedule and event triggers for routines (ROUT-11). Schedules sleep until their next time.
//! Events come from a light sample of the PC every few seconds, compared with the one before: the
//! programs running, the USB devices plugged in, the networks joined, how long since the last
//! input and the battery. Nothing is sampled when no enabled routine has an event trigger, and the
//! first sample only sets the baseline, so KIVO starting up never counts as "an app launched".
//! A routine that finishes starts the routines waiting on it. Every run is unattended, so
//! High-risk steps ask on screen first (ROUT-12).

use crate::core::Core;
use crate::routines::Routines;
use kivo_core::event::{EventKind, TaskEvent};
use kivo_core::routine::{Routine, RoutineEvent, Trigger};
use kivo_platform::{Processes, SystemInfo};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

/// How often events are sampled while a routine waits on one.
pub const SAMPLE_EVERY: Duration = Duration::from_secs(5);
/// A schedule this late (the PC was asleep) still runs; later than this it waits for next time.
const LATE_OK_MS: i64 = 10 * 60_000;
/// Idle this short means the user just came back.
const BACK_SECONDS: u32 = 30;

/// One look at the PC.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sample {
    /// Running programs, lower-case, without `.exe`.
    pub apps: BTreeSet<String>,
    pub usb: Vec<String>,
    pub networks: Vec<String>,
    pub idle_seconds: u32,
    /// (on battery, percent).
    pub battery: Option<(bool, u8)>,
}

/// Takes a sample.
pub fn sample(system: &dyn SystemInfo, processes: &dyn Processes) -> Sample {
    let apps = processes
        .list()
        .unwrap_or_default()
        .into_iter()
        .map(|p| program(&p.name))
        .collect();
    let snapshot = system.snapshot().ok();
    Sample {
        apps,
        usb: system.usb_devices(),
        networks: system.networks(),
        idle_seconds: system.presence().idle_seconds,
        battery: snapshot.and_then(|s| s.battery_percent.map(|p| (s.on_battery, p))),
    }
}

fn program(name: &str) -> String {
    let lower = name.to_lowercase();
    lower.strip_suffix(".exe").unwrap_or(&lower).to_owned()
}

fn contains(haystack: &str, needle: &str) -> bool {
    haystack
        .to_lowercase()
        .contains(&needle.trim().to_lowercase())
}

/// Whether `event` happened between two samples.
pub fn fired(before: &Sample, now: &Sample, event: &RoutineEvent) -> bool {
    match event {
        RoutineEvent::AppLaunched { app } => {
            let wanted = program(app.trim());
            !wanted.is_empty()
                && now
                    .apps
                    .iter()
                    .any(|a| (*a == wanted || a.contains(&wanted)) && !before.apps.contains(a))
        }
        RoutineEvent::UsbDevice { name } => {
            let count = |s: &Sample| {
                s.usb
                    .iter()
                    .filter(|d| name.as_deref().is_none_or(|n| contains(d, n)))
                    .count()
            };
            count(now) > count(before)
        }
        RoutineEvent::Network { name } => {
            let on = |s: &Sample| s.networks.iter().any(|n| contains(n, name));
            !name.trim().is_empty() && on(now) && !on(before)
        }
        RoutineEvent::Idle { minutes } => {
            let limit = minutes.saturating_mul(60);
            before.idle_seconds < limit && now.idle_seconds >= limit
        }
        RoutineEvent::Return { away_minutes } => {
            before.idle_seconds >= away_minutes.saturating_mul(60).max(60)
                && now.idle_seconds < BACK_SECONDS
        }
        RoutineEvent::BatteryLow { percent } => match (before.battery, now.battery) {
            (Some((was_on, was)), Some((true, is))) => {
                is <= *percent && (was > *percent || !was_on)
            }
            _ => false,
        },
        // Times of day are schedules; routines after routines come from finished tasks.
        RoutineEvent::TimeOfDay { .. } | RoutineEvent::AfterRoutine { .. } => false,
    }
}

/// The schedules of a routine: its cron triggers and its times of day, with their time zones.
fn schedules(routine: &Routine) -> Vec<(String, Option<String>)> {
    routine
        .triggers
        .iter()
        .filter_map(|t| match t {
            Trigger::Schedule { cron, tz } => Some((cron.clone(), tz.clone())),
            Trigger::Event { event } => event.schedule().and_then(Result::ok).map(|c| (c, None)),
            _ => None,
        })
        .collect()
}

/// The sampled events a routine waits on.
fn events(routine: &Routine) -> impl Iterator<Item = &RoutineEvent> {
    routine.triggers.iter().filter_map(|t| match t {
        Trigger::Event { event } => Some(event),
        _ => None,
    })
}

fn sampled(event: &RoutineEvent) -> bool {
    !matches!(
        event,
        RoutineEvent::TimeOfDay { .. } | RoutineEvent::AfterRoutine { .. }
    )
}

/// Runs the triggers until KIVO quits.
pub async fn run(
    core: Arc<Core>,
    routines: Arc<Routines>,
    system: Arc<dyn SystemInfo>,
    processes: Arc<dyn Processes>,
    db: Arc<std::sync::Mutex<kivo_store::Database>>,
) {
    let shutdown = core.shutdown();
    let mut bus = core.bus.subscribe();
    let mut before: Option<Sample> = None;
    // Each schedule's next time, by (routine id, schedule).
    let mut next: BTreeMap<(String, String), i64> = BTreeMap::new();
    let start = |id: &str, cause: &str| {
        if let Err(e) = routines.run_unattended(id, cause) {
            tracing::warn!(routine = id, cause, %e, "a triggered routine didn't start");
        }
    };
    loop {
        let enabled = routines.enabled();
        let now = kivo_store::brains::now_ms();

        // Schedules: run the ones due (or a little late), then work out their next time.
        let mut wanted = BTreeMap::new();
        for r in &enabled {
            for (cron, tz) in schedules(r) {
                let key = (
                    r.id.clone(),
                    format!("{cron}|{}", tz.clone().unwrap_or_default()),
                );
                let at = match next.get(&key) {
                    Some(&at) if at <= now => {
                        if now - at <= LATE_OK_MS {
                            start(&r.id, &format!("schedule {cron}"));
                        }
                        None
                    }
                    Some(&at) => Some(at),
                    None => None,
                };
                let at = at.or_else(|| {
                    kivo_core::cron::next_fire(&cron, tz.as_deref(), now)
                        .ok()
                        .flatten()
                });
                if let Some(at) = at {
                    wanted.insert(key, at);
                }
            }
        }
        next = wanted;

        // Events: compare with the last sample.
        let watching = enabled.iter().any(|r| events(r).any(sampled));
        if watching {
            let (s, p) = (Arc::clone(&system), Arc::clone(&processes));
            if let Ok(now_sample) = tokio::task::spawn_blocking(move || sample(&*s, &*p)).await {
                if let Some(prev) = &before {
                    for r in &enabled {
                        if let Some(e) = events(r).find(|e| fired(prev, &now_sample, e)) {
                            start(&r.id, &format!("{e:?}"));
                        }
                    }
                }
                before = Some(now_sample);
            }
        } else {
            before = None;
        }

        // Sleep until the next schedule or sample, a routine finishing, or quitting.
        let until_schedule = next
            .values()
            .min()
            .map(|at| Duration::from_millis(u64::try_from((at - now).max(0)).unwrap_or(0)));
        let wait = [
            until_schedule,
            Some(if watching {
                SAMPLE_EVERY
            } else {
                Duration::from_secs(30)
            }),
        ]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(SAMPLE_EVERY)
        .max(Duration::from_millis(200));
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            let received = tokio::select! {
                () = tokio::time::sleep_until(deadline) => break,
                () = shutdown.cancelled() => return,
                r = bus.recv() => r,
            };
            match received {
                kivo_core::Received::Event(event) => {
                    if let EventKind::Task(TaskEvent::Completed) = &event.kind
                        && let Some(task) = event.meta.task_id.as_ref()
                    {
                        let finished = db
                            .lock()
                            .ok()
                            .and_then(|d| d.task(&task.to_string()).ok().flatten())
                            .and_then(|t| t.routine_id);
                        if let Some(done) = finished {
                            for r in routines.enabled() {
                                let waits = events(&r).any(|e| {
                                    matches!(e, RoutineEvent::AfterRoutine { routine } if *routine == done)
                                });
                                if waits && r.id != done {
                                    start(&r.id, &format!("after {done}"));
                                }
                            }
                        }
                    }
                }
                kivo_core::Received::Missed(_) => {}
                kivo_core::Received::Closed => return,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(
        apps: &[&str],
        usb: &[&str],
        nets: &[&str],
        idle: u32,
        battery: Option<(bool, u8)>,
    ) -> Sample {
        Sample {
            apps: apps.iter().map(|a| (*a).to_owned()).collect(),
            usb: usb.iter().map(|a| (*a).to_owned()).collect(),
            networks: nets.iter().map(|a| (*a).to_owned()).collect(),
            idle_seconds: idle,
            battery,
        }
    }

    #[test]
    fn events_fire_on_the_change_only() {
        let base = s(&["explorer"], &["USB Root Hub"], &[], 5, Some((false, 80)));
        let spotify = RoutineEvent::AppLaunched {
            app: "Spotify.exe".into(),
        };
        let later = s(
            &["explorer", "spotify"],
            &["USB Root Hub"],
            &[],
            5,
            Some((false, 80)),
        );
        assert!(fired(&base, &later, &spotify));
        assert!(
            !fired(&later, &later, &spotify),
            "still running isn't launching"
        );

        let yubikey = RoutineEvent::UsbDevice {
            name: Some("yubikey".into()),
        };
        let any = RoutineEvent::UsbDevice { name: None };
        let plugged = s(
            &["explorer"],
            &["USB Root Hub", "YubiKey OTP+FIDO"],
            &[],
            5,
            None,
        );
        assert!(fired(&base, &plugged, &yubikey) && fired(&base, &plugged, &any));
        assert!(
            !fired(&plugged, &base, &any),
            "unplugging isn't plugging in"
        );

        let office = RoutineEvent::Network {
            name: "Office".into(),
        };
        let joined = s(&[], &[], &["Office WiFi 2"], 5, None);
        assert!(fired(&base, &joined, &office));
        assert!(!fired(&joined, &joined, &office));

        let idle = RoutineEvent::Idle { minutes: 10 };
        let back = RoutineEvent::Return { away_minutes: 10 };
        let away = s(&[], &[], &[], 600, None);
        let returned = s(&[], &[], &[], 2, None);
        assert!(fired(&base, &away, &idle) && !fired(&away, &away, &idle));
        assert!(fired(&away, &returned, &back));
        assert!(!fired(&base, &returned, &back), "not away long enough");

        let low = RoutineEvent::BatteryLow { percent: 20 };
        let on_battery = |p| s(&[], &[], &[], 5, Some((true, p)));
        assert!(fired(&on_battery(21), &on_battery(20), &low));
        assert!(
            !fired(&on_battery(20), &on_battery(19), &low),
            "once per drop"
        );
        assert!(
            fired(
                &s(&[], &[], &[], 5, Some((false, 15))),
                &on_battery(15),
                &low
            ),
            "unplugged while low"
        );
        assert!(
            !fired(
                &on_battery(21),
                &s(&[], &[], &[], 5, Some((false, 19))),
                &low
            ),
            "charging"
        );
    }

    #[test]
    fn times_of_day_are_schedules() {
        let e = RoutineEvent::TimeOfDay {
            time: "07:30".into(),
            days: vec![1, 2, 3, 4, 5],
        };
        assert_eq!(e.schedule(), Some(Ok("30 7 * * 1,2,3,4,5".to_owned())));
        let bad = RoutineEvent::TimeOfDay {
            time: "25:00".into(),
            days: vec![],
        };
        assert!(matches!(bad.schedule(), Some(Err(_))));
    }
}
