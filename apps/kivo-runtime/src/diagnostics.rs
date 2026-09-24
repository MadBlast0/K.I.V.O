//! The diagnostics bundle (ARCHITECTURE §6, plan §109, ARCH-40): versions, the OS, the hardware,
//! which capabilities are on, how the brains and speech engines are, performance numbers, the
//! checks and recent errors. It is made on this PC, shown to the user to read before anything is
//! saved, and holds no secrets or content: no keys, no transcripts or answers, no file contents.
//! Errors come from KIVO's own logs (already redacted on the way to disk), keep only their
//! message, and go through the secret filter and a path scrub again.

use kivo_core::capability::Capability;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// What a bundle says it is.
pub const KIND: &str = "kivo-diagnostics";
pub const VERSION: u64 = 1;
/// Most distinct errors a bundle lists.
const MAX_ERRORS: usize = 50;
/// How many of the newest log files are read.
const LOG_FILES: usize = 2;

/// One distinct warning or error from the logs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogEntry {
    pub level: String,
    pub target: String,
    pub message: String,
    /// The last time it happened.
    pub last: String,
    pub count: u32,
}

/// The warnings and errors in the newest log files of `dir`, newest last, repeats counted once.
/// Only each line's level, source and message are kept; the other fields (which may hold names or
/// paths) are dropped.
pub fn recent_errors(dir: &Path, home: Option<&Path>) -> Vec<LogEntry> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|it| {
            it.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("kivo") && n.ends_with(".log"))
                })
                .collect()
        })
        .unwrap_or_default();
    // Daily files are named kivo.YYYY-MM-DD.log, so the name orders them.
    files.sort();
    let newest = &files[files.len().saturating_sub(LOG_FILES)..];
    let mut seen: BTreeMap<(String, String, String), LogEntry> = BTreeMap::new();
    let mut order: Vec<(String, String, String)> = Vec::new();
    for file in newest {
        let Ok(body) = std::fs::read_to_string(file) else {
            continue;
        };
        for line in body.lines() {
            let Ok(v) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            let level = v["level"].as_str().unwrap_or_default();
            if level != "WARN" && level != "ERROR" {
                continue;
            }
            let target = v["target"].as_str().unwrap_or_default().to_owned();
            let message = scrub(v["fields"]["message"].as_str().unwrap_or_default(), home);
            let key = (level.to_owned(), target.clone(), message.clone());
            let ts = v["timestamp"].as_str().unwrap_or_default().to_owned();
            if let Some(entry) = seen.get_mut(&key) {
                entry.count += 1;
                entry.last = ts;
                order.retain(|k| k != &key);
            } else {
                seen.insert(
                    key.clone(),
                    LogEntry {
                        level: level.to_owned(),
                        target,
                        message,
                        last: ts,
                        count: 1,
                    },
                );
            }
            order.push(key);
        }
    }
    let skip = order.len().saturating_sub(MAX_ERRORS);
    order
        .into_iter()
        .skip(skip)
        .filter_map(|k| seen.remove(&k))
        .collect()
}

/// Masks secrets, and the user's own folder in paths (a user name is personal).
pub fn scrub(text: &str, home: Option<&Path>) -> String {
    let logged = kivo_store::logging::Redactor::new(false)
        .redact(text)
        .into_owned();
    let mut out = kivo_security::classify::redact_secrets(&logged);
    if let Some(home) = home.and_then(Path::to_str).filter(|h| !h.is_empty()) {
        for form in [
            home.to_owned(),
            home.replace('\\', "/"),
            home.replace('\\', "\\\\"),
        ] {
            out = out.replace(&form, "%USERPROFILE%");
        }
    }
    out
}

/// What goes into a bundle; the caller gathers it.
pub struct Parts<'a> {
    pub config: &'a kivo_core::KivoConfig,
    pub platform: &'a kivo_platform::Capabilities,
    pub machine: Option<&'a kivo_platform::SystemSnapshot>,
    pub brains: &'a [crate::brains::BrainView],
    pub checks: &'a [crate::system_rpc::Check],
    pub performance: Value,
    pub errors: Vec<LogEntry>,
    pub created: String,
    pub home: Option<&'a Path>,
}

