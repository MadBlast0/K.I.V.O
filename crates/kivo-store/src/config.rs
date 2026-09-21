//! Reading and writing `kivo.toml` (ARCHITECTURE §5, DISTRIBUTION §2). Loading never fails: a
//! missing file gets defaults, an old file is migrated forward after a backup, and a broken or
//! newer file is left untouched while KIVO runs on defaults and tells the user why.

use kivo_core::config::{CONFIG_SCHEMA_VERSION, KivoConfig, PermissionMode};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use toml::Table;

/// Upgrades a settings table from schema version N to N+1 (index 0 upgrades 1 → 2).
pub type Migration = fn(&mut Table) -> Result<(), String>;

/// Forward migrations, in order. Schema 1 is the first release, so there are none yet.
pub const MIGRATIONS: &[Migration] = &[];

/// The settings plus anything the user should be told about how they were loaded.
#[derive(Debug)]
pub struct Loaded {
    pub config: KivoConfig,
    pub notice: Option<Notice>,
}

/// Something about loading the settings that the Control Center should mention once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// First run: defaults were written.
    Created,
    /// Upgraded from an older version; the old file was kept at `backup`.
    Migrated { from: u32, backup: PathBuf },
    /// The file couldn't be used. It was kept at `kept_at` and KIVO is running on defaults.
    Invalid { reason: String, kept_at: PathBuf },
    /// Written by a newer KIVO. It's untouched and KIVO is running on defaults until updated.
    FromNewerVersion { version: u32 },
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("couldn't read or write the settings file: {0}")]
    Io(#[from] std::io::Error),
    #[error("couldn't write the settings: {0}")]
    Serialize(#[from] toml::ser::Error),
}

pub fn load(path: &Path) -> Result<Loaded, ConfigError> {
    load_with(path, MIGRATIONS)
}

/// `load` with an explicit migration list (tests supply their own).
pub fn load_with(path: &Path, migrations: &[Migration]) -> Result<Loaded, ConfigError> {
    let current = u32::try_from(migrations.len())
        .map_or(u32::MAX, |n| n + 1)
        .max(CONFIG_SCHEMA_VERSION);
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let config = KivoConfig::default();
            save(path, &config)?;
            return Ok(Loaded {
                config,
                notice: Some(Notice::Created),
            });
        }
        Err(e) => return Err(e.into()),
    };

    let mut table = match text.parse::<Table>() {
        Ok(t) => t,
        Err(e) => return set_aside(path, format!("it isn't valid TOML ({})", e.message())),
    };
    let version = match table.get("schema-version") {
        None => 1,
        Some(v) => match v.as_integer().and_then(|n| u32::try_from(n).ok()) {
            Some(n) if n >= 1 => n,
            _ => return set_aside(path, "schema-version isn't a positive whole number".into()),
        },
    };
    if version > current {
        tracing::warn!(
            version,
            current,
            "settings were written by a newer KIVO; using defaults"
        );
        return Ok(Loaded {
            config: KivoConfig::default(),
            notice: Some(Notice::FromNewerVersion { version }),
        });
    }

    let mut backup = None;
    if version < current {
        let kept = sibling(path, &format!("v{version}.bak"));
        std::fs::copy(path, &kept)?;
        for (i, migrate) in migrations.iter().enumerate().skip((version - 1) as usize) {
            if let Err(reason) = migrate(&mut table) {
                return set_aside(
                    path,
                    format!("upgrading from version {} failed: {reason}", i + 1),
                );
            }
        }
        table.insert(
            "schema-version".into(),
            toml::Value::Integer(i64::from(current)),
        );
        backup = Some(kept);
    }

    let mut config: KivoConfig = match table.try_into() {
        Ok(c) => c,
        Err(e) => return set_aside(path, e.message().to_owned()),
    };
    if let Err(problems) = config.validate() {
        let list: Vec<String> = problems.iter().map(ToString::to_string).collect();
        return set_aside(path, list.join("; "));
    }
    config.schema_version = current;
    // Bypass is time-limited and never survives a restart (SECURITY §1.1).
    if config.permissions.mode == PermissionMode::Bypass {
        tracing::info!("Bypass permissions doesn't persist across restarts; starting in Auto");
        config.permissions.mode = PermissionMode::Auto;
    }

    let notice = match backup {
        Some(backup) => {
            save(path, &config)?;
            Some(Notice::Migrated {
                from: version,
                backup,
            })
        }
        None => None,
    };
    Ok(Loaded { config, notice })
}

/// Writes the settings atomically: a crash mid-write leaves the old file intact.
pub fn save(path: &Path, config: &KivoConfig) -> Result<(), ConfigError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = format!(
        "# KIVO settings. Edit while KIVO is closed, or use Settings in the app.\n\n{}",
        toml::to_string_pretty(config)?
    );
    let tmp = sibling(path, "tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Moves an unusable file aside (never deletes it), writes defaults, and reports why.
fn set_aside(path: &Path, reason: String) -> Result<Loaded, ConfigError> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let kept_at = sibling(path, &format!("invalid-{stamp}"));
    std::fs::rename(path, &kept_at)?;
    tracing::warn!(%reason, kept_at = %kept_at.display(), "settings file couldn't be used; starting from defaults");
    let config = KivoConfig::default();
    save(path, &config)?;
    Ok(Loaded {
        config,
        notice: Some(Notice::Invalid { reason, kept_at }),
    })
}

