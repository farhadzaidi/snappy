//! Caps Lock as the Snappy key on GNOME. Snappy writes an XKB option of its own
//! (`snappy_linux::xkb`), and the extension adds it to the keymap GNOME Shell builds, reports
//! presses of Caps Lock alone, and turns Caps Lock on or off. Nothing outlives GNOME Shell's
//! keymap but the option's files, and the extension takes the option out itself when the
//! daemon goes away.

use snappy_core::keys::Mods;
use snappy_linux::journal;
use snappy_linux::xkb::{self, CapsLockSymbol};

use crate::bus::CallError;
use crate::keys;
use crate::messages::{CAPS_LOCK_UNSUPPORTED, SHELL_UNREACHABLE, UNEXPECTED_REPLY, caps_lock_not_written};
use crate::protocol::{Request, UNSUPPORTED};
use crate::shared::Shared;

/// What a Caps Lock request's failure tells the user.
fn refusal(error: CallError) -> String {
    match error {
        CallError::Refused(reason) if reason == UNSUPPORTED => CAPS_LOCK_UNSUPPORTED.to_owned(),
        CallError::Unreachable(_) => SHELL_UNREACHABLE.to_owned(),
        CallError::Refused(_) | CallError::Malformed(_) => UNEXPECTED_REPLY.to_owned(),
    }
}

/// Checks the user's own options and the keymap's Caps Lock, writes Snappy's XKB option for
/// `modifiers`, and has the extension add it to the keymap. A refusal leaves nothing behind.
pub fn hold(shared: &Shared, modifiers: Mods) -> Result<(), String> {
    let held = add_option(shared, modifiers);
    if held.is_err() {
        remove(shared);
    }
    held
}

fn add_option(shared: &Shared, modifiers: Mods) -> Result<(), String> {
    let options: String = shared.bus.call(&Request::KeyboardOptions).map_err(refusal)?;
    if let Some(reason) = xkb::options_refusal(&options) {
        return Err(reason);
    }
    if let Some(reason) = xkb::layout_refusal() {
        return Err(reason.to_owned());
    }
    let folder = xkb::user_folder().ok_or_else(|| {
        caps_lock_not_written(&std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no home folder",
        ))
    })?;
    xkb::install(&folder, modifiers, CapsLockSymbol::Nothing)
        .map_err(|error| caps_lock_not_written(&error))?;
    let hold = Request::CapsLockHold {
        modifiers: keys::modifier_prefix(modifiers),
        mask: keys::mask_of(modifiers),
        keycodes: keys::keys_while_caps_lock_is_down(),
    };
    shared.bus.send(&hold).map_err(refusal)?;
    shared.state().caps_lock = Some(hold);
    Ok(())
}

/// Takes Snappy's option out of the keymap, then its files out of the XKB folder.
pub fn remove(shared: &Shared) {
    shared.state().caps_lock = None;
    let _ = shared.bus.send(&Request::CapsLockRestore);
    if let Some(folder) = xkb::user_folder()
        && let Err(error) = xkb::uninstall(&folder)
    {
        journal::warn(format_args!("can't remove Snappy's keyboard option: {error}"));
    }
}

/// The user's own XKB options changed, as GNOME Shell built them, while Snappy's option is in
/// the keymap. When they now remap Caps Lock, theirs win: Snappy's option comes out, and the
/// return is true for the engine to hear Caps Lock was lost.
pub fn options_changed(shared: &Shared, options: &str) -> bool {
    if shared.state().caps_lock.is_none() || xkb::options_refusal(options).is_none() {
        return false;
    }
    remove(shared);
    true
}

/// Turn Caps Lock on or off as its key does.
pub fn toggle(shared: &Shared) -> Result<(), String> {
    shared
        .bus
        .send(&Request::CapsLockToggle)
        .map_err(|error| format!("{error:?}"))
}
