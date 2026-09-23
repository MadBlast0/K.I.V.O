//! Wake words (VOICE §4, VOICE-15): the built-in "Hey Kivo" and the user's own words, each
//! spotted by the keyword model from its typed phrase. Up to five can be enabled at once
//! (VOICE-18); the built-in word can be disabled but never deleted.

use crate::db::{Database, DbError};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// The built-in word's id.
pub const HEY_KIVO: &str = "hey-kivo";
/// VOICE-18: enabled words at once (a CPU budget).
pub const MAX_ENABLED: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Quality {
    Good,
    Fair,
    Risky,
}

impl Quality {
    fn as_str(self) -> &'static str {
        match self {
            Self::Good => "good",
            Self::Fair => "fair",
            Self::Risky => "risky",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "good" => Self::Good,
            "fair" => Self::Fair,
            _ => Self::Risky,
        }
    }
}

/// A false-alarm test result: how many times the word fired on background speech.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FalseAlarmTest {
    pub minutes: f32,
    pub false_alarms: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WakeWord {
    pub id: String,
    pub phrase: String,
    /// How to say it, when the typed spelling misleads the model ("Kee-vo").
    pub phonetic: Option<String>,
    pub enabled: bool,
    pub built_in: bool,
    /// 0–1: higher hears it more easily, with more false alarms.
    pub sensitivity: f32,
    /// Recorded samples (files in the voice folder).
    pub samples: Vec<String>,
    pub quality: Quality,
    pub false_alarm_test: Option<FalseAlarmTest>,
    pub created_at: i64,
}