/// `kivo.toml` → `kivo.toml.<suffix>` in the same folder.
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(suffix);
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kivo_core::config::OverlayPosition;

    fn temp() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config").join("kivo.toml");
        (dir, path)
    }

    #[test]
    fn first_run_writes_defaults() {
        let (_d, path) = temp();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.notice, Some(Notice::Created));
        assert_eq!(loaded.config, KivoConfig::default());
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("schema-version = 1")
        );
        assert!(
            load(&path).unwrap().notice.is_none(),
            "second load is quiet"
        );
    }

    #[test]
    fn saved_settings_come_back_identical() {
        let (_d, path) = temp();
        let mut c = KivoConfig::default();
        c.permissions.mode = PermissionMode::Ask;
        c.overlay.position = OverlayPosition::BottomCenter;
        c.voice.push_to_talk = vec!["Ctrl".into(), "Alt".into(), "K".into()];
        save(&path, &c).unwrap();
        assert_eq!(load(&path).unwrap().config, c);
    }

    #[test]
    fn bypass_never_survives_a_restart() {
        let (_d, path) = temp();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "[permissions]
mode = \"bypass\"
",
        )
        .unwrap();
        assert_eq!(
            load(&path).unwrap().config.permissions.mode,
            PermissionMode::Auto
        );
    }

    #[test]
    fn a_partial_file_fills_in_defaults() {
        let (_d, path) = temp();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "schema-version = 1\n[sounds]\nvolume = 40\n").unwrap();
        let c = load(&path).unwrap().config;
        assert_eq!(c.sounds.volume, 40);
        assert_eq!(c.permissions.mode, PermissionMode::Auto);
    }

    #[test]
    fn a_broken_file_is_kept_and_defaults_are_used() {
        let (_d, path) = temp();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "[sounds\nvolume = ").unwrap();
        let loaded = load(&path).unwrap();
        let Some(Notice::Invalid { reason, kept_at }) = loaded.notice else {
            panic!("expected Invalid")
        };
        assert!(reason.contains("TOML"), "{reason}");
        assert_eq!(
            std::fs::read_to_string(kept_at).unwrap(),
            "[sounds\nvolume = ",
            "the user's file is preserved"
        );
        assert_eq!(loaded.config, KivoConfig::default());
    }

    #[test]
    fn values_out_of_range_or_unknown_keys_are_rejected_not_guessed() {
        for bad in [
            "[sounds]\nvolume = 300\n",
            "[sounds]\nvolumee = 30\n",
            "[voice]\npush-to-talk = []\n",
        ] {
            let (_d, path) = temp();
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, bad).unwrap();
            assert!(
                matches!(load(&path).unwrap().notice, Some(Notice::Invalid { .. })),
                "{bad}"
            );
        }
    }

    #[test]
    fn a_file_from_a_newer_kivo_is_left_untouched() {
        let (_d, path) = temp();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "schema-version = 99\n").unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(
            loaded.notice,
            Some(Notice::FromNewerVersion { version: 99 })
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "schema-version = 99\n"
        );
    }

    #[test]
    fn older_files_are_backed_up_then_migrated_in_order() {
        // Two synthetic migrations: 1 → 2 renames a key, 2 → 3 sets a value.
        fn rename_volume(t: &mut Table) -> Result<(), String> {
            let sounds = t
                .get_mut("sounds")
                .and_then(toml::Value::as_table_mut)
                .ok_or("no sounds")?;
            let v = sounds.remove("loudness").ok_or("no loudness")?;
            sounds.insert("volume".into(), v);
            Ok(())
        }
        fn quiet_thinking(t: &mut Table) -> Result<(), String> {
            let sounds = t
                .get_mut("sounds")
                .and_then(toml::Value::as_table_mut)
                .ok_or("no sounds")?;
            sounds.insert("thinking-cue".into(), toml::Value::Boolean(false));
            Ok(())
        }
        let (_d, path) = temp();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let old = "schema-version = 1\n[sounds]\nloudness = 55\n";
        std::fs::write(&path, old).unwrap();
        let loaded = load_with(&path, &[rename_volume, quiet_thinking]).unwrap();
        let Some(Notice::Migrated { from: 1, backup }) = loaded.notice else {
            panic!("expected Migrated")
        };
        assert_eq!(std::fs::read_to_string(backup).unwrap(), old);
        assert_eq!(loaded.config.sounds.volume, 55);
        assert_eq!(loaded.config.schema_version, 3);
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("schema-version = 3")
        );
    }
}
