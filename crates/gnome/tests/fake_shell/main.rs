//! The GNOME adapter against a fake GNOME Shell on a private session bus: what it asks the
//! extension, how it reads the answers, and how it turns the extension's events into engine
//! events, one module per feature. Needs `dbus-daemon`; prints "skipped" without it.
//!
//! The tests share one private bus, its own temporary home, and an in-memory settings
//! backend, so nothing reaches the session the tests run in.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod caps_lock;
mod displays;
mod events;
mod extension;
mod overlay;
mod shortcuts;
mod support;
mod windows;
