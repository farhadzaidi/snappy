//! The GNOME adapter against a real GNOME Shell with Snappy's extension and the live tests'
//! helper loaded, one module per feature. Runs only when `SNAPPY_GNOME_TEST=1`, which
//! `tools/live/gnome/test.sh` sets inside a throwaway GNOME Shell; elsewhere every test prints
//! "skipped". Fixture windows come from `snappy-fixture`, named by `SNAPPY_GNOME_FIXTURE`.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod caps_lock;
mod displays;
mod drags;
mod extension;
mod overlay;
mod parking;
mod shortcuts;
mod support;
mod windows;
