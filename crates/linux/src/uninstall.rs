//! What `snappy --uninstall` removes that every Linux adapter may have added: the autostart
//! entry and the icon it names, and Snappy's XKB option. Warnings go to the daemon's output;
//! the sentences say only what happened.

use snappy_platform::uninstall::Removal;

use crate::messages::{
    KEYBOARD_OPTION_NOT_REMOVED, KEYBOARD_OPTION_REMOVED, LOGIN_ITEM_NOT_REMOVED, LOGIN_ITEM_REMOVED,
};
use crate::{app_icon, journal, login_item, xkb};

/// Removes the autostart entry, and the icon in the user's icon theme that it and other
/// entries Snappy writes name.
pub fn remove_login_item() -> Removal {
    if let Err(error) = app_icon::remove() {
        journal::warn(format_args!("can't remove the app icon: {error}"));
    }
    let Some(path) = login_item::entry_path().filter(|path| path.exists()) else {
        return Removal::Nothing;
    };
    match std::fs::remove_file(&path) {
        Ok(()) => Removal::Removed(LOGIN_ITEM_REMOVED.to_owned()),
        Err(error) => {
            journal::warn(format_args!("can't remove {}: {error}", path.display()));
            Removal::Failed(LOGIN_ITEM_NOT_REMOVED.to_owned())
        }
    }
}

/// Takes Snappy's XKB option out of the user's XKB folder, keeping the user's own rules.
pub fn remove_keyboard_option() -> Removal {
    let Some(folder) = xkb::user_folder().filter(|folder| xkb::is_installed(folder)) else {
        return Removal::Nothing;
    };
    match xkb::uninstall(&folder) {
        Ok(()) => Removal::Removed(KEYBOARD_OPTION_REMOVED.to_owned()),
        Err(error) => {
            journal::warn(format_args!("can't remove Snappy's keyboard option: {error}"));
            Removal::Failed(KEYBOARD_OPTION_NOT_REMOVED.to_owned())
        }
    }
}
