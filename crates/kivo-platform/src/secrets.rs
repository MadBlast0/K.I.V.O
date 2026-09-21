//! Credential storage and data protection (SECURITY §5). Config holds only handles; values are
//! read inside the adapter or tool that needs them.

use crate::error::PlatformResult;
use kivo_core::Secret;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Names a stored secret: `secret://kivo/<provider>/<name>` in config.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SecretHandle {
    pub provider: String,
    pub name: String,
}

impl fmt::Display for SecretHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "secret://kivo/{}/{}", self.provider, self.name)
    }
}

pub trait Secrets: Send + Sync {
    fn get(&self, handle: &SecretHandle) -> PlatformResult<Option<Secret<String>>>;
    fn set(&self, handle: &SecretHandle, value: Secret<String>) -> PlatformResult<()>;
    fn delete(&self, handle: &SecretHandle) -> PlatformResult<()>;
    /// Encrypts data so only this user on this machine can read it (DPAPI on Windows).
    fn protect(&self, data: &[u8]) -> PlatformResult<Vec<u8>>;
    fn unprotect(&self, data: &[u8]) -> PlatformResult<Vec<u8>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_render_as_config_references() {
        let h = SecretHandle {
            provider: "anthropic".into(),
            name: "api-key".into(),
        };
        assert_eq!(h.to_string(), "secret://kivo/anthropic/api-key");
    }
}
