//! Hands-free listening in the runtime (VOICE §4, CAPABILITIES §1). The listener listens for wake
//! words only while:
//!
//! - the "Microphone listening" capability is on (off until the user turns it on);
//! - the keyword model is on this PC (turning the capability on is the user's choice to have it,
//!   so that starts its download);
//! - listening isn't paused.
//!
//! The words are the enabled wake words ("Hey Kivo" and the user's own, VOICE-15) plus the stop
//! words, which count only while KIVO is busy (VOICE-19). Whatever changes one of those (settings,
//! models, wake words, the session) re-applies them.

use crate::core::Core;
use crate::models::Models;
use crate::voice::{HandsFree, Listener};
use kivo_core::{Capability, SessionState};
use kivo_store::Database;
use kivo_store::models::KEYWORD_SPOTTER;
use kivo_voice::kws::Keyword;
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

/// The stop words, stricter than wake words: a false "stop" interrupts the user's answer.
const STOP_SENSITIVITY: f32 = 0.35;
/// How people (and voices) say "Kivo": the keyword model spells it several ways, so the built-in
/// word listens for each (same id).
const HEY_KIVO_VARIANTS: [&str; 4] = ["Hey Kivo", "Hey Keyvo", "Hey Kevo", "Hey Kiva"];

/// The keywords one wake word listens for.
pub fn keywords_for(word: &kivo_store::wake::WakeWord) -> Vec<Keyword> {
    let custom_spelling = word
        .phonetic
        .as_deref()
        .is_some_and(|p| !p.trim().is_empty());
    if word.built_in && !custom_spelling {
        HEY_KIVO_VARIANTS
            .iter()
            .map(|v| Keyword::new(&word.id, v).with_sensitivity(word.sensitivity))
            .collect()
    } else {
        vec![Keyword::new(&word.id, word.spoken()).with_sensitivity(word.sensitivity)]
    }
}

pub struct Wake {
    core: Arc<Core>,
    models: Arc<Models>,
    db: Arc<Mutex<Database>>,
    listener: Arc<Listener>,
    /// Signalled when wake words are added, edited or removed.
    edited: Notify,
}

/// The stop words KIVO hears while busy.
pub fn stop_words() -> Vec<Keyword> {
    [
        ("kivo-stop", "Kivo stop"),
        ("stop", "stop"),
        ("cancel", "cancel"),
    ]
    .into_iter()
    .map(|(id, phrase)| Keyword::new(id, phrase).with_sensitivity(STOP_SENSITIVITY))
    .collect()
}

impl Wake {
    pub fn new(
        core: Arc<Core>,
        models: Arc<Models>,
        db: Arc<Mutex<Database>>,
        listener: Arc<Listener>,
    ) -> Arc<Self> {
        Arc::new(Self {
            core,
            models,
            db,
            listener,
            edited: Notify::new(),
        })
    }

    /// The wake words changed (VOICE-16): listen for the new set.
    pub fn words_changed(&self) {
        self.edited.notify_one();
    }

    /// What the listener should listen for now, if anything.
    pub fn plan(&self) -> Option<HandsFree> {
        let config = self.core.config();
        if !config.capabilities.enabled(Capability::MicListening)
            || self.core.state().borrow().session == SessionState::Paused
        {
            return None;
        }
        let model_dir = self.models.installed_dir(KEYWORD_SPOTTER)?;
        let words = self
            .db
            .lock()
            .ok()?
            .wake_words()
            .map_err(|e| tracing::warn!(%e, "couldn't read the wake words"))
            .ok()?;
        let wake: Vec<Keyword> = words
            .iter()
            .filter(|w| w.enabled)
            .flat_map(keywords_for)
            .collect();
        if wake.is_empty() {
            return None;
        }
        Some(HandsFree {
            model_dir,
            wake,
            stop: stop_words(),
        })
    }

    /// A key for comparing plans, so the listener reloads only when something changed.
    fn key(plan: Option<&HandsFree>) -> String {
        plan.map_or_else(String::new, |p| {
            let words: Vec<String> = p
                .wake
                .iter()
                .map(|k| format!("{}={}@{:.2}", k.id, k.phrase, k.threshold))
                .collect();
            format!("{}|{}", p.model_dir.display(), words.join(","))
        })
    }

    /// Starts the keyword model's download when hands-free listening is wanted without it.
    fn fetch_model_if_wanted(&self) {
        let wanted = self
            .core
            .config()
            .capabilities
            .enabled(Capability::MicListening);
        if wanted
            && self.models.installed_dir(KEYWORD_SPOTTER).is_none()
            && let Err(e) = self.models.install(KEYWORD_SPOTTER)
        {
            tracing::warn!(%e, "couldn't start the wake-word model download");
        }
    }

    /// Applies the plan whenever something it depends on changes, and tells the listener when
    /// KIVO is busy (for the stop words). Runs until shutdown.
    pub async fn run(self: Arc<Self>) {
        let mut settings = self.core.settings_changed();
        let mut state = self.core.state();
        let mut events = self.core.bus.subscribe();
        let shutdown = self.core.shutdown();
        let mut applied: Option<String> = None;
        loop {
            // Speaker recognition needs each request's whole audio (VOICE-22).
            let config = self.core.config();
            self.listener.set_keep_audio(
                config.voice.speaker_mode != kivo_core::config::SpeakerMode::Off
                    && config.capabilities.enabled(Capability::SpeakerRecognition),
            );
            let session = state.borrow().session;
            self.listener.set_busy(matches!(
                session,
                SessionState::Thinking
                    | SessionState::Acting
                    | SessionState::Speaking
                    | SessionState::AwaitingConfirmation
            ));
            let plan = self.plan();
            let key = Self::key(plan.as_ref());
            if applied.as_ref() != Some(&key) {
                self.listener.set_hands_free(plan);
                applied = Some(key);
            }
            tokio::select! {
                changed = settings.changed() => {
                    if changed.is_err() { return; }
                    self.fetch_model_if_wanted();
                }
                changed = state.changed() => if changed.is_err() { return; },
                // A model finished installing or was removed.
                event = events.recv() => match event {
                    kivo_core::bus::Received::Event(e) if matches!(
                        e.kind,
                        kivo_core::EventKind::System(
                            kivo_core::event::SystemEvent::ModelChanged { percent: None, .. }
                        )
                    ) => {}
                    kivo_core::bus::Received::Event(_) => continue,
                    kivo_core::bus::Received::Missed(_) => {}
                    kivo_core::bus::Received::Closed => return,
                },
                () = self.edited.notified() => {}
                () = shutdown.cancelled() => return,
            }
        }
    }

    /// Called once at startup, before `run`.
    pub fn start(&self) {
        self.fetch_model_if_wanted();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_words_are_stricter_than_the_default_wake_word() {
        let stop = stop_words();
        let ids: Vec<&str> = stop.iter().map(|k| k.id.as_str()).collect();
        assert_eq!(ids, ["kivo-stop", "stop", "cancel"]);
        let wake = Keyword::new("hey-kivo", "Hey Kivo").with_sensitivity(0.5);
        assert!(stop.iter().all(|k| k.threshold > wake.threshold));
    }
}
