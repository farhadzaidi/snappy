//! Live tests: the real daemon, real windows, and real input, checked against what the
//! windows report about themselves. They run only inside a sandbox, a VM or a container,
//! never in the owner's session.

pub mod fixture;
#[cfg(target_os = "linux")]
pub mod gnome;
pub mod input;
pub mod protocol;
pub mod scenarios;
pub mod screen;
pub mod session;
