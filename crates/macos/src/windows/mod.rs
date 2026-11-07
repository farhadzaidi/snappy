//! Other apps' windows through Accessibility, on the engine thread.

mod chrome;
mod enumerate;
mod lookup;
mod operations;
pub(crate) mod server;

use std::time::Duration;

use snappy_core::error::WindowError;
use snappy_core::geometry::Rect;
use snappy_platform::{RecoveryKey, WindowId, WindowInfo};

use crate::apps;
use crate::ax::{AxElement, AxError, names};
use crate::eligibility::{AppFacts, WindowFacts};
use crate::messages::{NOT_RESPONDING, WINDOW_GONE, unavailable};
use crate::observers;
use crate::permissions;
use crate::registry::{self, Entry, Shared};
use crate::thresholds::AX_MESSAGING_TIMEOUT;

use chrome::ChromeWorkaround;

pub struct Windows {
    registry: Shared<AxElement>,
    chrome: ChromeWorkaround,
}

impl Windows {
    pub fn new(registry: Shared<AxElement>) -> Self {
        Self {
            registry,
            chrome: ChromeWorkaround::default(),
        }
    }

    /// A tracked window, set up for single-element calls.
    fn tracked(&self, id: WindowId) -> Result<Entry<AxElement>, WindowError> {
        let entry = registry::lock(&self.registry)
            .get(id)
            .ok_or_else(|| unavailable(WINDOW_GONE))?;
        entry.element.set_timeout(AX_MESSAGING_TIMEOUT);
        Ok(entry)
    }

    /// Prepares the window's app for a bounds change.
    pub fn begin_bounds_change(&mut self, id: WindowId) {
        if let Ok(entry) = self.tracked(id) {
            self.chrome.begin(entry.pid);
        }
    }

    pub fn end_bounds_change(&mut self, id: WindowId) -> Duration {
        match self.tracked(id) {
            Ok(entry) => self.chrome.end(entry.pid),
            Err(_) => Duration::ZERO,
        }
    }

    /// Undoes bounds-change preparations still outstanding, at quit.
    pub fn restore_bounds_changes(&mut self) {
        self.chrome.restore_all();
    }

    /// The window's ID, minting one and observing the window the first time it's seen.
    fn identify(&self, element: &AxElement, pid: i32) -> WindowId {
        let identified = registry::lock(&self.registry).identify(element.clone(), pid);
        if identified.new {
            observers::ax::watch_window(pid, element, identified.id);
        }
        identified.id
    }

    /// Describes a window after checking its eligibility.
    fn info(&self, element: &AxElement, facts: &AppFacts) -> Result<WindowInfo, WindowError> {
        window_facts(element).check()?;
        let frame = element.frame().map_err(gone)?;
        Ok(self.describe(element, frame, facts))
    }

    /// Describes an eligible window whose frame was just read.
    fn describe(&self, element: &AxElement, frame: Rect, facts: &AppFacts) -> WindowInfo {
        WindowInfo {
            id: self.identify(element, facts.pid),
            app: facts.identifier.clone(),
            pid: facts.pid as u32,
            frame,
            recovery_key: recovery_key(element, facts.pid),
        }
    }
}

/// What eligibility needs to know about a window.
fn window_facts(element: &AxElement) -> WindowFacts {
    WindowFacts {
        subrole: element.string_attribute(names::SUBROLE),
        full_screen: element.bool_attribute(names::FULL_SCREEN).unwrap_or(false),
        position_settable: element.is_settable(names::POSITION),
        size_settable: element.is_settable(names::SIZE),
    }
}

/// The process launch and the window's hash identify it across a daemon restart.
fn recovery_key(element: &AxElement, pid: i32) -> Option<RecoveryKey> {
    Some(RecoveryKey {
        pid: pid as u32,
        launch_time: apps::launch_time(pid)?,
        window: element.token().to_string(),
    })
}

/// Treats a failed read as the window being gone, and notices revoked access.
fn gone(error: AxError) -> WindowError {
    notice(error);
    if error.is_timeout() {
        unavailable(NOT_RESPONDING)
    } else {
        unavailable(WINDOW_GONE)
    }
}

/// A `kAXErrorAPIDisabled` result means access was just revoked.
fn notice(error: AxError) {
    if error.is_revocation() {
        permissions::recheck_soon();
    }
}
