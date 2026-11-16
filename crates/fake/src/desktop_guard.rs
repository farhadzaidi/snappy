//! The native desktop on the fake platform.

use snappy_platform::{DesktopGuard, Display};

use crate::platform::FakePlatform;

impl DesktopGuard for FakePlatform {
    fn bind(&mut self, _displays: &[Display]) {}

    fn on_managed_desktop(&mut self) -> bool {
        self.on_managed_desktop
    }

    fn clear(&mut self) {}
}
