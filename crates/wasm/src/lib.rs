//! The `snappy-core` rules the app needs, exported to WebAssembly.
//!
//! Every function takes one JSON request and returns JSON, so the TypeScript types mirror
//! the Rust serde shapes. Host builds expose the same functions for tests.

mod api;
mod exports;
mod wire;

pub use exports::*;
