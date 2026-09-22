//! Egress checks for the privacy mode (SECURITY §6). Cloud speech engines count as egress: before
//! any audio or text leaves the device for one, the mode and the Cloud AI capability must allow it
//! (VOICE-07). Privacy always wins over convenience: a refused cloud engine is not a failure to
//! work around, the local engine is used instead.

use crate::policy::{Denial, DenyCode};
use kivo_core::capability::{Capability, CapabilitySettings};
use kivo_core::config::PrivacyMode;
use kivo_core::text;

/// Whether audio (or text to speak) may go to an engine. `cloud` is true for an engine that runs
/// off the device; local and system engines always pass.
pub fn speech_egress(
    cloud: bool,
    mode: PrivacyMode,
    capabilities: &CapabilitySettings,
) -> Result<(), Denial> {
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
        // Cloud, or Custom where cloud features are switched on one by one (Cloud AI here).
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
            assert!(speech_egress(false, mode, &caps).is_ok(), "{mode:?}");
        }
        assert!(speech_egress(true, PrivacyMode::Cloud, &caps).is_ok());
        assert!(speech_egress(true, PrivacyMode::Custom, &caps).is_ok());
        for mode in [PrivacyMode::Local, PrivacyMode::StrictPrivate] {
            let denied = speech_egress(true, mode, &caps).unwrap_err();
            assert_eq!(denied.code, DenyCode::Privacy);
            assert!(!denied.message.starts_with("policy."));
        }
        caps.set(Capability::CloudBrains, false);
        assert_eq!(
            speech_egress(true, PrivacyMode::Cloud, &caps)
                .unwrap_err()
                .code,
            DenyCode::Privacy,
            "the user turned cloud processing off (invariant 11)"
        );
    }
}
