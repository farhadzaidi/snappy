//! Other clients' windows through EWMH, on the engine thread's own connection.

mod desktops;
mod record;

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use snappy_core::error::WindowError;
use snappy_core::geometry::{Point, Rect, Size};
use snappy_platform::{WindowId, WindowInfo, WindowState};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{Atom, ClientMessageEvent, ConnectionExt, EventMask, InputFocus, Window};
use x11rb::rust_connection::RustConnection;

pub use record::WindowRecord;

use crate::frames::MoveResize;
use crate::messages::{NO_FOCUSED_WINDOW, NOT_STANDARD, WINDOW_GONE, unavailable};
use crate::shared::Shared;
use crate::x::{self, Atoms, ScreenInfo};

/// `_NET_WM_STATE` action that removes states.
const STATE_REMOVE: u32 = 0;
/// Source indication for requests from a pager.
const FROM_PAGER: u32 = 2;
/// How long a write waits for the window manager to restore a maximized window, whose
/// decorations change with the state, and how often it looks.
const RESTORE_TIMEOUT: Duration = Duration::from_millis(250);
const RESTORE_POLL: Duration = Duration::from_millis(10);
/// How long a workspace move or desktop change waits for the window manager, and how
/// often it looks.
const WORKSPACE_MOVE_TIMEOUT: Duration = Duration::from_millis(250);
const WORKSPACE_MOVE_POLL: Duration = Duration::from_millis(5);
/// A workspace move has landed when the frame is this close to its target.
const LANDED: f64 = 1.0;

pub struct Windows {
    conn: Arc<RustConnection>,
    screen: ScreenInfo,
    atoms: Atoms,
    shared: Arc<Shared>,
}

impl Windows {
    pub fn new(conn: Arc<RustConnection>, shared: Arc<Shared>) -> Windows {
        Windows {
            conn,
            screen: shared.screen,
            atoms: shared.atoms,
            shared,
        }
    }

    pub fn conn(&self) -> &RustConnection {
        &self.conn
    }

    pub fn screen(&self) -> &ScreenInfo {
        &self.screen
    }

    pub fn atoms(&self) -> &Atoms {
        &self.atoms
    }

    /// Managed clients from bottom to top of the stack.
    fn stacking(&self) -> Vec<Window> {
        let stacked = x::window_list(
            &*self.conn,
            self.screen.root,
            self.atoms._NET_CLIENT_LIST_STACKING,
        );
        if stacked.is_empty() {
            x::window_list(&*self.conn, self.screen.root, self.atoms._NET_CLIENT_LIST)
        } else {
            stacked
        }
    }

    pub fn current_desktop(&self) -> Option<u32> {
        x::cardinal(&*self.conn, self.screen.root, self.atoms._NET_CURRENT_DESKTOP)
    }

    fn records(&self, windows: &[Window]) -> Vec<Option<WindowRecord>> {
        let clients: BTreeSet<Window> = self.stacking().into_iter().collect();
        record::read(&self.conn, self.screen.root, &self.atoms, windows, &clients)
    }

    /// Every managed client, top of the stack first.
    fn all_records(&self) -> Vec<WindowRecord> {
        let mut stack = self.stacking();
        stack.reverse();
        let clients: BTreeSet<Window> = stack.iter().copied().collect();
        record::read(&self.conn, self.screen.root, &self.atoms, &stack, &clients)
            .into_iter()
            .flatten()
            .collect()
    }

    pub fn record(&self, id: WindowId) -> Result<WindowRecord, WindowError> {
        let window = Window::try_from(id.0).map_err(|_| unavailable(WINDOW_GONE))?;
        self.records(&[window])
            .pop()
            .flatten()
            .ok_or_else(|| unavailable(WINDOW_GONE))
    }

    /// Describes an eligible window and starts reporting its moves.
    fn info(&self, record: &WindowRecord) -> WindowInfo {
        self.shared.track(record.window);
        record.info()
    }

    /// The topmost visible client whose frame contains `point`.
    pub fn window_at(&self, point: Point) -> Result<WindowInfo, WindowError> {
        let desktop = self.current_desktop();
        let record = self
            .all_records()
            .into_iter()
            .find(|record| record.is_visible(desktop) && record.frame.contains(point))
            .ok_or_else(|| unavailable(NOT_STANDARD))?;
        record.facts.check()?;
        Ok(self.info(&record))
    }

    pub fn focused_window(&self) -> Result<WindowInfo, WindowError> {
        let active = x::window_list(&*self.conn, self.screen.root, self.atoms._NET_ACTIVE_WINDOW)
            .first()
            .copied()
            .filter(|window| *window != x11rb::NONE)
            .ok_or_else(|| unavailable(NO_FOCUSED_WINDOW))?;
        let record = self
            .records(&[active])
            .pop()
            .flatten()
            .ok_or_else(|| unavailable(NO_FOCUSED_WINDOW))?;
        record.facts.check()?;
        Ok(self.info(&record))
    }

    pub fn frame(&self, id: WindowId) -> Result<Rect, WindowError> {
        Ok(self.record(id)?.frame)
    }

    pub fn state(&self, id: WindowId, parking_desktop: Option<u32>) -> Result<WindowState, WindowError> {
        let record = self.record(id)?;
        Ok(WindowState {
            full_screen: record.facts.full_screen,
            has_sheet: false,
            minimized: record.facts.minimized,
            on_parking_desktop: parking_desktop.is_some() && record.desktop == parking_desktop,
        })
    }

    /// Closed, or withdrawn from management.
    pub fn is_closed(&self, id: WindowId) -> bool {
        Window::try_from(id.0).map_or(true, |window| !self.stacking().contains(&window))
    }

