//! Whether Snappy can control windows on this desktop, and what can make it able to.

use serde::{Deserialize, Serialize};

/// Whether Snappy can control windows here at all, apart from any permission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveControl {
    Available,
    Unavailable {
        /// Why not, as a sentence for the user that says what to do, such as logging in
        /// with another kind of session.
        reason: String,
        /// Something Snappy can do about it when the user asks.
        remedy: Option<Remedy>,
    },
}

/// What Snappy can do, when asked, to make live control available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Remedy {
    /// End the desktop session, after the desktop's own confirmation, so it starts again
    /// with what Snappy installed.
    LogOut,
}

impl LiveControl {
    pub fn unavailable(reason: impl Into<String>) -> LiveControl {
        LiveControl::Unavailable {
            reason: reason.into(),
            remedy: None,
        }
    }

    pub fn is_available(&self) -> bool {
        matches!(self, LiveControl::Available)
    }

    pub fn reason(&self) -> Option<&str> {
        match self {
            LiveControl::Available => None,
            LiveControl::Unavailable { reason, .. } => Some(reason),
        }
    }

    pub fn remedy(&self) -> Option<Remedy> {
        match self {
            LiveControl::Available => None,
            LiveControl::Unavailable { remedy, .. } => *remedy,
        }
    }
}
