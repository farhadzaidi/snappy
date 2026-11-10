//! Other apps' windows, read and moved from the engine thread.

pub mod record;

use std::sync::Arc;
use std::time::{Duration, Instant};

use snappy_core::error::WindowError;
use snappy_core::geometry::{Point, Rect, Size};
use snappy_platform::{WindowId, WindowInfo, WindowState};
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GA_ROOT, GetAncestor, GetForegroundWindow, IsWindow, SET_WINDOW_POS_FLAGS, SW_RESTORE,
    SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER,
    SetForegroundWindow, SetWindowPos, ShowWindowAsync, WindowFromPoint,
};
use windows::core::BOOL;

use self::record::{Record, hwnd, visible_frame, window_rect};
use crate::dispatch::Dispatcher;
use crate::events::cursor;
use crate::frames::{Borders, extent, pixel};
use crate::identity::handle_of;
use crate::messages::{NO_FOCUSED_WINDOW, NOT_STANDARD, WINDOW_GONE, unavailable};
use crate::shared::{Shared, lock};

/// Writes run on the window's own thread. Wait this long for the window to change,
/// rereading this often.
const WRITE_WAIT: Duration = Duration::from_millis(50);
const WRITE_POLL: Duration = Duration::from_millis(1);
/// A workspace move is checked right after it's made. Wait this long for it to land.
const PLACE_TIMEOUT: Duration = Duration::from_millis(250);
const PLACE_POLL: Duration = Duration::from_millis(5);
/// Workspace moves count as landed within this distance.
const PLACE_TOLERANCE: f64 = 3.0;

/// Async positioning keeps a hung window from blocking the engine thread.
const WRITE_FLAGS: SET_WINDOW_POS_FLAGS =
    SET_WINDOW_POS_FLAGS(SWP_NOZORDER.0 | SWP_NOACTIVATE.0 | SWP_NOOWNERZORDER.0 | SWP_ASYNCWINDOWPOS.0);

pub struct Windows {
    shared: Arc<Shared>,
    dispatcher: Dispatcher,
}

unsafe extern "system" fn collect_window(window: HWND, data: LPARAM) -> BOOL {
    // SAFETY: `data` is the vector `top_level` passed, alive for the enumeration.
    let found = unsafe { &mut *(data.0 as *mut Vec<u32>) };
    found.push(record::handle(window));
    BOOL(1)
}

/// Top-level windows, front to back.
fn top_level() -> Vec<u32> {
    let mut handles: Vec<u32> = Vec::new();
    // SAFETY: the callback only pushes into `handles`, which outlives the call.
    let _ = unsafe { EnumWindows(Some(collect_window), LPARAM(&raw mut handles as isize)) };
    handles
}

impl Windows {
    pub fn new(shared: Arc<Shared>, dispatcher: Dispatcher) -> Windows {
        Windows { shared, dispatcher }
    }

    /// The window `id` still names.
    fn resolve(&self, id: WindowId) -> Result<HWND, WindowError> {
        let owner = lock(&self.shared.registry)
            .owner(id)
            .ok_or_else(|| unavailable(WINDOW_GONE))?;
        let window = hwnd(handle_of(id));
        // SAFETY: a plain query.
        let alive = unsafe { IsWindow(Some(window)) }.as_bool();
        if alive && record::pid(window) == Some(owner.pid) {
            Ok(window)
        } else {
            Err(unavailable(WINDOW_GONE))
        }
    }

    fn record(&self, id: WindowId) -> Result<Record, WindowError> {
        let window = self.resolve(id)?;
        record::read(window, &self.shared).ok_or_else(|| unavailable(WINDOW_GONE))
    }

    /// Describes a window and starts reporting its process's moves.
    fn info(&self, record: &Record) -> WindowInfo {
        let (info, minted) = record.describe(&self.shared);
        if minted.new_process {
            let pid = info.pid;
            self.dispatcher.post(move |main| main.hooks.watch_process(pid));
        }
        info
    }

    fn managed(&self, window: HWND) -> Result<WindowInfo, WindowError> {
        let record = record::read(window, &self.shared).ok_or_else(|| unavailable(NOT_STANDARD))?;
        record.facts.check()?;
        Ok(self.info(&record))
    }

    /// The top-level window under `point`.
    pub fn window_at(&self, point: Point) -> Result<WindowInfo, WindowError> {
        let (x, y) = (pixel(point.x), pixel(point.y));
        let (Some(x), Some(y)) = (x, y) else {
            return Err(unavailable(NOT_STANDARD));
        };
        // SAFETY: plain queries.
        let window = unsafe {
            GetAncestor(
                WindowFromPoint(windows::Win32::Foundation::POINT { x, y }),
                GA_ROOT,
            )
        };
        if window.is_invalid() {
            return Err(unavailable(NOT_STANDARD));
        }
        self.managed(window)
    }

    pub fn focused_window(&self) -> Result<WindowInfo, WindowError> {
        // SAFETY: a plain query.
        let window = unsafe { GetForegroundWindow() };
        if window.is_invalid() {
            return Err(unavailable(NO_FOCUSED_WINDOW));
        }
        self.managed(window)
    }

    pub fn frame(&self, id: WindowId) -> Result<Rect, WindowError> {
        visible_frame(self.resolve(id)?).ok_or_else(|| unavailable(WINDOW_GONE))
    }

    pub fn state(&self, id: WindowId) -> Result<WindowState, WindowError> {
        let facts = self.record(id)?.facts;
        Ok(WindowState {
            full_screen: facts.full_screen(),
            has_sheet: facts.has_modal_popup,
            minimized: facts.minimized,
            on_parking_desktop: false,
        })
    }

    pub fn is_closed(&self, id: WindowId) -> bool {
        self.resolve(id).is_err()
    }

