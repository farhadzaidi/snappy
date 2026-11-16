//! The login item on the fake platform.

use snappy_platform::LoginItem;

use crate::platform::FakePlatform;

impl LoginItem for FakePlatform {
    fn is_enabled(&mut self) -> bool {
        self.login_item
    }

    fn set_enabled(&mut self, on: bool) -> Result<(), String> {
        if let Some(error) = &self.login_item_error {
            return Err(error.clone());
        }
        self.login_item = on;
        Ok(())
    }
}
