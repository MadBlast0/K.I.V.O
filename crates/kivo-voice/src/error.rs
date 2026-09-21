use thiserror::Error;

/// A speech engine failure. `Display` is safe to show and speak (plan §146); engine internals go
/// to the log through `Engine`'s detail.
#[derive(Debug, Error)]
pub enum VoiceError {
    #[error("the {0} model isn't downloaded yet")]
    ModelMissing(String),
    #[error("the voice engine stopped working")]
    Engine(String),
    #[error("cancelled")]
    Cancelled,
    #[error("{0} isn't available on this PC")]
    Unavailable(String),
}

impl VoiceError {
    /// The detail for the log (never spoken).
    pub fn detail(&self) -> String {
        match self {
            Self::Engine(detail) => detail.clone(),
            other => other.to_string(),
        }
    }
}

impl<R> From<ort::Error<R>> for VoiceError {
    fn from(e: ort::Error<R>) -> Self {
        Self::Engine(e.to_string())
    }
}

pub type VoiceResult<T> = Result<T, VoiceError>;