    /// Eligible windows, front to back, that pass `keep`.
    fn eligible(&self, keep: impl Fn(&Record) -> bool) -> Vec<WindowInfo> {
        top_level()
            .into_iter()
            .filter_map(|handle| record::read(hwnd(handle), &self.shared))
            .filter(|record| record.facts.check().is_ok() && keep(record))
            .map(|record| self.info(&record))
            .collect()
    }

    /// Eligible windows that aren't minimized, optionally of one app. Cloaked windows,
    /// such as those on other virtual desktops, are never eligible.
    pub fn visible_windows(&self, app: Option<&str>) -> Vec<WindowInfo> {
        self.eligible(|record| !record.facts.minimized && app.is_none_or(|app| record.process.app == app))
    }

    pub fn all_windows(&self) -> Vec<WindowInfo> {
        self.eligible(|_| true)
    }

    /// Visible eligible windows filling any of `frames`.
    pub fn windows_occupying(&self, frames: &[Rect]) -> Vec<WindowInfo> {
        self.eligible(|record| {
            !record.facts.minimized && frames.iter().any(|zone| fills(record.frame, *zone))
        })
    }

    /// Restores a maximized window so the write applies to its restored frame. Both
    /// requests run in order on the window's thread.
    fn restore_if_maximized(&self, window: HWND, record: &Record) {
        if record.facts.maximized {
            // SAFETY: posts the request to the window's thread.
            let _ = unsafe { ShowWindowAsync(window, SW_RESTORE) };
        }
    }

    fn borders(window: HWND) -> Result<Borders, WindowError> {
        let outer = window_rect(window).ok_or_else(|| unavailable(WINDOW_GONE))?;
        let visible = visible_frame(window).ok_or_else(|| unavailable(WINDOW_GONE))?;
        Ok(Borders::between(outer, visible))
    }

    /// Posts a change to the window's thread, then waits briefly for the window to
    /// change, so the settle procedure's first read already sees the result instead of
    /// waiting out its schedule. A hung window only costs the wait.
    fn write(
        window: HWND,
        target: (i32, i32, i32, i32),
        flags: SET_WINDOW_POS_FLAGS,
    ) -> Result<(), WindowError> {
        let before = window_rect(window).ok_or_else(|| unavailable(WINDOW_GONE))?;
        let (x, y, width, height) = target;
        let unchanged = if flags.contains(SWP_NOSIZE) {
            before.x == f64::from(x) && before.y == f64::from(y)
        } else {
            before.width == f64::from(width) && before.height == f64::from(height)
        };
        if unchanged {
            return Ok(());
        }
        // SAFETY: posts the change to the window's thread.
        unsafe { SetWindowPos(window, None, x, y, width, height, WRITE_FLAGS | flags) }
            .map_err(|_| unavailable(WINDOW_GONE))?;
        let deadline = Instant::now() + WRITE_WAIT;
        while window_rect(window) == Some(before) && Instant::now() < deadline {
            std::thread::sleep(WRITE_POLL);
        }
        Ok(())
    }

    pub fn set_position(&self, id: WindowId, origin: Point) -> Result<(), WindowError> {
        let record = self.record(id)?;
        let window = record.hwnd;
        self.restore_if_maximized(window, &record);
        let target = Self::borders(window)?.window_origin(origin);
        let (Some(x), Some(y)) = (pixel(target.x), pixel(target.y)) else {
            return Err(WindowError::InvalidFrame);
        };
        Self::write(window, (x, y, 0, 0), SWP_NOSIZE)
    }

    pub fn set_size(&self, id: WindowId, size: Size) -> Result<(), WindowError> {
        let record = self.record(id)?;
        let window = record.hwnd;
        self.restore_if_maximized(window, &record);
        let target = Self::borders(window)?.window_size(size);
        let (Some(width), Some(height)) = (extent(target.width), extent(target.height)) else {
            return Err(WindowError::InvalidFrame);
        };
        Self::write(window, (0, 0, width, height), SWP_NOMOVE)
    }

    /// Moves a window for a workspace change and waits briefly for the move to land, so
    /// the check that follows sees it.
    pub fn place(&self, id: WindowId, origin: Point) -> Result<(), WindowError> {
        self.set_position(id, origin)?;
        let deadline = Instant::now() + PLACE_TIMEOUT;
        loop {
            let frame = self.frame(id)?;
            let landed =
                (frame.x - origin.x).abs() < PLACE_TOLERANCE && (frame.y - origin.y).abs() < PLACE_TOLERANCE;
            if landed || Instant::now() >= deadline {
                return Ok(());
            }
            std::thread::sleep(PLACE_POLL);
        }
    }

    pub fn focus(&self, id: WindowId) {
        if let Ok(window) = self.resolve(id) {
            // SAFETY: a plain request; Windows refuses it unless input just happened.
            let _ = unsafe { SetForegroundWindow(window) };
        }
    }

    /// Makes Snappy's hidden window the foreground, so typing reaches no app.
    pub fn activate_self(&self) {
        // SAFETY: a plain request for this process's own window.
        let _ = unsafe { SetForegroundWindow(self.dispatcher.window()) };
    }

    pub fn pointer(&self) -> Point {
        cursor()
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
        let zone = Rect::new(12.0, 12.0, 940.0, 1004.0);
        assert!(fills(Rect::new(14.0, 10.0, 936.0, 1008.0), zone));
        assert!(!fills(Rect::new(100.0, 12.0, 852.0, 1004.0), zone));
    }

    #[test]
    fn handles_round_trip_with_sign_extension() {
        assert_eq!(record::handle(hwnd(0x0001_02a4)), 0x0001_02a4);
        assert_eq!(record::handle(hwnd(0xffff_fff0)), 0xffff_fff0);
    }
}
