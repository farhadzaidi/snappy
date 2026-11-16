//! Other clients' windows: which ones Snappy manages, their frames, and the EWMH requests
//! that read, move, and park them.

mod clients;
mod desktops;
mod eligibility;
mod frames;
mod record;

pub use clients::Windows;
