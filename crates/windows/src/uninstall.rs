//! What `snappy --uninstall` removes on Windows once the daemon is gone: Open at login.
//! Caps Lock changes nothing in the system, so there's nothing to give back.

use snappy_platform::uninstall::Removal;

use crate::login_item;
use crate::messages::{LOGIN_ITEM_NOT_REMOVED, LOGIN_ITEM_REMOVED};

pub fn remove_login_item() -> Removal {
    match login_item::remove_any_copy() {
        Ok(true) => Removal::Removed(LOGIN_ITEM_REMOVED.to_owned()),
        Ok(false) => Removal::Nothing,
        Err(_) => Removal::Failed(LOGIN_ITEM_NOT_REMOVED.to_owned()),
    }
}
