//! The window access a session moves windows through, and saving its journal.

use snappy_core::error::WindowError;
use snappy_core::geometry::{Point, Rect};
use snappy_core::workspace::{JournalEntry, Session, WorkspaceIo};
use snappy_platform::{Platform, WindowId};

use crate::state::EngineState;
use crate::store::remove_state;

use super::recovery::{SavedWindow, write_windows};

/// Window access for a session, with the journal mapped to recovery keys.
pub(super) struct CoreIo<'a, P> {
    core: &'a mut EngineState<P>,
}

impl<'a, P: Platform> CoreIo<'a, P> {
    pub(super) fn new(core: &'a mut EngineState<P>) -> Self {
        Self { core }
    }
}

impl<P: Platform> WorkspaceIo for CoreIo<'_, P> {
    fn frame(&mut self, window: WindowId) -> Result<Rect, WindowError> {
        self.core.platform.frame(window)
    }

    fn is_closed(&mut self, window: WindowId) -> bool {
        self.core.platform.is_closed(window)
    }

    fn is_movable(&mut self, window: WindowId) -> bool {
        !self.core.platform.is_closed(window)
            && self
                .core
                .platform
                .state(window)
                .is_ok_and(|state| state.movable())
    }

    fn is_minimized(&mut self, window: WindowId) -> bool {
        self.core
            .platform
            .state(window)
            .is_ok_and(|state| state.minimized)
    }

    fn is_on_parking_desktop(&mut self, window: WindowId) -> bool {
        self.core
            .platform
            .state(window)
            .is_ok_and(|state| state.on_parking_desktop)
    }

    fn place(&mut self, window: WindowId, origin: Point, hidden: bool) -> Result<(), WindowError> {
        if hidden {
            self.core.platform.hide_for_workspace(window, origin)
        } else {
            self.core.platform.show_for_workspace(window, origin)
        }
    }

    fn focus(&mut self, window: WindowId) {
        self.core.platform.focus(window);
    }

    fn write_journal(&mut self, entries: &[JournalEntry]) -> Result<(), WindowError> {
        let path = self.core.paths.journal();
        if entries.is_empty() {
            return remove_state(&path).map_err(|_| unavailable_journal());
        }
        let saved: Option<Vec<SavedWindow>> = entries
            .iter()
            .map(|entry| {
                self.core
                    .workspaces
                    .keys
                    .get(&entry.window)
                    .map(|key| SavedWindow {
                        key: key.clone(),
                        workspace_id: entry.workspace.clone(),
                        frame: entry.frame,
                        parked_origin: Some(entry.parked),
                        prior_parked_origin: entry.prior_parked,
                    })
            })
            .collect();
        let saved = saved.ok_or_else(unavailable_journal)?;
        write_windows(&path, &saved).map_err(|_| unavailable_journal())
    }
}

fn unavailable_journal() -> WindowError {
    WindowError::unavailable("Snappy couldn't save its list of hidden windows, so nothing was moved.")
}

/// Write the session's parked windows to the journal, telling the user when it can't be.
pub(super) fn save_journal<P: Platform>(core: &mut EngineState<P>, session: &Session) {
    let journal = session.journal();
    if let Err(error) = CoreIo::new(&mut *core).write_journal(&journal) {
        core.announce_error(&error.to_string());
    }
}
