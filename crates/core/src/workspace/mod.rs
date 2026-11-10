//! Workspaces: groups of windows, one visible at a time, switched by parking the others.

mod focus;
mod parking;
mod session;
#[cfg(test)]
mod tests;

pub use focus::{FOCUS_GATE_SECONDS, FocusPolicy};
pub use parking::{PARKED_TOLERANCE, ParkingPolicy, is_parked, parking_origin, reachable};
pub use session::{JournalEntry, Member, Session, WorkspaceIo};
