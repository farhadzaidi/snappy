//! Snappy's rules: where zones sit, how layouts are edited, and how windows are
//! placed. No operating-system dependencies and no I/O.

pub mod assignment;
pub mod bindings;
pub mod bounds;
pub mod caps_lock;
pub mod drag;
pub mod editor;
pub mod empty_zone;
pub mod error;
pub mod focus;
pub mod geometry;
pub mod guide;
pub mod keys;
pub mod layout;
pub mod navigate;
pub mod operations;
pub mod reset;
pub mod settings;
pub mod shortcuts;
pub mod swap;
pub mod window;
pub mod workspace;
