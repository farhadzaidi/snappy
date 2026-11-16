//! The adapter against a real X server and window manager, one module per feature. These
//! run only when `SNAPPY_X11_TEST_DISPLAY` names a disposable display, which
//! `tools/live/x11/x11-session.sh` provides; they never touch the desktop in `DISPLAY`.
//! Run them one at a time: they share the screen and the pointer.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used)]

mod caps_lock;
mod displays;
mod overlay;
mod parking;
mod pointer;
mod shortcuts;
mod support;
mod windows;
