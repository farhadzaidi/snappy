//! The permission grant on the fake platform.

use snappy_platform::Permissions;

use crate::platform::FakePlatform;

impl Permissions for FakePlatform {
    fn is_granted(&mut self) -> bool {
        self.granted
    }

    fn request_access(&mut self) -> bool {
        self.access_requests += 1;
        true
    }

    fn open_settings_page(&mut self) -> bool {
        self.settings_page_opens += 1;
        true
    }

    fn watch(&mut self, waiting_for_grant: bool) {
        self.waiting_for_grant = waiting_for_grant;
    }
}
