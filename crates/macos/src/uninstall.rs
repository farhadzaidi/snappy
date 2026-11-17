//! What `snappy --uninstall` removes on macOS once the daemon is gone: the login item, which
//! only a program inside the app can unregister. Quitting already gave Caps Lock back.

use snappy_platform::uninstall::Removal;

use crate::login_item;
use crate::messages::{LOGIN_ITEM_NOT_REMOVED, LOGIN_ITEM_REMOVED};

pub fn remove_login_item() -> Removal {
    if !login_item::is_enabled() {
        return Removal::Nothing;
    }
    match login_item::set_enabled(false) {
        Ok(()) => Removal::Removed(LOGIN_ITEM_REMOVED.to_owned()),
        Err(error) => {
            eprintln!("snappy: can't unregister the login item: {error}");
            Removal::Failed(LOGIN_ITEM_NOT_REMOVED.to_owned())
        }
    }
}
