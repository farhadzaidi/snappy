//! Finding one window: the one under the pointer, or the focused one.

use snappy_core::error::WindowError;
use snappy_core::geometry::Point;
use snappy_platform::WindowInfo;

use super::Windows;
use crate::apps;
use crate::ax::{self, AxElement, names};
use crate::messages::{NO_FOCUSED_WINDOW, NO_PERMISSION, NOT_STANDARD, unavailable};
use crate::thresholds::{AX_MESSAGING_TIMEOUT, POINTER_HIT_WALK};

impl Windows {
    /// The managed window under a top-left global point.
    pub fn window_at(&self, point: Point) -> Result<WindowInfo, WindowError> {
        require_trust()?;
        let system = AxElement::system_wide(AX_MESSAGING_TIMEOUT).ok_or_else(not_standard)?;
        let hit = system.element_at(point).map_err(|_| not_standard())?;
        hit.set_timeout(AX_MESSAGING_TIMEOUT);
        let pid = hit.pid().ok_or_else(not_standard)?;
        let app = apps::running(pid).ok_or_else(not_standard)?;
        let facts = apps::facts(&app);
        facts.check()?;
        let window = enclosing_window(hit).ok_or_else(not_standard)?;
        self.info(&window, &facts)
    }

    /// The frontmost app's focused window.
    pub fn focused_window(&self) -> Result<WindowInfo, WindowError> {
        require_trust()?;
        let app = apps::frontmost().ok_or_else(no_focused)?;
        let facts = apps::facts(&app);
        if facts.is_self {
            return Err(no_focused());
        }
        facts.check()?;
        let window = AxElement::application(facts.pid, AX_MESSAGING_TIMEOUT)
            .and_then(|application| application.element_attribute(names::FOCUSED_WINDOW))
            .ok_or_else(no_focused)?;
        self.info(&window, &facts)
    }
}

/// Walks up from a hit element to its window: an `AXWindow`, else the element's
/// `AXWindow` attribute, else its parent.
fn enclosing_window(hit: AxElement) -> Option<AxElement> {
    let mut current = hit;
    for _ in 0..POINTER_HIT_WALK {
        if current.string_attribute(names::ROLE).as_deref() == Some(names::WINDOW_ROLE) {
            return Some(current);
        }
        if let Some(window) = current.element_attribute(names::WINDOW) {
            return Some(window);
        }
        current = current.element_attribute(names::PARENT)?;
    }
    None
}

fn require_trust() -> Result<(), WindowError> {
    if ax::is_trusted() {
        Ok(())
    } else {
        Err(unavailable(NO_PERMISSION))
    }
}

fn not_standard() -> WindowError {
    unavailable(NOT_STANDARD)
}

fn no_focused() -> WindowError {
    unavailable(NO_FOCUSED_WINDOW)
}
