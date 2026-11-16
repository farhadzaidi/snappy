//! Watching primary-button presses, drags, and modifier changes.

use snappy_core::keys::Modifier;

pub trait Input {
    /// Primary-button presses and releases.
    fn set_press_monitoring(&mut self, on: bool);
    /// Drags and modifier changes; on only during drags.
    fn set_drag_monitoring(&mut self, on: bool);
    /// The key that makes a drag a Snappy drag. A window manager that reacts to that key
    /// during its own moves, as GNOME pulls windows onto nearby edges while Shift is held,
    /// keeps the window under the pointer instead.
    fn set_drag_modifier(&mut self, _modifier: Modifier) {}
}
