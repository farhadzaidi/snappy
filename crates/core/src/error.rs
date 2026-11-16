use thiserror::Error;

/// Why a window operation didn't happen. Messages are shown to the user as written.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WindowError {
    /// The window or the operation can't be used right now.
    #[error("{0}")]
    Unavailable(String),
    /// An app refused or only partly applied a change.
    #[error("{0}")]
    Refused(String),
    #[error("That zone doesn't have a usable size.")]
    InvalidFrame,
    #[error("Nothing to undo.")]
    NoHistory,
    #[error("Nothing to redo.")]
    NoRedo,
    /// A newer gesture took over. Never shown.
    #[error("cancelled")]
    Cancelled,
}

impl WindowError {
    pub fn unavailable(message: impl Into<String>) -> Self {
        WindowError::Unavailable(message.into())
    }

    pub fn refused(message: impl Into<String>) -> Self {
        WindowError::Refused(message.into())
    }
}
