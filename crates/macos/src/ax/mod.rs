//! Safe wrappers over the Accessibility API.

mod element;
mod error;
pub mod names;
mod value;

pub use element::AxElement;
pub use error::AxError;

use objc2_core_foundation::CFDictionary;
use objc2_foundation::{NSDictionary, NSNumber, NSString};

use crate::ffi::ax::{AXIsProcessTrusted, AXIsProcessTrustedWithOptions};

/// The value of `kAXTrustedCheckOptionPrompt`.
pub const TRUSTED_CHECK_OPTION_PROMPT: &str = "AXTrustedCheckOptionPrompt";

/// Whether Snappy has Accessibility access.
pub fn is_trusted() -> bool {
    // SAFETY: no arguments; callable from any thread.
    unsafe { AXIsProcessTrusted() != 0 }
}

/// `AXIsProcessTrustedWithOptions`, which can prompt for access.
pub fn is_trusted_with_options(options: &NSDictionary<NSString, NSNumber>) -> bool {
    let options: *const NSDictionary<NSString, NSNumber> = options;
    // SAFETY: NSDictionary is toll-free bridged to CFDictionary, and the call doesn't
    // retain the dictionary beyond its duration.
    unsafe { AXIsProcessTrustedWithOptions(options.cast::<CFDictionary>()) != 0 }
}
