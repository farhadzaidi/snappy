//! The KWin adapter against a real, headless KWin. Runs only when `SNAPPY_KWIN_TEST=1`, which
//! `tools/live/kwin/test.sh` sets inside a sealed-off KWin session; elsewhere every test
//! prints "skipped". Fixture windows come from `snappy-fixture`, named by
//! `SNAPPY_KWIN_FIXTURE`. Input goes through KWin's `org_kde_kwin_fake_input`, which KWin
//! handles like a device's.
//!
//! One module per feature. `support` runs the adapter and the fixture app, `input` drives
//! KWin's fake input, `panel` reserves space like Plasma's panels, and `probes` asks KWin
//! itself what it shows.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod caps_lock;
mod daemon;
mod displays;
mod dragging;
mod input;
mod overlay;
mod panel;
mod probes;
mod shortcuts;
mod support;
mod windows;
