//! Press and drag monitoring on the fake platform.

use snappy_core::keys::Modifier;
use snappy_platform::Input;

use crate::platform::FakePlatform;

impl Input for FakePlatform {
    fn set_press_monitoring(&mut self, on: bool) {
        self.press_monitoring = on;
    }

    fn set_drag_monitoring(&mut self, on: bool) {
        self.drag_monitoring = on;
    }

    fn set_drag_modifier(&mut self, modifier: Modifier) {
        self.drag_modifier = Some(modifier);
    }
}
