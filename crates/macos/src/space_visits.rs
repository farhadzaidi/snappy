//! Counting visits to the Spaces showing, so the engine thread can tell a window it saw on
//! them from one it last saw before they changed. A visit ends whenever the active Space or
//! the displays change.
//!
//! An Accessibility query that runs just before a change's notification arrives still counts
//! the visit as going on. Workspaces pause on every Space but Snappy's own, so such a query
//! comes only as they stop, and a window it wrongly puts in a tab group leaves the group as
//! soon as its app lists it on its own again.

use std::sync::atomic::{AtomicU64, Ordering};

static VISIT: AtomicU64 = AtomicU64::new(0);

/// Starts a new visit. Called on the main thread as the Spaces showing may have changed,
/// before the change is reported, so nothing the engine learns of it predates the new visit.
pub fn start_new() {
    VISIT.fetch_add(1, Ordering::SeqCst);
}

/// The visit going on now.
pub fn current() -> u64 {
    VISIT.load(Ordering::SeqCst)
}
