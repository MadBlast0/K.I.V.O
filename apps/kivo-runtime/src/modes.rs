//! Product modes (plan §142, PLAN-06): one switch for how KIVO behaves right now, each mapped onto
//! the privacy mode, the performance profile and the Island.
//!
//! | Mode | Privacy | Profile | Island and notices |
//! |---|---|---|---|
//! | Normal | yours | yours | yours |
//! | Private | Prefer this PC | yours | yours |
//! | Offline | Strictly private: nothing leaves the PC | yours | yours |
//! | Battery | yours | Battery | yours |
//! | Performance | yours | Performance (models stay loaded, prewarmed) | yours |
//! | Gaming | yours | Gaming (no GPU) | hidden over fullscreen |
//! | Presentation | yours | yours | large, with the words and hints shown; no toasts, nothing said unasked |
//!
//! Switching away from Normal remembers the settings the mode changes; switching back to Normal
//! restores them, so a mode never loses the user's own choices.

pub use kivo_core::config::ProductMode;
use kivo_core::config::{
    FullscreenBehavior, IslandSize, KivoConfig, PerformanceProfile, PrivacyMode, SpeakMode,
};
use serde::{Deserialize, Serialize};

/// Where the settings a mode replaced are kept (the database's meta table).
pub const BEFORE_KEY: &str = "mode.before";

/// The settings a mode may change, as they were before it (to restore on Normal).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Before {
    pub privacy: PrivacyMode,
    pub profile: PerformanceProfile,
    pub island_size: IslandSize,
    pub show_transcript: bool,
    pub voice_hints: bool,
    pub in_fullscreen: FullscreenBehavior,
    pub toasts: bool,
    pub speak: SpeakMode,
}

impl Before {
    pub fn of(config: &KivoConfig) -> Self {
        Self {
            privacy: config.privacy.mode,
            profile: config.performance.profile,
            island_size: config.overlay.size,
            show_transcript: config.overlay.show_transcript,
            voice_hints: config.overlay.voice_hints,
            in_fullscreen: config.overlay.in_fullscreen,
            toasts: config.automation.toasts,
            speak: config.automation.speak,
        }
    }

    fn restore(&self, config: &mut KivoConfig) {
        config.privacy.mode = self.privacy;
        config.performance.profile = self.profile;
        config.overlay.size = self.island_size;
        config.overlay.show_transcript = self.show_transcript;
        config.overlay.voice_hints = self.voice_hints;
        config.overlay.in_fullscreen = self.in_fullscreen;
        config.automation.toasts = self.toasts;
        config.automation.speak = self.speak;
    }
}

/// Switches KIVO to `mode`: the settings it maps to are saved, and what Normal looked like is kept
/// in the database until Normal comes back. Returns the new settings.
pub fn apply(
    core: &crate::core::Core,
    engine: &crate::engine::Engine,
    mode: ProductMode,
) -> KivoConfig {
    let db = engine.recorder.database();
    let before: Option<Before> = db
        .lock()
        .ok()
        .and_then(|d| d.meta(BEFORE_KEY).ok().flatten())
        .and_then(|s| serde_json::from_str(&s).ok());
    let mut config = core.config();
    let kept = switch(&mut config, mode, before);
    config.general.product_mode = mode;
    if let Ok(d) = db.lock() {
        let value = kept
            .as_ref()
            .and_then(|b| serde_json::to_string(b).ok())
            .unwrap_or_default();
        if let Err(e) = d.set_meta(BEFORE_KEY, &value) {
            tracing::warn!(%e, "couldn't keep the settings the mode replaced");
        }
    }
    core.update_config(|c| *c = config)
}

/// Switches `config` to `mode`. `before` is what Normal looked like (kept by the caller while a
/// mode is on); it's returned updated. From one mode to another, the user's own settings are put
/// back first, so modes don't stack.
pub fn switch(
    config: &mut KivoConfig,
    mode: ProductMode,
    before: Option<Before>,
) -> Option<Before> {
    let saved = before.unwrap_or_else(|| Before::of(config));
    saved.restore(config);
    match mode {
        ProductMode::Normal => return None,
        ProductMode::Private => config.privacy.mode = PrivacyMode::Local,
        ProductMode::Offline => config.privacy.mode = PrivacyMode::StrictPrivate,
        ProductMode::Battery => config.performance.profile = PerformanceProfile::Battery,
        ProductMode::Performance => config.performance.profile = PerformanceProfile::Performance,
        ProductMode::Gaming => {
            config.performance.profile = PerformanceProfile::Gaming;
            config.overlay.in_fullscreen = FullscreenBehavior::Hide;
        }
        ProductMode::Presentation => {
            config.overlay.size = IslandSize::Large;
            config.overlay.show_transcript = true;
            config.overlay.voice_hints = true;
            config.automation.toasts = false;
            config.automation.speak = SpeakMode::Never;
        }
    }
    Some(saved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_mode_sets_its_part_and_normal_puts_everything_back() {
        let mut config = KivoConfig::default();
        config.overlay.size = IslandSize::Compact;
        config.performance.profile = PerformanceProfile::Balanced;
        let mine = config.clone();

        let before = switch(&mut config, ProductMode::Presentation, None);
        assert_eq!(config.overlay.size, IslandSize::Large);
        assert!(!config.automation.toasts);
        assert_eq!(config.automation.speak, SpeakMode::Never);
        assert_eq!(config.privacy.mode, mine.privacy.mode, "untouched");

        // Straight to another mode: presentation's changes are undone first.
        let before = switch(&mut config, ProductMode::Offline, before);
        assert_eq!(config.privacy.mode, PrivacyMode::StrictPrivate);
        assert_eq!(config.overlay.size, IslandSize::Compact);
        assert!(config.automation.toasts);

        let before = switch(&mut config, ProductMode::Gaming, before);
        assert_eq!(config.performance.profile, PerformanceProfile::Gaming);
        assert_eq!(config.privacy.mode, mine.privacy.mode);

        assert!(switch(&mut config, ProductMode::Normal, before).is_none());
        assert_eq!(
            config, mine,
            "Normal is exactly the user's own settings again"
        );
    }

    #[test]
    fn private_battery_and_performance() {
        for (mode, check) in [
            (ProductMode::Private, (Some(PrivacyMode::Local), None)),
            (
                ProductMode::Battery,
                (None, Some(PerformanceProfile::Battery)),
            ),
            (
                ProductMode::Performance,
                (None, Some(PerformanceProfile::Performance)),
            ),
        ] {
            let mut config = KivoConfig::default();
            let _ = switch(&mut config, mode, None);
            if let Some(p) = check.0 {
                assert_eq!(config.privacy.mode, p);
            }
            if let Some(p) = check.1 {
                assert_eq!(config.performance.profile, p);
            }
        }
    }
}
