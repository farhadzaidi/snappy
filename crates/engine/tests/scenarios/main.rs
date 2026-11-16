//! End-to-end behavior on the fake platform, one module per feature. `support` sets up
//! engines, acts like the user, and reads what the engine did.
#![allow(clippy::unwrap_used)]

mod caps_lock;
mod containment;
mod desktop_parking;
mod dragging;
mod focus;
mod history;
mod layouts;
mod live_control;
mod nearest_empty;
mod restarts;
mod selection;
mod sending;
mod settings;
mod shortcuts;
mod snappy_key;
mod support;
mod swaps;
mod tabs;
mod workspaces;
