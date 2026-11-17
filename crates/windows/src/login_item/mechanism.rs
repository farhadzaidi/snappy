//! Picks how Windows opens Snappy at login. A packaged Snappy has to use its package's startup
//! task: a package's registry writes go to a private copy of the user's hive, so Explorer
//! would never see a Run value it wrote.

use crate::login_item::{run_key, startup_task};
use crate::{messages, package};

pub fn is_enabled() -> bool {
    if package::is_packaged() {
        startup_task::is_enabled()
    } else {
        std::env::current_exe().is_ok_and(|executable| run_key::is_enabled(&executable))
    }
}

/// Turns opening at login off for every copy of Snappy, for `snappy --uninstall`. True when
/// it was on.
pub fn remove_any_copy() -> Result<bool, String> {
    if package::is_packaged() {
        let on = startup_task::is_enabled();
        if on {
            startup_task::set_enabled(false)?;
        }
        Ok(on)
    } else {
        run_key::remove_any_copy()
    }
}

pub fn set_enabled(on: bool) -> Result<(), String> {
    if package::is_packaged() {
        startup_task::set_enabled(on)
    } else {
        let executable = std::env::current_exe().map_err(|_| messages::LOGIN_ITEM_NOT_CHANGED.to_owned())?;
        run_key::set_enabled(&executable, on)
    }
}
