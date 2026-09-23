//! M7 Settings, end to end on the real runtime pieces with fakes: export and import of settings,
//! routines and memory (grants never travel, Bypass never comes back), Reset all settings, and the
//! Performance and Diagnostics tabs' requests (UX-31, UX-37).

#![cfg(windows)]

use kivo_core::config::PermissionMode;
use kivo_runtime::scripted::{self, Rig};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

fn worker() -> PathBuf {
    let mut dir = std::env::current_exe().expect("test binary");
    dir.pop();
    dir.pop();
    dir.join("kivo-infer.exe")
}

static ONE_AT_A_TIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct Running {
    rig: Rig,
    worker: tokio::task::JoinHandle<()>,
    pump: tokio::task::JoinHandle<()>,
}

impl Running {
    async fn stop(self) {
        self.rig.core.quit();
        let _ = tokio::time::timeout(Duration::from_secs(5), self.worker).await;
        self.pump.abort();
    }
}

fn start() -> Running {
    let (rig, worker, pump) = scripted::rig(worker(), Vec::new(), PathBuf::from("no-model"));
    Running { rig, worker, pump }
}

/// Settings → General: Export on one PC, Import on another.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn settings_routines_and_memory_move_to_another_pc() {
    let _one = ONE_AT_A_TIME.lock().await;
    let here = start();
    here.rig.core.update_config(|c| {
        c.sounds.volume = 20;
        c.appearance.accent = "#12ab34".into();
        c.permissions.mode = PermissionMode::Bypass;
    });
    // A routine of the user's own, granted here.
    let mut mine = kivo_runtime::routines::starters()
        .into_iter()
        .find(|r| r.starter.is_some())
        .expect("a starter");
    mine.id = String::new();
    mine.starter = None;
    mine.name = "Deep work".into();
    mine.triggers = vec![kivo_core::routine::Trigger::Phrase {
        phrases: vec!["time for deep work".into()],
        lang: "en".into(),
    }];
    let saved = here.rig.routines.save(mine, true).unwrap();
    assert!(!saved.routine.grants.is_empty());
    let memory = here.rig.engine.memory().unwrap();
    memory
        .remember(
            "My project is in D:\\work",
            &["projects".into()],
            None,
            false,
        )
        .unwrap();
    let bundle = kivo_runtime::settings_file::export(&here.rig.core, &here.rig.engine);
    let text = bundle.to_string();
    assert!(!text.contains("\"grants\":[{"), "grants stay on this PC");
    let dir = tempfile::tempdir().unwrap();
    let file = kivo_runtime::settings_file::save(dir.path(), &bundle, 0).unwrap();
    here.stop().await;

    let there = start();
    let content = std::fs::read_to_string(&file).unwrap();
    let report =
        kivo_runtime::settings_file::import(&there.rig.core, &there.rig.engine, &content).unwrap();
    assert!(report.settings);
    assert!(report.routines >= 1, "{report:?}");
    assert_eq!(report.notes, 1);
    let config = there.rig.core.config();
    assert_eq!(config.sounds.volume, 20);
    assert_eq!(config.appearance.accent, "#12ab34");
    assert_eq!(
        config.permissions.mode,
        PermissionMode::Auto,
        "Bypass never comes back"
    );
    let imported = there
        .rig
        .routines
        .all()
        .into_iter()
        .find(|(r, _)| r.name == "Deep work")
        .expect("the routine came over")
        .0;
    assert!(imported.grants.is_empty(), "it asks until saved again here");
    let notes = there.rig.engine.memory().unwrap().overview().notes;
    assert!(notes.iter().any(|n| n.excerpt.contains("D:\\work")));
    // Importing again changes nothing that's already here.
    let again =
        kivo_runtime::settings_file::import(&there.rig.core, &there.rig.engine, &content).unwrap();
    assert_eq!(again.notes, 0);
    // Not a settings file.
    assert!(
        kivo_runtime::settings_file::import(&there.rig.core, &there.rig.engine, "{\"a\":1}")
            .is_err()
    );
    assert!(
        kivo_runtime::settings_file::import(
            &there.rig.core,
            &there.rig.engine,
            &json!({"kind": "kivo-settings", "version": 99}).to_string()
        )
        .is_err()
    );
    there.stop().await;
}

