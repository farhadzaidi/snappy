//! What sending a window to the back needs where Accessibility can't lower one: raising a
//! window alone, without activating its app, and reading whether the window server lists
//! windows in an order, so the engine can await each raise before the next.

use snappy_core::error::WindowError;
use snappy_core::window::WindowSnapshot;
use snappy_platform::WindowId;

use super::server;
use super::stack_order::{ListedWindow, listed_in_order};
use super::tracked::{Windows, gone};
use crate::ax::{self, names};
use crate::messages::{NO_PERMISSION, unavailable};

impl Windows {
    /// Raises the window over the others without focusing it or activating its app.
    pub fn raise(&self, id: WindowId) -> Result<(), WindowError> {
        if !ax::is_trusted() {
            return Err(unavailable(NO_PERMISSION));
        }
        self.tracked(id)?
            .element
            .perform(names::RAISE_ACTION)
            .map_err(gone)
    }

    /// Whether the window server lists `windows` in this order, front to back, matched by
    /// their numbers there, else by their process and the frames the engine knows them by.
    /// A window no longer tracked never shows.
    pub fn shows_in_order(&self, windows: &[WindowSnapshot]) -> bool {
        let listed: Option<Vec<ListedWindow>> = windows
            .iter()
            .map(|window| {
                let entry = self.tracked(window.id).ok()?;
                Some(ListedWindow {
                    pid: entry.pid,
                    frame: window.frame,
                    number: entry.number,
                })
            })
            .collect();
        let Some(listed) = listed else {
            return false;
        };
        let rows = server::on_screen_windows().unwrap_or_default();
        listed_in_order(&rows, &listed)
    }
}
