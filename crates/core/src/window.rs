use serde::{Deserialize, Serialize};

use crate::geometry::Rect;

/// A window, as identified by the platform adapter for the window's lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct WindowId(pub u64);

/// A window and its frame at one moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowSnapshot {
    pub id: WindowId,
    pub frame: Rect,
}
