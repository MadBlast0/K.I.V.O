//! Code signatures (SECURITY §9, SEC-32): whether a file carries a valid Authenticode signature,
//! and whose. The updater checks an installer with it before running it.

use crate::PlatformResult;
use std::path::Path;

pub trait CodeTrust: Send + Sync {
    /// The signer's name (the signing certificate's display name, such as "Microsoft
    /// Corporation") when `path` has a valid signature that chains to a trusted root and isn't
    /// revoked; `None` when it is unsigned, altered after signing, or not trusted.
    fn signer(&self, path: &Path) -> PlatformResult<Option<String>>;
}
