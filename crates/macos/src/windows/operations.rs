//! Reading and changing one tracked window.

use objc2_app_kit::NSApplicationActivationOptions;
use snappy_core::error::WindowError;
use snappy_core::geometry::{Point, Rect, Size};
use snappy_platform::{WindowId, WindowState};

use super::{Windows, gone, notice};
use crate::ax::{self, AxElement, AxError, names};
use crate::messages::{CLOSED_OR_REVOKED, UNAVAILABLE_OR_FULL_SCREEN, declined, unavailable};
use crate::registry::Entry;
use crate::thresholds::AX_MESSAGING_TIMEOUT;
use crate::{apps, main_thread};

impl Windows {
    pub fn frame(&self, id: WindowId) -> Result<Rect, WindowError> {
        let element = self.tracked(id)?.element;
        // A busy app, for example one applying a minimum size, can miss the messaging
        // timeout once. Ask again before giving up.
        match element.frame() {
            Err(error) if error.is_timeout() => element.frame(),
            read => read,
        }
        .map_err(gone)
    }

    pub fn set_size(&self, id: WindowId, size: Size) -> Result<(), WindowError> {
        let entry = self.writable(id)?;
        entry.element.set_size(size).map_err(write_error)
    }

    pub fn set_position(&self, id: WindowId, origin: Point) -> Result<(), WindowError> {
        let entry = self.writable(id)?;
        entry.element.set_position(origin).map_err(write_error)
    }

    /// Confirms access, a running app, and a window that isn't full screen before each
    /// write.
    fn writable(&self, id: WindowId) -> Result<Entry<AxElement>, WindowError> {
        let entry = self.tracked(id)?;
        let full_screen = entry.element.bool_attribute(names::FULL_SCREEN).unwrap_or(false);
        if !ax::is_trusted() || !apps::is_running(entry.pid) || full_screen {
            return Err(unavailable(UNAVAILABLE_OR_FULL_SCREEN));
        }
        Ok(entry)
    }

    pub fn state(&self, id: WindowId) -> Result<WindowState, WindowError> {
        let window = self.tracked(id)?.element;
        window.attribute(names::ROLE).map_err(gone)?;
        Ok(WindowState {
            full_screen: window.bool_attribute(names::FULL_SCREEN).unwrap_or(false),
            has_sheet: has_sheet(&window),
            minimized: window.bool_attribute(names::MINIMIZED).unwrap_or(false),
            on_parking_desktop: false,
        })
    }

    /// Closed when the process is gone or the element no longer exists.
    pub fn is_closed(&self, id: WindowId) -> bool {
        let Ok(entry) = self.tracked(id) else { return true };
        if !apps::is_running(entry.pid) {
            return true;
        }
        entry
            .element
            .attribute(names::ROLE)
            .is_err_and(|error| error.is_gone())
    }

    /// Activates the app, raises the window, and makes it the app's focused window.
    /// Minimized windows are left alone.
    pub fn focus(&self, id: WindowId) {
        let Ok(entry) = self.tracked(id) else { return };
        if entry.element.bool_attribute(names::MINIMIZED).unwrap_or(false) {
            return;
        }
        let pid = entry.pid;
        main_thread::sync(move |_| {
            if let Some(app) = apps::running(pid) {
                app.activateWithOptions(NSApplicationActivationOptions::empty());
            }
        });
        let _ = entry.element.perform(names::RAISE_ACTION);
        if let Some(application) = AxElement::application(pid, AX_MESSAGING_TIMEOUT) {
            let _ = application.set_attribute(names::FOCUSED_WINDOW, entry.element.as_cf());
        }
    }
}

/// Whether any child of the window is a sheet.
pub fn has_sheet(window: &AxElement) -> bool {
    window
        .elements_attribute(names::CHILDREN)
        .unwrap_or_default()
        .iter()
        .any(|child| {
            child.set_timeout(AX_MESSAGING_TIMEOUT);
            child.string_attribute(names::ROLE).as_deref() == Some(names::SHEET_ROLE)
        })
}

fn write_error(error: AxError) -> WindowError {
    notice(error);
    if error.is_gone() || error.is_revocation() {
        unavailable(CLOSED_OR_REVOKED)
    } else {
        declined(error.0)
    }
}
