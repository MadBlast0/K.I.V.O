//! Voice enrollment data (VOICE §5, SECURITY §5): which clips the owner recorded and their
//! speaker profile. Clip files and the profile's embeddings are encrypted by the caller
//! (`Secrets::protect`, DPAPI on Windows) before they reach disk; this module keeps the rows.
//! "Delete voice data" removes all of it in one go.

use crate::db::{Database, DbError};
use rusqlite::{OptionalExtension, params};
use std::time::{SystemTime, UNIX_EPOCH};

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoiceClip {
    pub id: i64,
    pub prompt: String,
    /// The encrypted file's name in the voice folder.
    pub file: String,
    pub created_at: i64,
}

/// The owner's speaker profile as stored: the model it was built with, the match threshold, and
/// the encrypted embeddings.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredSpeakerProfile {
    pub model_id: String,
    pub threshold: f32,
    pub embeddings: Vec<u8>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Database {
    pub fn add_voice_clip(&self, prompt: &str, file: &str) -> Result<VoiceClip, DbError> {
        let owner = self.owner_profile()?.to_string();
        let created_at = now();
        self.connection().execute(
            "INSERT INTO voice_clips (profile_id, prompt, file, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![owner, prompt, file, created_at],
        )?;
        Ok(VoiceClip {
            id: self.connection().last_insert_rowid(),
            prompt: prompt.into(),
            file: file.into(),
            created_at,
        })
    }

    pub fn voice_clips(&self) -> Result<Vec<VoiceClip>, DbError> {
        let owner = self.owner_profile()?.to_string();
        let mut stmt = self.connection().prepare(
            "SELECT id, prompt, file, created_at FROM voice_clips WHERE profile_id = ?1 ORDER BY id",
        )?;
        let clips = stmt
            .query_map([owner], |r| {
                Ok(VoiceClip {
                    id: r.get(0)?,
                    prompt: r.get(1)?,
                    file: r.get(2)?,
                    created_at: r.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(clips)
    }

    pub fn speaker_profile(&self) -> Result<Option<StoredSpeakerProfile>, DbError> {
        let owner = self.owner_profile()?.to_string();
        Ok(self
            .connection()
            .query_row(
                "SELECT model_id, threshold, embeddings, created_at, updated_at
                 FROM speaker_profiles WHERE profile_id = ?1",
                [owner],
                |r| {
                    Ok(StoredSpeakerProfile {
                        model_id: r.get(0)?,
                        #[allow(clippy::cast_possible_truncation, reason = "0–1")]
                        threshold: r.get::<_, f64>(1)? as f32,
                        embeddings: r.get(2)?,
                        created_at: r.get(3)?,
                        updated_at: r.get(4)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn save_speaker_profile(
        &self,
        model_id: &str,
        threshold: f32,
        embeddings: &[u8],
    ) -> Result<(), DbError> {
        let owner = self.owner_profile()?.to_string();
        let t = now();
        self.connection().execute(
            "INSERT INTO speaker_profiles (profile_id, model_id, threshold, embeddings, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT(profile_id) DO UPDATE SET model_id = excluded.model_id,
                 threshold = excluded.threshold, embeddings = excluded.embeddings,
                 updated_at = excluded.updated_at",
            params![owner, model_id, f64::from(threshold), embeddings, t],
        )?;
        Ok(())
    }

    /// Removes every clip row and the profile; returns the clip files for the caller to delete.
    pub fn delete_voice_data(&self) -> Result<Vec<String>, DbError> {
        let files = self
            .voice_clips()?
            .into_iter()
            .map(|c| c.file)
            .collect::<Vec<_>>();
        let owner = self.owner_profile()?.to_string();
        self.connection()
            .execute("DELETE FROM voice_clips WHERE profile_id = ?1", [&owner])?;
        self.connection().execute(
            "DELETE FROM speaker_profiles WHERE profile_id = ?1",
            [&owner],
        )?;
        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clips_and_the_profile_are_kept_and_deleted_together() {
        let db = Database::in_memory().unwrap();
        assert!(db.speaker_profile().unwrap().is_none());
        db.add_voice_clip("Hey Kivo", "clip-1.bin").unwrap();
        db.add_voice_clip("Hey Kivo, open Chrome", "clip-2.bin")
            .unwrap();
        db.save_speaker_profile("campplus", 0.6, &[1, 2, 3])
            .unwrap();
        db.save_speaker_profile("campplus", 0.65, &[4, 5]).unwrap();
        let profile = db.speaker_profile().unwrap().unwrap();
        assert_eq!(profile.embeddings, [4, 5]);
        assert!((profile.threshold - 0.65).abs() < 1e-6);
        assert_eq!(db.voice_clips().unwrap().len(), 2);
        assert_eq!(
            db.delete_voice_data().unwrap(),
            ["clip-1.bin", "clip-2.bin"]
        );
        assert!(db.voice_clips().unwrap().is_empty());
        assert!(db.speaker_profile().unwrap().is_none());
    }
}
