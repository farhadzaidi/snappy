//! Keeping workspaces to the native desktop they started on.

use crate::windows::Display;

pub trait DesktopGuard {
    fn bind(&mut self, displays: &[Display]);
    fn on_managed_desktop(&mut self) -> bool;
    fn clear(&mut self);
}
