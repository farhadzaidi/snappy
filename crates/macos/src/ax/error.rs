//! Accessibility result codes (`AXError.h`).

use crate::ffi::ax::AXError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AxError(pub AXError);

impl AxError {
    pub const SUCCESS: AXError = 0;
    pub const FAILURE: AxError = AxError(-25200);
    pub const INVALID_UI_ELEMENT: AxError = AxError(-25202);
    pub const CANNOT_COMPLETE: AxError = AxError(-25204);
    pub const API_DISABLED: AxError = AxError(-25211);
    pub const NO_VALUE: AxError = AxError(-25212);

    /// `Ok` for `kAXErrorSuccess`.
    pub fn check(code: AXError) -> Result<(), AxError> {
        if code == Self::SUCCESS {
            Ok(())
        } else {
            Err(AxError(code))
        }
    }

    /// The element no longer exists.
    pub fn is_gone(self) -> bool {
        self == Self::INVALID_UI_ELEMENT
    }

    /// The app didn't answer within the messaging timeout.
    pub fn is_timeout(self) -> bool {
        self == Self::CANNOT_COMPLETE
    }

    /// Accessibility access was revoked.
    pub fn is_revocation(self) -> bool {
        self == Self::API_DISABLED
    }
}
