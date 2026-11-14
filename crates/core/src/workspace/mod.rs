//! Workspaces: groups of windows, one visible at a time, switched by parking the others.

mod focus;
mod parking;
mod relaunch;
mod session;
#[cfg(test)]
mod tests;

pub use focus::{FOCUS_GATE_SECONDS, FocusPolicy};
pub use parking::{ParkingPolicy, is_parked, parking_origin, reachable, sits_at};
pub use relaunch::{Found, Journaled, Relaunch, relaunch};
pub use session::{JournalEntry, Member, Session, Tabs, WorkspaceIo};
