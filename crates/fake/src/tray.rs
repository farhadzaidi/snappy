//! The fake tray, which keeps the last menu.

use snappy_platform::{Tray, TrayMenu};

use crate::platform::FakePlatform;

impl Tray for FakePlatform {
    fn update(&mut self, menu: &TrayMenu) {
        self.tray = menu.clone();
    }
}
