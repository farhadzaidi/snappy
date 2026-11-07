//! Snappy's rules: where zones sit, how layouts are edited, and how windows are
//! placed. No operating-system dependencies and no I/O.

pub mod bindings;
pub mod bounds;
pub mod drag;
pub mod editor;
pub mod error;
pub mod geometry;
pub mod keys;
pub mod layout;
pub mod navigate;
pub mod operations;
pub mod settings;
pub mod swap;
pub mod window;
pub mod workspace;