impl WakeWord {
    /// What the spotter listens for: the phonetic spelling if there is one.
    pub fn spoken(&self) -> &str {
        self.phonetic
            .as_deref()
            .filter(|p| !p.trim().is_empty())
            .unwrap_or(&self.phrase)
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WakeError {
    #[error("\"Hey Kivo\" can be turned off, but not deleted")]
    BuiltIn,
    #[error("there's no wake word with that name")]
    Unknown,
    #[error("up to {MAX_ENABLED} wake words can be on at once; turn one off first")]
    TooMany,
    #[error("{0}")]
    Db(String),
}

impl From<DbError> for WakeError {
    fn from(e: DbError) -> Self {
        Self::Db(e.to_string())
    }
}

impl From<rusqlite::Error> for WakeError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Db(e.to_string())
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<WakeWord> {
    let samples: String = r.get("samples")?;
    let fa: Option<String> = r.get("fa_test")?;
    let quality: String = r.get("quality")?;
    Ok(WakeWord {
        id: r.get("id")?,
        phrase: r.get("phrase")?,
        phonetic: r.get("phonetic")?,
        enabled: r.get::<_, i64>("enabled")? == 1,
        built_in: r.get::<_, i64>("built_in")? == 1,
        #[allow(clippy::cast_possible_truncation, reason = "0–1")]
        sensitivity: r.get::<_, f64>("sensitivity")? as f32,
        samples: serde_json::from_str(&samples).unwrap_or_default(),
        quality: Quality::parse(&quality),
        false_alarm_test: fa.and_then(|f| serde_json::from_str(&f).ok()),
        created_at: r.get("created_at")?,
    })
}

impl Database {
    /// Every wake word, the built-in one first (it is created on first use).
    pub fn wake_words(&self) -> Result<Vec<WakeWord>, WakeError> {
        self.ensure_hey_kivo()?;
        let mut stmt = self
            .connection()
            .prepare("SELECT * FROM wake_words ORDER BY built_in DESC, created_at, phrase")?;
        let words = stmt.query_map([], row)?.collect::<Result<Vec<_>, _>>()?;
        Ok(words)
    }

    pub fn wake_word(&self, id: &str) -> Result<Option<WakeWord>, WakeError> {
        self.ensure_hey_kivo()?;
        Ok(self
            .connection()
            .query_row("SELECT * FROM wake_words WHERE id = ?1", [id], row)
            .optional()?)
    }

    fn ensure_hey_kivo(&self) -> Result<(), WakeError> {
        let owner = self.owner_profile()?.to_string();
        self.connection().execute(
            "INSERT OR IGNORE INTO wake_words
                 (id, profile_id, phrase, engine, enabled, built_in, sensitivity, quality, created_at)
             VALUES (?1, ?2, 'Hey Kivo', 'kws', 1, 1, 0.5, 'good', ?3)",
            params![HEY_KIVO, owner, now()],
        )?;
        Ok(())
    }

    /// Adds or updates a user's wake word. Enabling it respects the limit of five.
    pub fn save_wake_word(&self, word: &WakeWord) -> Result<(), WakeError> {
        self.ensure_hey_kivo()?;
        if word.enabled {
            let others = self
                .wake_words()?
                .iter()
                .filter(|w| w.enabled && w.id != word.id)
                .count();
            if others >= MAX_ENABLED {
                return Err(WakeError::TooMany);
            }
        }
        let owner = self.owner_profile()?.to_string();
        let fa = word
            .false_alarm_test
            .as_ref()
            .and_then(|f| serde_json::to_string(f).ok());
        let samples = serde_json::to_string(&word.samples).unwrap_or_else(|_| "[]".into());
        self.connection().execute(
            "INSERT INTO wake_words
                 (id, profile_id, phrase, phonetic, engine, enabled, built_in, sensitivity,
                  samples, quality, fa_test, created_at)
             VALUES (?1, ?2, ?3, ?4, 'kws', ?5, 0, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
                 phrase = CASE WHEN built_in = 1 THEN phrase ELSE excluded.phrase END,
                 phonetic = excluded.phonetic, enabled = excluded.enabled,
                 sensitivity = excluded.sensitivity, samples = excluded.samples,
                 quality = CASE WHEN built_in = 1 THEN quality ELSE excluded.quality END,
                 fa_test = excluded.fa_test",
            params![
                word.id,
                owner,
                word.phrase,
                word.phonetic,
                i64::from(word.enabled),
                f64::from(word.sensitivity.clamp(0.0, 1.0)),
                samples,
                word.quality.as_str(),
                fa,
                if word.created_at > 0 {
                    word.created_at
                } else {
                    now()
                },
            ],
        )?;
        Ok(())
    }

    pub fn set_wake_word_enabled(&self, id: &str, enabled: bool) -> Result<(), WakeError> {
        let mut word = self.wake_word(id)?.ok_or(WakeError::Unknown)?;
        word.enabled = enabled;
        self.save_wake_word(&word)
    }

    pub fn set_wake_word_sensitivity(&self, id: &str, sensitivity: f32) -> Result<(), WakeError> {
        let mut word = self.wake_word(id)?.ok_or(WakeError::Unknown)?;
        word.sensitivity = sensitivity;
        self.save_wake_word(&word)
    }

    /// Deletes a user's word; returns its sample files for the caller to remove.
    pub fn delete_wake_word(&self, id: &str) -> Result<Vec<String>, WakeError> {
        let word = self.wake_word(id)?.ok_or(WakeError::Unknown)?;
        if word.built_in {
            return Err(WakeError::BuiltIn);
        }
        self.connection()
            .execute("DELETE FROM wake_words WHERE id = ?1", [id])?;
        Ok(word.samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(id: &str, phrase: &str) -> WakeWord {
        WakeWord {
            id: id.into(),
            phrase: phrase.into(),
            phonetic: None,
            enabled: true,
            built_in: false,
            sensitivity: 0.5,
            samples: vec![],
            quality: Quality::Good,
            false_alarm_test: None,
            created_at: 0,
        }
    }

    #[test]
    fn hey_kivo_is_built_in_and_can_be_turned_off_but_not_deleted() {
        let db = Database::in_memory().unwrap();
        let words = db.wake_words().unwrap();
        assert_eq!(words.len(), 1);
        assert_eq!((words[0].id.as_str(), words[0].built_in), (HEY_KIVO, true));
        assert_eq!(db.delete_wake_word(HEY_KIVO), Err(WakeError::BuiltIn));
        db.set_wake_word_enabled(HEY_KIVO, false).unwrap();
        assert!(!db.wake_word(HEY_KIVO).unwrap().unwrap().enabled);
        // Its phrase can't be changed.
        let mut renamed = db.wake_word(HEY_KIVO).unwrap().unwrap();
        renamed.phrase = "Hi Bob".into();
        db.save_wake_word(&renamed).unwrap();
        assert_eq!(db.wake_word(HEY_KIVO).unwrap().unwrap().phrase, "Hey Kivo");
    }

    #[test]
    fn custom_words_are_saved_updated_and_deleted() {
        let db = Database::in_memory().unwrap();
        let mut w = word("computer", "Hey Computer");
        w.samples = vec!["computer-1.bin".into()];
        w.false_alarm_test = Some(FalseAlarmTest {
            minutes: 3.0,
            false_alarms: 0,
        });
        db.save_wake_word(&w).unwrap();
        db.set_wake_word_sensitivity("computer", 0.8).unwrap();
        let saved = db.wake_word("computer").unwrap().unwrap();
        assert!((saved.sensitivity - 0.8).abs() < 1e-6);
        assert_eq!(saved.false_alarm_test, w.false_alarm_test);
        assert_eq!(db.delete_wake_word("computer").unwrap(), ["computer-1.bin"]);
        assert_eq!(db.wake_word("computer").unwrap(), None);
        assert_eq!(db.delete_wake_word("computer"), Err(WakeError::Unknown));
    }

    #[test]
    fn at_most_five_are_on_at_once() {
        let db = Database::in_memory().unwrap();
        for i in 0..4 {
            db.save_wake_word(&word(&format!("w{i}"), &format!("Word {i}")))
                .unwrap();
        }
        // Hey Kivo + 4 = 5.
        assert_eq!(
            db.save_wake_word(&word("w4", "Word four")),
            Err(WakeError::TooMany)
        );
        let mut off = word("w4", "Word four");
        off.enabled = false;
        db.save_wake_word(&off).unwrap();
        assert_eq!(
            db.set_wake_word_enabled("w4", true),
            Err(WakeError::TooMany)
        );
        db.set_wake_word_enabled(HEY_KIVO, false).unwrap();
        db.set_wake_word_enabled("w4", true).unwrap();
    }

    #[test]
    fn a_phonetic_spelling_is_what_gets_spotted() {
        let mut w = word("k", "Hey Kivo");
        assert_eq!(w.spoken(), "Hey Kivo");
        w.phonetic = Some("Hey Keevo".into());
        assert_eq!(w.spoken(), "Hey Keevo");
    }
}
