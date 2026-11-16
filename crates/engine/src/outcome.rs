//! What a drop, a snap, or a swap came to, as the debug log and the drop diagnostics name it.

use snappy_core::placement::drag::CancelReason;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    Snapped,
    /// The app kept the window from taking the zone's exact frame.
    AppConstrainedBounds,
    /// A newer gesture took over the snap.
    Interrupted,
    WindowWriteFailed,
    Swapped,
    SwapInterrupted,
    SwapBlocked,
    SwapFailed,
    // A drop that did nothing, and why.
    TargetLookupFailed,
    WindowUnavailable,
    Cancelled(CancelReason),
    NoWindowMovement,
    ModifierReleased,
    OutsideDisplay,
    LayoutDisabled,
    OutsideZone,
}

impl Outcome {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Outcome::Snapped => "snapped",
            Outcome::AppConstrainedBounds => "appConstrainedBounds",
            Outcome::Interrupted => "interruptedByNewInteraction",
            Outcome::WindowWriteFailed => "windowWriteFailed",
            Outcome::Swapped => "swapped",
            Outcome::SwapInterrupted => "swapInterrupted",
            Outcome::SwapBlocked => "swapBlocked",
            Outcome::SwapFailed => "swapFailed",
            Outcome::TargetLookupFailed => "targetLookupFailed",
            Outcome::WindowUnavailable => "windowUnavailable",
            Outcome::Cancelled(reason) => reason.as_str(),
            Outcome::NoWindowMovement => "noWindowMovement",
            Outcome::ModifierReleased => "modifierReleased",
            Outcome::OutsideDisplay => "outsideDisplay",
            Outcome::LayoutDisabled => "layoutDisabled",
            Outcome::OutsideZone => "outsideZone",
        }
    }

    /// Logged as a warning.
    pub(crate) fn is_failure(self) -> bool {
        matches!(self, Outcome::WindowWriteFailed | Outcome::SwapFailed)
    }

    /// The drop was turned down, so the window goes back where it started.
    pub(crate) fn is_rejected(self) -> bool {
        matches!(self, Outcome::SwapBlocked | Outcome::SwapFailed)
    }
}
