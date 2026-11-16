//! What can go wrong reading, checking, or changing settings, as the user is told.

use thiserror::Error;

use super::FORMAT_VERSION;
use crate::keys::SnappyKeyError;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SettingsError {
    #[error("This file uses settings version {0}. Snappy reads version {FORMAT_VERSION}.", FORMAT_VERSION = FORMAT_VERSION)]
    UnsupportedVersion(u32),
    #[error("The settings file contains invalid data.")]
    Invalid,
    #[error("Snappy couldn't read the settings file.")]
    Unreadable,
    #[error("Snappy couldn't save the settings file.")]
    Unwritable,
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    SnappyKey(#[from] SnappyKeyError),
    #[error(
        "The existing settings file couldn't be loaded and hasn't been changed. Repair or move it, then reopen Snappy before saving."
    )]
    Protected,
}

/// Why a layout edit from the settings app is refused, as the user is told.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LayoutEditError {
    #[error("Unknown layout.")]
    Unknown,
    #[error("Give this layout a name and 1–32 valid zones.")]
    Invalid,
    #[error("Move or resize overlapping zones apart before saving.")]
    Overlapping,
    #[error("{0}")]
    Conflict(String),
}