/// Builds the bundle.
pub fn bundle(parts: &Parts<'_>) -> Value {
    let config = parts.config;
    let capabilities: BTreeMap<String, bool> = Capability::ALL
        .iter()
        .map(|&c| (kivo_core::text::key_of(&c), config.capabilities.enabled(c)))
        .collect();
    let hardware = parts.machine.map(|m| {
        json!({
            "cpu": m.cpu_name,
            "logicalCpus": m.logical_cpus,
            "ramMb": m.ram_mb,
            "gpus": m.gpus.iter().map(|g| json!({ "name": g.name, "vramMb": g.vram_mb })).collect::<Vec<_>>(),
            "onBattery": m.on_battery,
        })
    });
    // Brains: what kind they are and whether they answer; never keys or conversations.
    let brains: Vec<Value> = parts
        .brains
        .iter()
        .map(|b| {
            json!({
                "id": b.id,
                "kind": b.kind,
                "privacy": b.privacy,
                "enabled": b.enabled,
                "health": health(&b.health, parts.home),
                "keySaved": b.has_key,
            })
        })
        .collect();
    let checks: Vec<Value> = parts
        .checks
        .iter()
        .map(|c| json!({ "id": c.id, "ok": c.ok, "detail": scrub(&c.detail, parts.home) }))
        .collect();
    let errors: Vec<Value> = parts
        .errors
        .iter()
        .map(|e| {
            json!({
                "level": e.level, "source": e.target, "message": e.message,
                "last": e.last, "count": e.count,
            })
        })
        .collect();
    json!({
        "kind": KIND,
        "version": VERSION,
        "created": parts.created,
        "kivo": env!("CARGO_PKG_VERSION"),
        "os": {
            "version": parts.platform.os_version,
            "build": parts.platform.os_build,
            "echoCancellation": parts.platform.os_echo_cancellation,
            "mica": parts.platform.mica,
            "npu": parts.platform.npu,
        },
        "hardware": hardware,
        "settings": {
            "productMode": config.general.product_mode,
            "privacy": config.privacy.mode,
            "permissions": config.permissions.mode,
            "performanceProfile": config.performance.profile,
            "lowMemoryMode": config.general.low_memory_mode,
            "speech": { "stt": config.voice.stt_engine, "tts": config.voice.tts_engine },
        },
        "capabilities": capabilities,
        "brains": brains,
        "checks": checks,
        "performance": parts.performance,
        "recentErrors": errors,
    })
}

fn health(health: &kivo_brain::Health, home: Option<&Path>) -> Value {
    match health {
        kivo_brain::Health::Unreachable { reason } => {
            json!({ "unreachable": scrub(reason, home) })
        }
        other => json!(other),
    }
}

