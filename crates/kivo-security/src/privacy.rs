//! Egress checks for the privacy mode (SECURITY §6). Cloud speech engines count as egress: before
//! any audio or text leaves the device for one, the mode and the Cloud AI capability must allow it
//! (VOICE-07). Privacy always wins over convenience: a refused cloud engine is not a failure to
//! work around, the local engine is used instead.

use crate::DataClass;
use crate::policy::{Denial, DenyCode};
use kivo_core::capability::{Capability, CapabilitySettings};
use kivo_core::config::{Privacy, PrivacyMode};
use kivo_core::text;

/// Whether cloud brains may answer at all (SEC-21, invariant 11): Cloud, or Custom with its
/// switch on; and the Cloud AI capability.
pub fn cloud_brains(privacy: &Privacy, capabilities: &CapabilitySettings) -> bool {
    let mode = match privacy.mode {
        PrivacyMode::Cloud => true,
        PrivacyMode::Custom => privacy.custom.cloud_brains,
        PrivacyMode::Local | PrivacyMode::StrictPrivate => false,
    };
    mode && capabilities.enabled(Capability::CloudBrains)
}

/// Whether a cloud brain may be shown a screenshot (CAP-08, SEC-21).
pub fn cloud_vision(privacy: &Privacy) -> bool {
    match privacy.mode {
        PrivacyMode::Cloud => true,
        PrivacyMode::Custom => privacy.custom.cloud_vision,
        PrivacyMode::Local | PrivacyMode::StrictPrivate => false,
    }
}

/// The request itself asks to stay on the PC ("summarize this confidential document locally",
/// plan §139): the words for that come from the language's list, matched as whole words.
pub fn asks_to_stay_local(request: &str) -> bool {
    let request = format!(" {} ", normalize(request));
    text::t("privacy.localWords")
        .split('|')
        .map(normalize)
        .filter(|w| !w.is_empty())
        .any(|w| request.contains(&format!(" {w} ")))
}

/// Lower case, letters and digits only, single spaces.
fn normalize(text: &str) -> String {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '\'' && c != '’')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The most protected class that may go to a cloud brain (SECURITY §6): personal details in Cloud
/// mode, or in Custom when allowed; nothing sensitive, ever.
pub fn cloud_limit(privacy: &Privacy) -> DataClass {
    match privacy.mode {
        PrivacyMode::Cloud => DataClass::Personal,
        PrivacyMode::Custom if privacy.custom.personal_to_cloud => DataClass::Personal,
        PrivacyMode::Custom | PrivacyMode::StrictPrivate => DataClass::Normal,
        PrivacyMode::Local => DataClass::Public,
    }
}

/// Whether audio (or text to speak) may go to an engine. `cloud` is true for an engine that runs
/// off the device; local and system engines always pass.
pub fn speech_egress(
    cloud: bool,
    privacy: &Privacy,
    capabilities: &CapabilitySettings,
) -> Result<(), Denial> {
    let mode = privacy.mode;
    if !cloud {
        return Ok(());
    }
    let refused = |key: &str| {
        Err(Denial {
            code: DenyCode::Privacy,
            message: text::t(key),
        })
    };
    match mode {
        PrivacyMode::Local | PrivacyMode::StrictPrivate => refused("policy.privacyLocal"),
        // Custom: cloud speech has its own switch.
        PrivacyMode::Custom if !privacy.custom.cloud_speech => {
            refused("policy.privacyCustomSpeech")
        }
        PrivacyMode::Cloud | PrivacyMode::Custom => {
            if capabilities.enabled(Capability::CloudBrains) {
                Ok(())
            } else {
                refused("policy.privacyCloudOff")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn privacy(mode: PrivacyMode) -> Privacy {
        Privacy {
            mode,
            ..Privacy::default()
        }
    }

    #[test]
    fn local_engines_always_pass_and_cloud_ones_follow_the_mode() {
        let mut caps = CapabilitySettings::default();
        caps.set(Capability::CloudBrains, true);
        for mode in [
            PrivacyMode::Cloud,
            PrivacyMode::Local,
            PrivacyMode::StrictPrivate,
            PrivacyMode::Custom,
        ] {
            assert!(
                speech_egress(false, &privacy(mode), &caps).is_ok(),
                "{mode:?}"
            );
        }
        assert!(speech_egress(true, &privacy(PrivacyMode::Cloud), &caps).is_ok());
        for mode in [PrivacyMode::Local, PrivacyMode::StrictPrivate] {
            let denied = speech_egress(true, &privacy(mode), &caps).unwrap_err();
            assert_eq!(denied.code, DenyCode::Privacy);
            assert!(!denied.message.starts_with("policy."));
        }
        caps.set(Capability::CloudBrains, false);
        assert_eq!(
            speech_egress(true, &privacy(PrivacyMode::Cloud), &caps)
                .unwrap_err()
                .code,
            DenyCode::Privacy,
            "the user turned cloud processing off (invariant 11)"
        );
    }

    #[test]
    fn a_request_can_ask_to_stay_on_this_pc() {
        for yes in [
            "Summarize this confidential document locally.",
            "keep it private and summarize my notes",
            "Do this on this PC only",
            "don't upload it, just read it",
        ] {
            assert!(asks_to_stay_local(yes), "{yes}");
        }
        for no in [
            "Summarize this document",
            "what's the local weather",
            "open my private browser",
        ] {
            assert!(!asks_to_stay_local(no), "{no}");
        }
    }

    #[test]
    fn custom_turns_each_kind_of_cloud_on_by_itself() {
        let mut caps = CapabilitySettings::default();
        caps.set(Capability::CloudBrains, true);
        let mut p = privacy(PrivacyMode::Custom);
        // The defaults: cloud brains yes, speech and screenshots no, personal details stay local.
        assert!(cloud_brains(&p, &caps));
        assert!(speech_egress(true, &p, &caps).is_err());
        assert!(!cloud_vision(&p));
        assert_eq!(cloud_limit(&p), DataClass::Normal);
        p.custom.cloud_speech = true;
        p.custom.cloud_vision = true;
        p.custom.personal_to_cloud = true;
        p.custom.cloud_brains = false;
        assert!(speech_egress(true, &p, &caps).is_ok());
        assert!(cloud_vision(&p));
        assert_eq!(cloud_limit(&p), DataClass::Personal);
        assert!(!cloud_brains(&p, &caps));
        // The other modes.
        assert!(cloud_brains(&privacy(PrivacyMode::Cloud), &caps));
        assert!(!cloud_brains(&privacy(PrivacyMode::Local), &caps));
        assert_eq!(
            cloud_limit(&privacy(PrivacyMode::Cloud)),
            DataClass::Personal
        );
        assert_eq!(cloud_limit(&privacy(PrivacyMode::Local)), DataClass::Public);
        // Nothing sensitive goes to the cloud in any mode.
        for mode in [
            PrivacyMode::Cloud,
            PrivacyMode::Custom,
            PrivacyMode::Local,
            PrivacyMode::StrictPrivate,
        ] {
            assert!(cloud_limit(&privacy(mode)) < DataClass::Sensitive);
        }
    }
}