    /// Eligible clients on the current desktop and not minimized, top first.
    pub fn visible_windows(&self, app: Option<&str>) -> Vec<WindowInfo> {
        let desktop = self.current_desktop();
        self.all_records()
            .iter()
            .filter(|record| record.facts.check().is_ok() && record.is_visible(desktop))
            .filter(|record| app.is_none_or(|app| record.app() == app))
            .map(|record| self.info(record))
            .collect()
    }

    pub fn all_windows(&self) -> Vec<WindowInfo> {
        self.all_records()
            .iter()
            .filter(|record| record.facts.check().is_ok())
            .map(|record| self.info(record))
            .collect()
    }

    /// Visible eligible windows filling any of `frames`.
    pub fn windows_occupying(&self, frames: &[Rect]) -> Vec<WindowInfo> {
        let desktop = self.current_desktop();
        self.all_records()
            .iter()
            .filter(|record| record.facts.check().is_ok() && record.is_visible(desktop))
            .filter(|record| frames.iter().any(|zone| fills(record.frame, *zone)))
            .map(|record| self.info(record))
            .collect()
    }

    pub fn set_position(&self, id: WindowId, origin: Point) -> Result<(), WindowError> {
        let record = self.restored(id)?;
        let request =
            MoveResize::position(record.geometry.client_origin(origin)).ok_or(WindowError::InvalidFrame)?;
        self.move_resize(&record, request)
    }

    /// Move a window and wait until the window manager has placed it.
    pub fn move_and_wait(&self, id: WindowId, origin: Point) -> Result<(), WindowError> {
        let landed = |frame: Rect| (frame.x - origin.x).abs() < LANDED && (frame.y - origin.y).abs() < LANDED;
        if landed(self.frame(id)?) {
            return Ok(());
        }
        self.set_position(id, origin)?;
        self.wait(|| self.frame(id).is_ok_and(landed));
        Ok(())
    }

    /// Wait until `done`, for at most the workspace move timeout. Returns whether it held.
    fn wait(&self, done: impl Fn() -> bool) -> bool {
        let deadline = Instant::now() + WORKSPACE_MOVE_TIMEOUT;
        while !done() {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(WORKSPACE_MOVE_POLL);
        }
        true
    }

    pub fn set_size(&self, id: WindowId, size: Size) -> Result<(), WindowError> {
        let record = self.restored(id)?;
        let request = MoveResize::size(record.geometry.client_size(size)).ok_or(WindowError::InvalidFrame)?;
        self.move_resize(&record, request)
    }

    /// The window, restored first when maximized so the request uses its restored
    /// decorations.
    fn restored(&self, id: WindowId) -> Result<WindowRecord, WindowError> {
        let record = self.record(id)?;
        if !record.facts.maximized {
            return Ok(record);
        }
        let maximized = [
            STATE_REMOVE,
            self.atoms._NET_WM_STATE_MAXIMIZED_VERT,
            self.atoms._NET_WM_STATE_MAXIMIZED_HORZ,
            FROM_PAGER,
            0,
        ];
        self.client_message(record.window, self.atoms._NET_WM_STATE, maximized)?;
        self.conn.flush().map_err(|_| unavailable(WINDOW_GONE))?;

        let deadline = Instant::now() + RESTORE_TIMEOUT;
        loop {
            let current = self.record(id)?;
            if !current.facts.maximized || Instant::now() >= deadline {
                return Ok(current);
            }
            std::thread::sleep(RESTORE_POLL);
        }
    }

    fn move_resize(&self, record: &WindowRecord, request: MoveResize) -> Result<(), WindowError> {
        self.client_message(record.window, self.atoms._NET_MOVERESIZE_WINDOW, request.data())?;
        self.conn.flush().map_err(|_| unavailable(WINDOW_GONE))
    }

    /// Activates a window as a pager would.
    pub fn focus(&self, id: WindowId) {
        let Ok(window) = Window::try_from(id.0) else {
            return;
        };
        let _ = self.client_message(window, self.atoms._NET_ACTIVE_WINDOW, [FROM_PAGER, 0, 0, 0, 0]);
        let _ = self.conn.flush();
    }

    /// Moves keyboard focus off every client, so typing reaches no parked window.
    pub fn activate_self(&self) {
        if let Ok(cookie) = self.conn.set_input_focus(
            InputFocus::POINTER_ROOT,
            InputFocus::POINTER_ROOT,
            x11rb::CURRENT_TIME,
        ) {
            cookie.ignore_error();
        }
        let _ = self.conn.flush();
    }

    pub fn pointer(&self) -> Point {
        self.conn
            .query_pointer(self.screen.root)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map_or(Point::default(), |reply| {
                Point::new(f64::from(reply.root_x), f64::from(reply.root_y))
            })
    }

    fn client_message(&self, window: Window, kind: Atom, data: [u32; 5]) -> Result<(), WindowError> {
        let event = ClientMessageEvent::new(32, window, kind, data);
        let mask = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;
        self.conn
            .send_event(false, self.screen.root, mask, event)
            .map(|cookie| cookie.ignore_error())
            .map_err(|_| unavailable(WINDOW_GONE))
    }
}

/// A window fills a zone by the same rule snapping uses.
fn fills(frame: Rect, zone: Rect) -> bool {
    snappy_core::swap::fills_zone(frame, zone)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn occupants_fill_the_zone_within_tolerance() {
        let zone = Rect::new(12.0, 12.0, 940.0, 1044.0);
        assert!(fills(Rect::new(14.0, 10.0, 936.0, 1048.0), zone));
        assert!(!fills(Rect::new(100.0, 12.0, 852.0, 1044.0), zone));
    }
}
