//! Other apps' windows through Accessibility, on the engine thread: which ones Snappy
//! manages, how each is identified, how they match the window server's list, and the
//! calls that read, move, and focus them.

mod chrome;
pub(crate) mod eligibility;
mod enumerate;
mod lookup;
pub(crate) mod occupancy;
mod operations;
pub(crate) mod registry;
pub(crate) mod server;
mod tab_matching;
mod tabs;
mod tracked;

pub use tracked::Windows;
