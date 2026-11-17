//! What `snappy --uninstall` removes from KDE Plasma once the daemon is gone. Quitting
//! already takes Caps Lock's option out of `kxkbrc`; what's left here is an option a run that
//! couldn't quit left behind, the applications entry that lets Snappy toggle Caps Lock, and
//! what KDE's shortcut service keeps of Snappy's.

use snappy_linux::journal;
use snappy_platform::uninstall::Removal;
use zbus::blocking::Connection;

use crate::bus;
use crate::caps_lock::{self, CHANGED_KEYS};
use crate::messages::{
    KEYBOARD_SETTINGS_NOT_RESTORED, KEYBOARD_SETTINGS_RESTORED, SHORTCUTS_NOT_REMOVED, SHORTCUTS_REMOVED,
};

/// Takes Snappy's option out of `kxkbrc`, telling a running KWin, and removes the
/// applications entry.
pub fn remove_keyboard_settings() -> Removal {
    if caps_lock::applications_entry_path().is_some_and(|path| path.exists())
        && let Err(error) = caps_lock::remove_applications_entry()
    {
        journal::warn(format_args!("can't remove Snappy's applications entry: {error}"));
    }
    match caps_lock::remove_option(None) {
        Ok(false) => Removal::Nothing,
        Ok(true) => {
            if let Some(conn) = session()
                && let Err(error) = bus::notify_config_changed(&conn, &CHANGED_KEYS)
            {
                journal::warn(format_args!(
                    "can't tell KWin the keyboard options changed: {error:?}"
                ));
            }
            Removal::Removed(KEYBOARD_SETTINGS_RESTORED.to_owned())
        }
        Err(error) => {
            journal::warn(format_args!("can't take Snappy's option out of kxkbrc: {error}"));
            Removal::Failed(KEYBOARD_SETTINGS_NOT_RESTORED.to_owned())
        }
    }
}

/// Has KDE's shortcut service, when it runs, forget Snappy's shortcuts.
pub fn remove_shortcuts() -> Removal {
    let Some(conn) = session().filter(has_shortcut_service) else {
        return Removal::Nothing;
    };
    match bus::forget_shortcuts(&conn) {
        Ok(true) => Removal::Removed(SHORTCUTS_REMOVED.to_owned()),
        Ok(false) => Removal::Nothing,
        Err(error) => {
            journal::warn(format_args!("KDE's shortcut service didn't answer: {error:?}"));
            Removal::Failed(SHORTCUTS_NOT_REMOVED.to_owned())
        }
    }
}

/// The session bus, when there is one, as in a desktop session.
fn session() -> Option<Connection> {
    Connection::session().ok()
}

/// Whether KDE's shortcut service is on the bus, without starting it.
fn has_shortcut_service(conn: &Connection) -> bool {
    let Ok(name) = bus::GLOBAL_ACCEL_NAME.try_into() else {
        return false;
    };
    zbus::blocking::fdo::DBusProxy::new(conn).is_ok_and(|proxy| proxy.name_has_owner(name).unwrap_or(false))
}
