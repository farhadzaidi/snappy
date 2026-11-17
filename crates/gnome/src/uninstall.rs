//! Removing Snappy's GNOME Shell extension for `snappy --uninstall`. In a running GNOME
//! session, GNOME Shell itself turns the extension off, which brings back any window it
//! still hid, and deletes it; elsewhere its folder is deleted, and either way it comes off
//! GNOME's lists of extensions.

use snappy_linux::journal;
use snappy_platform::uninstall::Removal;

use crate::bus::Bus;
use crate::extension;
use crate::messages::{EXTENSION_NOT_REMOVED, EXTENSION_REMOVED};
use crate::protocol::EXTENSION_UUID;

pub fn remove_extension() -> Removal {
    let Some(folder) = extension::install_folder() else {
        return Removal::Nothing;
    };
    let installed = folder.exists();
    if installed {
        // Outside a desktop session there's no bus, and deleting the folder is all there is.
        if let Ok(bus) = Bus::session()
            && let Err(error) = bus.uninstall_extension(EXTENSION_UUID)
        {
            journal::warn(format_args!("GNOME Shell didn't remove the extension: {error:?}"));
        }
    }
    let deleted = match std::fs::remove_dir_all(&folder) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Err(error) => {
            journal::warn(format_args!("can't delete {}: {error}", folder.display()));
            false
        }
    };
    let forgotten = extension::forget_in_settings().unwrap_or_else(|error| {
        journal::warn(format_args!(
            "can't take the extension off GNOME's settings: {error}"
        ));
        false
    });
    match (deleted, installed || forgotten) {
        (false, _) => Removal::Failed(EXTENSION_NOT_REMOVED.to_owned()),
        (true, true) => Removal::Removed(EXTENSION_REMOVED.to_owned()),
        (true, false) => Removal::Nothing,
    }
}
