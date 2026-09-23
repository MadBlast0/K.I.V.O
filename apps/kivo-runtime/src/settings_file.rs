//! Settings → General "Export settings", "Import settings" and "Reset all settings" (UX-31).
//!
//! The file holds the settings (`kivo.toml` as JSON), the routines and the memory vault's notes.
//! It never holds secrets: brain keys are only names of Credential Manager entries, so on another
//! PC the brains ask to sign in again. An imported file is the user's own choice but still
//! untrusted input: it is validated like any settings change, Bypass never comes back on, and
//! routines come in without their grants (they ask until saved again, SECURITY §2).

use crate::core::Core;
use crate::engine::Engine;
use kivo_core::KivoConfig;
use kivo_core::config::PermissionMode;
use kivo_core::text;
use serde::Serialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const KIND: &str = "kivo-settings";
const VERSION: u64 = 1;

/// What an import brought in.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Imported {
    pub settings: bool,
    pub routines: usize,
    /// Routines left out (a phrase or shortcut that clashes), by name.
    pub skipped: Vec<String>,
    pub notes: usize,
}

/// Everything in one JSON document.
pub fn export(core: &Core, engine: &Engine) -> Value {
    let routines: Vec<Value> = engine
        .routines()
        .map(|r| {
            r.all()
                .into_iter()
                .map(|(routine, stored)| {
                    let mut body = serde_json::to_value(&routine).unwrap_or(Value::Null);
                    // Grants are this PC's decisions, not part of the routine.
                    if let Some(o) = body.as_object_mut() {
                        o.remove("grants");
                    }
                    json!({ "id": stored.id, "starter": stored.starter, "routine": body })
                })
                .collect()
        })
        .unwrap_or_default();
    let memory = engine.memory().map_or(Value::Null, |m| m.export());
    json!({
        "kind": KIND,
        "version": VERSION,
        "settings": core.config(),
        "routines": routines,
        "memory": memory,
    })
}

/// Writes `bundle` to `KIVO settings <date>.json` in `dir`; returns the file.
pub fn save(dir: &Path, bundle: &Value, utc_offset: i32) -> Result<PathBuf, String> {
    let date = kivo_memory::date::format(kivo_store::brains::now_ms(), utc_offset);
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let file = (1..)
        .map(|n: u32| {
            dir.join(if n < 2 {
                format!("KIVO settings {date}.json")
            } else {
                format!("KIVO settings {date} ({n}).json")
            })
        })
        .find(|f| !f.exists())
        .unwrap_or_else(|| dir.join("KIVO settings.json"));
    let body = serde_json::to_string_pretty(bundle).map_err(|e| e.to_string())?;
    std::fs::write(&file, body).map_err(|e| e.to_string())?;
    Ok(file)
}

/// Restores from a file's contents: the settings (validated), routines (without grants) and
/// memory notes that aren't on this PC yet.
pub fn import(core: &Core, engine: &Engine, content: &str) -> Result<Imported, String> {
    let doc: Value = serde_json::from_str(content).map_err(|_| text::t("settings.importBad"))?;
    if doc["kind"] != KIND {
        return Err(text::t("settings.importBad"));
    }
    if doc["version"].as_u64().is_none_or(|v| v > VERSION) {
        return Err(text::t("settings.importNewer"));
    }
    let mut report = Imported::default();
    if let Some(settings) = doc.get("settings").filter(|s| s.is_object()) {
        let mut config: KivoConfig = serde_json::from_value(settings.clone())
            .map_err(|e| text::tf("settings.importInvalid", &[("problem", &e.to_string())]))?;
        config.validate().map_err(|problems| {
            problems
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        })?;
        // Bypass is switched on each time and never restored (SEC-03).
        if config.permissions.mode == PermissionMode::Bypass {
            config.permissions.mode = PermissionMode::Auto;
        }
        // This PC's own state stays as it is.
        let here = core.config();
        config.general.onboarded = here.general.onboarded;
        config.general.first_close_seen = here.general.first_close_seen;
        config.voice.input_device = here.voice.input_device.clone();
        config.voice.output_device = here.voice.output_device.clone();
        config.overlay.spots = here.overlay.spots.clone();
        core.update_config(|c| *c = config);
        report.settings = true;
    }
    if let (Some(routines), Some(list)) = (engine.routines(), doc["routines"].as_array()) {
        let existing = routines.all();
        for item in list {
            let Ok(mut routine) =
                serde_json::from_value::<kivo_core::routine::Routine>(item["routine"].clone())
            else {
                continue;
            };
            routine.grants.clear();
            // A starter keeps the id it has on this PC (one row per starter).
            if let Some(starter) = &routine.starter
                && let Some((_, stored)) = existing
                    .iter()
                    .find(|(_, s)| s.starter.as_deref() == Some(starter.as_str()))
            {
                routine.id.clone_from(&stored.id);
            }
            let name = routine.name.clone();
            match routines.save(routine, false) {
                Ok(_) => report.routines += 1,
                Err(_) => report.skipped.push(name),
            }
        }
    }
    if let (Some(memory), Some(notes)) = (engine.memory(), doc["memory"]["notes"].as_array()) {
        report.notes = memory.import(notes);
    }
    Ok(report)
}

/// The defaults, keeping what isn't a preference: setup done, the first-close notice seen, and
/// the connected brains (accounts, like memory and conversations, are kept).
pub fn reset(current: &KivoConfig) -> KivoConfig {
    let mut fresh = KivoConfig::default();
    fresh.general.onboarded = current.general.onboarded;
    fresh.general.first_close_seen = current.general.first_close_seen;
    fresh
        .brains
        .connections
        .clone_from(&current.brains.connections);
    fresh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_keeps_setup_and_brains() {
        let mut c = KivoConfig::default();
        c.general.onboarded = true;
        c.sounds.volume = 20;
        c.brains
            .connections
            .push(kivo_core::config::BrainConnection {
                id: "anthropic".into(),
                name: String::new(),
                base_url: String::new(),
                local: false,
                key: "secret://kivo/anthropic/key".into(),
                enabled: true,
            });
        let fresh = reset(&c);
        assert!(fresh.general.onboarded);
        assert_eq!(fresh.sounds.volume, KivoConfig::default().sounds.volume);
        assert_eq!(fresh.brains.connections.len(), 1);
    }

    #[test]
    fn exports_are_saved_with_a_dated_name() {
        let dir = tempfile::tempdir().unwrap();
        let a = save(dir.path(), &json!({"kind": KIND}), 0).unwrap();
        let b = save(dir.path(), &json!({"kind": KIND}), 0).unwrap();
        assert_ne!(a, b);
        assert!(
            a.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("KIVO settings ")
        );
        assert!(b.to_string_lossy().ends_with("(2).json"));
    }
}