/// Settings → Performance and Diagnostics.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn performance_and_diagnostics_report_real_state() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start();
    // KIVO's app and its WebView2 helper, and someone else's program.
    r.rig.processes.start(20, "kivo-app.exe");
    r.rig.processes.start(30, "notepad.exe");
    {
        let mut usage = r.rig.system.usage.lock().unwrap();
        usage.insert(
            20,
            kivo_platform::ProcessUsage {
                cpu_ms: 5_000,
                memory_bytes: 120 * 1024 * 1024,
            },
        );
        usage.insert(
            30,
            kivo_platform::ProcessUsage {
                cpu_ms: 9_000,
                memory_bytes: 900 * 1024 * 1024,
            },
        );
    }
    // A finished request with its timings.
    {
        let db = r.rig.db.lock().unwrap();
        db.start_turn("t1", 1, "wake").unwrap();
        let mut spans = serde_json::Map::new();
        spans.insert("t1Listening".into(), json!(140));
        spans.insert("t4EndOfSpeech".into(), json!(900));
        spans.insert("t5FinalTranscript".into(), json!(1160));
        spans.insert("t8ToolDone".into(), json!(1080));
        db.record_turn_metrics("t1", &Value::Object(spans)).unwrap();
    }
    let models = Arc::new(kivo_runtime::models::Models::new(
        r.rig.screenshots.join("models"),
        Arc::clone(&r.rig.core),
        r.rig.infer.clone(),
    ));
    let system = kivo_runtime::system_rpc::SystemRpc {
        engine: Arc::clone(&r.rig.engine),
        models,
        system: r.rig.system.clone(),
        processes: r.rig.processes.clone(),
        browser: None,
        discovery: None,
        open_url: {
            let opened = Arc::clone(&r.rig.opened);
            Arc::new(move |url: &str| {
                opened.lock().unwrap().push(url.to_owned());
                Ok(())
            })
        },
    };
    let perf = system
        .call("performance.status", Value::Null)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(perf["memoryMb"], 120, "KIVO's processes only: {perf}");
    assert_eq!(perf["gpu"], false);
    assert_eq!(perf["latency"]["wakeToChime"], 140);
    assert_eq!(perf["latency"]["speechToText"], 260);
    assert_eq!(perf["latency"]["commandDone"], 180);

    let checks = system
        .call("diagnostics.run", Value::Null)
        .await
        .unwrap()
        .unwrap();
    let ids: Vec<&str> = checks
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["id"].as_str())
        .collect();
    assert_eq!(
        ids,
        [
            "microphone",
            "wake",
            "speech",
            "echo",
            "brains",
            "extension",
            "audit"
        ]
    );
    // Only web pages open.
    system
        .call(
            "system.openUrl",
            json!({"url": "https://github.com/MadBlast0/K.I.V.O"}),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(
        system
            .call(
                "system.openUrl",
                json!({"url": "file:///C:/Windows/system32/calc.exe"})
            )
            .await
            .unwrap()
            .is_err()
    );
    assert_eq!(
        *r.rig.opened.lock().unwrap(),
        ["https://github.com/MadBlast0/K.I.V.O"]
    );
    let audit = &checks[6];
    assert_eq!(audit["ok"], true, "{audit}");
    let extension = &checks[5];
    assert_eq!(extension["ok"], false);
    assert_eq!(extension["fix"], "permissions");
    r.stop().await;
}

/// Settings → General: Reset all settings keeps setup, memory and connected brains.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reset_keeps_what_isnt_a_preference() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start();
    r.rig.core.update_config(|c| {
        c.general.onboarded = true;
        c.sounds.volume = 15;
        c.overlay.hide_after_seconds = 0;
    });
    let memory = r.rig.engine.memory().unwrap();
    memory
        .remember("Standups are at 9:30", &[], None, false)
        .unwrap();
    let fresh = kivo_runtime::settings_file::reset(&r.rig.core.config());
    r.rig.core.update_config(|c| *c = fresh);
    let c = r.rig.core.config();
    assert!(c.general.onboarded);
    assert_eq!(
        c.sounds.volume,
        kivo_core::KivoConfig::default().sounds.volume
    );
    assert_eq!(c.overlay.hide_after_seconds, 4);
    assert_eq!(memory.overview().notes.len(), 1, "memory is kept");
    r.stop().await;
}
