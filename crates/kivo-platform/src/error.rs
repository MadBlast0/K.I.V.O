use thiserror::Error;

/// A platform call that failed. The `Display` text is safe to show to the user (plan §146); raw
/// OS codes are kept in `Os` for the log only.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PlatformError {
    #[error("this isn't supported on this version of the operating system")]
    Unsupported,
    #[error("{0} wasn't found")]
    NotFound(String),
    #[error("the operating system denied access")]
    AccessDenied,
    /// For example a hotkey another app already registered.
    #[error("{0} is already in use")]
    Conflict(String),
    #[error("the operation was cancelled")]
    Cancelled,
    /// The target runs as administrator and KIVO doesn't; Windows blocks automating it (UIPI).
    /// KIVO never elevates itself to get around this (TOOL-22).
    #[error("{0} is running as administrator")]
    Elevated(String),
    /// The app didn't answer in time (a hung window).
    #[error("the app didn't respond")]
    Timeout,
    /// An OS error. `message` is for the log, not the user.
    #[error("the operating system reported an error")]
    Os { code: i64, message: String },
}

pub type PlatformResult<T> = Result<T, PlatformError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_facing_text_never_includes_raw_os_details() {
        let e = PlatformError::Os {
            code: -2147024891,
            message: "0x80070005 E_ACCESSDENIED".into(),
        };
        assert_eq!(e.to_string(), "the operating system reported an error");
    }

    #[test]
    fn conflicts_name_what_is_taken() {
        assert_eq!(
            PlatformError::Conflict("Ctrl+Space".into()).to_string(),
            "Ctrl+Space is already in use"
        );
    }
}