/// Saves a reviewed bundle to `dir` as "KIVO diagnostics <date>.json".
pub fn save(dir: &Path, bundle: &Value, date: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let file = (1..)
        .map(|n: u32| {
            dir.join(if n < 2 {
                format!("KIVO diagnostics {date}.json")
            } else {
                format!("KIVO diagnostics {date} ({n}).json")
            })
        })
        .find(|f| !f.exists())
        .unwrap_or_else(|| dir.join("KIVO diagnostics.json"));
    let body = serde_json::to_string_pretty(bundle).map_err(|e| e.to_string())?;
    std::fs::write(&file, body).map_err(|e| e.to_string())?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kivo-diag-{name}-{}-{}",
            std::process::id(),
            kivo_store::brains::now_ms()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn errors_keep_only_the_message_and_count_repeats() {
        let dir = temp("logs");
        let home = Path::new(r"C:\Users\Sam");
        std::fs::write(
            dir.join("kivo.2026-09-22.log"),
            concat!(
                r#"{"timestamp":"2026-09-22T10:00:00Z","level":"ERROR","fields":{"message":"old day"},"target":"kivo_runtime::old"}"#,
                "\n",
            ),
        )
        .unwrap();
        std::fs::write(
            dir.join("kivo.2026-09-23.log"),
            concat!(
                r#"{"timestamp":"2026-09-23T10:00:00Z","level":"INFO","fields":{"message":"fine"},"target":"a"}"#,
                "\n",
                r#"{"timestamp":"2026-09-23T10:00:01Z","level":"WARN","fields":{"message":"couldn't open C:\\Users\\Sam\\notes.txt","path":"C:\\Users\\Sam\\secret plans.docx"},"target":"kivo_tools::files"}"#,
                "\n",
                r#"{"timestamp":"2026-09-23T10:00:02Z","level":"ERROR","fields":{"message":"auth failed with sk-ant-abcdefghijklmnopqrstuvwx"},"target":"kivo_brain"}"#,
                "\n",
                r#"{"timestamp":"2026-09-23T10:00:03Z","level":"WARN","fields":{"message":"couldn't open C:\\Users\\Sam\\notes.txt"},"target":"kivo_tools::files"}"#,
                "\n",
                "not json\n",
            ),
        )
        .unwrap();
        std::fs::write(dir.join("kivo.2026-09-21.log"), "").unwrap();
        let errors = recent_errors(&dir, Some(home));
        assert_eq!(errors.len(), 3, "{errors:#?}");
        assert_eq!(errors[0].message, "old day");
        assert_eq!(errors[1].target, "kivo_brain");
        assert!(
            !errors[1].message.contains("sk-ant"),
            "{}",
            errors[1].message
        );
        let files = &errors[2];
        assert_eq!(files.count, 2);
        assert_eq!(files.last, "2026-09-23T10:00:03Z");
        assert_eq!(files.message, r"couldn't open %USERPROFILE%\notes.txt");
        let all = format!("{errors:?}");
        assert!(!all.contains("secret plans"), "other fields are dropped");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_bundle_has_the_facts_and_no_secrets_or_content() {
        let mut config = kivo_core::KivoConfig::default();
        config.capabilities.set(Capability::Shell, true);
        let platform = kivo_platform::Capabilities {
            os: kivo_platform::OsFamily::Windows,
            os_version: "Windows 11 24H2 (build 26200)".into(),
            os_build: 26200,
            os_echo_cancellation: true,
            mica: true,
            package_identity: false,
            npu: false,
        };
        let machine = kivo_platform::SystemSnapshot {
            cpu_name: "Ryzen 7".into(),
            logical_cpus: 16,
            ram_mb: 32_768,
            ram_free_mb: 16_000,
            cpu_load_percent: 3,
            gpus: Vec::new(),
            on_battery: false,
            battery_percent: None,
            fullscreen_app: false,
            focus_mode: false,
        };
        let checks = [crate::system_rpc::Check {
            id: "microphone".into(),
            ok: true,
            title: "Microphone".into(),
            detail: "USB Headset".into(),
            fix: None,
        }];
        let home = Path::new(r"C:\Users\Sam");
        let b = bundle(&Parts {
            config: &config,
            platform: &platform,
            machine: Some(&machine),
            brains: &[],
            checks: &checks,
            performance: json!({ "memoryMb": 120 }),
            errors: vec![LogEntry {
                level: "WARN".into(),
                target: "x".into(),
                message: "m".into(),
                last: "t".into(),
                count: 1,
            }],
            created: "2026-09-24 10:00".into(),
            home: Some(home),
        });
        assert_eq!(b["kind"], KIND);
        assert_eq!(b["kivo"], env!("CARGO_PKG_VERSION"));
        assert_eq!(b["os"]["build"], 26200);
        assert_eq!(b["hardware"]["logicalCpus"], 16);
        assert_eq!(b["capabilities"]["shell"], true);
        assert_eq!(b["checks"][0]["id"], "microphone");
        assert_eq!(b["recentErrors"][0]["count"], 1);
        assert_eq!(b["performance"]["memoryMb"], 120);

        let dir = temp("save");
        let one = save(&dir, &b, "2026-09-24").unwrap();
        let two = save(&dir, &b, "2026-09-24").unwrap();
        assert_ne!(one, two, "never overwrites");
        let back: Value = serde_json::from_str(&std::fs::read_to_string(&one).unwrap()).unwrap();
        assert_eq!(back, b);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn home_folders_are_scrubbed_in_every_spelling() {
        let home = Path::new(r"C:\Users\Sam");
        assert_eq!(
            scrub(r"C:\Users\Sam\a and C:/Users/Sam/b", Some(home)),
            r"%USERPROFILE%\a and %USERPROFILE%/b"
        );
        assert_eq!(scrub("token=abc123", None), "token=***");
    }
}
