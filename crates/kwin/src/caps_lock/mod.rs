//! Caps Lock as the Snappy key on KDE Plasma.
//!
//! - **The key.** Snappy's XKB option (`snappy_linux::xkb`) makes Caps Lock, held, set the
//!   Snappy key's modifiers, and reports one of them as its key, so letting go of it alone
//!   looks like letting go of those modifiers alone. KWin builds its keymap from `kxkbrc`,
//!   so Snappy adds the option there and tells KWin, which reloads the keymap at once.
//! - **Presses.** While held as the Snappy key, KWin's keystate reports when Caps Lock goes
//!   down (`presses`), and a modifier-only shortcut of KDE's shortcut service, bound to the
//!   Snappy key's modifiers, fires when they come up with nothing else used meanwhile. That
//!   is a release of Caps Lock alone; the engine decides whether it was a tap.
//! - **Taps.** Snappy presses the keymap's real Caps Lock, Shift + Caps Lock, through
//!   `fake_input`.
//! - **The lock screen** gets Caps Lock as usual: the option comes out while the session is
//!   locked. When System Settings rewrites the keyboard options without Snappy's, it goes
//!   back in, unless they now remap Caps Lock themselves, which wins.

mod fake_input;
mod kxkbrc;
mod presses;

use serde::{Deserialize, Serialize};
use snappy_core::keys::Mods;
use snappy_linux::journal;
use snappy_linux::xkb::{self, CapsLockSymbol, OPTION};
use snappy_platform::CapsLockUndo;

use self::fake_input::FakeKeyboard;
use self::kxkbrc::OptionEntries;
use self::presses::Presses;
use crate::keystate::{self, Watch};
use crate::messages::{CAPS_LOCK_NOT_SET_UP, caps_lock_remapped};
use crate::shared::{Incoming, Shared};
use crate::{keys, shortcuts};

pub use self::kxkbrc::CHANGED_GROUP;

/// Caps Lock held as the Snappy key, while it is.
pub struct Hold {
    /// The entries before Snappy's, which restoring writes back.
    prior: OptionEntries,
    /// The options Snappy wrote, which tell its own entries from a later change of the user's.
    written: String,
    /// The session is locked, and Caps Lock is as usual meanwhile.
    locked: bool,
    /// The user's own options remap Caps Lock now; theirs win.
    stepped_aside: bool,
    presses: Presses,
    /// Follows the modifier keys, never other keys, while Caps Lock is the Snappy key; dropping
    /// it with the hold closes the connection.
    _watch: Option<Watch>,
    keyboard: Option<FakeKeyboard>,
}

/// What the engine keeps to undo the change after a crash.
#[derive(Debug, Serialize, Deserialize)]
struct Undo {
    prior: OptionEntries,
    written: String,
}

/// The session's own options, which KWin uses while `kxkbrc` doesn't reset them.
fn session_options() -> Option<String> {
    std::env::var("XKB_DEFAULT_OPTIONS").ok()
}

/// Make Caps Lock, held, the Snappy key's `modifiers`, replacing an earlier hold.
pub fn hold(shared: &Shared, modifiers: Mods) -> Result<CapsLockUndo, String> {
    restore(shared);
    let path = kxkbrc::path().ok_or_else(|| CAPS_LOCK_NOT_SET_UP.to_owned())?;
    let text = kxkbrc::load(&path).map_err(not_set_up)?;
    let prior = kxkbrc::read(&text);
    let effective = prior.effective_options(session_options().as_deref());
    let remapping = xkb::caps_lock_options(&effective);
    if !remapping.is_empty() {
        return Err(caps_lock_remapped(&remapping));
    }

    let written = kxkbrc::with_option(&effective, OPTION);
    let undo = Undo {
        prior: prior.clone(),
        written: written.clone(),
    };
    let undo = serde_json::to_string(&undo).map_err(|error| not_set_up(std::io::Error::other(error)))?;

    let folder = xkb::user_folder().ok_or_else(|| CAPS_LOCK_NOT_SET_UP.to_owned())?;
    xkb::install(&folder, modifiers, CapsLockSymbol::OneOfTheModifiers).map_err(not_set_up)?;
    if let Err(error) = fake_input::write_entry() {
        journal::warn(format_args!(
            "taps of Caps Lock may not reach KWin: can't write the applications entry: {error}"
        ));
    }
    let stored = kxkbrc::store(&path, &kxkbrc::write(&text, &OptionEntries::using(&written)));
    if let Err(error) = stored {
        let _ = xkb::uninstall(&folder);
        let _ = fake_input::remove_entry();
        return Err(not_set_up(error));
    }
    notify_kwin(shared);
    shortcuts::register_tap(&shared.bus, keys::modifiers_only(modifiers));

    shared.state().caps_lock = Some(Hold {
        prior,
        written,
        locked: false,
        stepped_aside: false,
        presses: Presses::new(modifiers),
        _watch: watch(shared),
        keyboard: None,
    });
    Ok(CapsLockUndo(undo))
}

fn not_set_up(error: std::io::Error) -> String {
    journal::warn(format_args!("can't make Caps Lock the Snappy key: {error}"));
    CAPS_LOCK_NOT_SET_UP.to_owned()
}

/// Follows the held modifiers from KWin's keystate; without it, taps never toggle.
fn watch(shared: &Shared) -> Option<Watch> {
    let incoming = shared.incoming();
    match keystate::watch(move |mods, _| {
        let _ = incoming.send(Incoming::CapsLockModifiers(mods));
    }) {
        Ok(watch) => Some(watch),
        Err(error) => {
            journal::warn(format_args!(
                "taps of Caps Lock won't turn it on or off: can't read modifier keys from KWin: {error}"
            ));
            None
        }
    }
}

/// Give Caps Lock back its usual behavior.
pub fn restore(shared: &Shared) {
    let Some(hold) = shared.state().caps_lock.take() else {
        return;
    };
    if !hold.locked && !hold.stepped_aside {
        take_option_out(shared, &hold.prior, &hold.written);
    }
    shortcuts::unregister_tap(&shared.bus);
    remove_files();
}

/// Undo what a run that ended without restoring left behind.
pub fn undo(shared: &Shared, undo: &CapsLockUndo) {
    match serde_json::from_str::<Undo>(&undo.0) {
        Ok(left) => take_option_out(shared, &left.prior, &left.written),
        Err(error) => journal::warn(format_args!("can't read what undoes Caps Lock: {error}")),
    }
    shortcuts::unregister_tap(&shared.bus);
    remove_files();
}

fn remove_files() {
    if let Some(folder) = xkb::user_folder()
        && let Err(error) = xkb::uninstall(&folder)
    {
        journal::warn(format_args!("can't remove Snappy's keyboard option: {error}"));
    }
    if let Err(error) = fake_input::remove_entry() {
        journal::warn(format_args!("can't remove Snappy's applications entry: {error}"));
    }
}

/// Takes Snappy's option out of `kxkbrc`. When the entries are still the ones Snappy wrote,
/// `prior` comes back exactly; when the user changed the options since, only Snappy's
/// option goes and their change stays.
fn take_option_out(shared: &Shared, prior: &OptionEntries, written: &str) {
    let Some(path) = kxkbrc::path() else {
        return;
    };
    let Ok(text) = kxkbrc::load(&path) else {
        return;
    };
    let current = kxkbrc::read(&text);
    let options = current.options.clone().unwrap_or_default();
    if !kxkbrc::has_option(&options, OPTION) {
        return;
    }
    let entries = if current == OptionEntries::using(written) {
        prior.clone()
    } else {
        OptionEntries {
            options: Some(kxkbrc::without_option(&options, OPTION)),
            ..current
        }
    };
    match kxkbrc::store(&path, &kxkbrc::write(&text, &entries)) {
        Ok(()) => notify_kwin(shared),
        Err(error) => journal::warn(format_args!("can't take Snappy's option out of kxkbrc: {error}")),
    }
}

/// Puts Snappy's option back into `kxkbrc`, taking the entries there now as the ones to
/// restore. Returns false, leaving the file alone, when the options there remap Caps Lock.
fn put_option_back(shared: &Shared, hold: &mut Hold) -> bool {
    let Some(path) = kxkbrc::path() else {
        return false;
    };
    let Ok(text) = kxkbrc::load(&path) else {
        return false;
    };
    let current = kxkbrc::read(&text);
    let effective = current.effective_options(session_options().as_deref());
    if !xkb::caps_lock_options(&effective).is_empty() {
        return false;
    }
    let written = kxkbrc::with_option(&effective, OPTION);
    match kxkbrc::store(&path, &kxkbrc::write(&text, &OptionEntries::using(&written))) {
        Ok(()) => {
            hold.prior = current;
            hold.written = written;
            notify_kwin(shared);
            true
        }
        Err(error) => {
            journal::warn(format_args!(
                "can't put Snappy's option back into kxkbrc: {error}"
            ));
            false
        }
    }
}

/// KConfig's change notification, which KWin's keyboard layout watches to build the keymap
/// again.
fn notify_kwin(shared: &Shared) {
    if let Err(error) = shared.bus.notify_config_changed(&kxkbrc::CHANGED_KEYS) {
        journal::warn(format_args!(
            "can't tell KWin the keyboard options changed: {error:?}"
        ));
    }
}

/// The session was locked or unlocked. The lock screen gets Caps Lock as usual.
pub fn screen_locked(shared: &Shared, locked: bool) {
    let mut state = shared.state();
    let Some(hold) = state.caps_lock.as_mut() else {
        return;
    };
    if hold.locked == locked || hold.stepped_aside {
        return;
    }
    hold.locked = locked;
    if locked {
        take_option_out(shared, &hold.prior, &hold.written);
    } else if !put_option_back(shared, hold) {
        step_aside(hold);
    }
}

/// Someone else changed `kxkbrc`'s layout entries, as System Settings does when the keyboard
/// settings are applied. Snappy's option goes back in when it was dropped, unless the
/// options now remap Caps Lock.
pub fn settings_changed(shared: &Shared) {
    let mut state = shared.state();
    let Some(hold) = state.caps_lock.as_mut() else {
        return;
    };
    if hold.locked || hold.stepped_aside {
        return;
    }
    let still_there = kxkbrc::path()
        .and_then(|path| kxkbrc::load(&path).ok())
        .map(|text| kxkbrc::read(&text))
        .is_some_and(|entries| {
            entries.resets_old_options()
                && kxkbrc::has_option(entries.options.as_deref().unwrap_or(""), OPTION)
        });
    if !still_there && !put_option_back(shared, hold) {
        step_aside(hold);
    }
}

fn step_aside(hold: &mut Hold) {
    hold.stepped_aside = true;
    journal::warn(
        "the keyboard options now remap Caps Lock themselves, so Caps Lock is no longer the Snappy key",
    );
}

/// The held modifiers, from the keystate watch. True when Caps Lock went down.
pub fn modifiers_reported(shared: &Shared, held: Mods) -> bool {
    shared
        .state()
        .caps_lock
        .as_mut()
        .is_some_and(|hold| !hold.locked && hold.presses.report(held))
}

/// The modifier-only shortcut fired. True when that was Caps Lock coming up alone.
pub fn released_alone(shared: &Shared) -> bool {
    shared
        .state()
        .caps_lock
        .as_mut()
        .is_some_and(|hold| hold.presses.released_alone())
}

/// Turn Caps Lock on or off by pressing the keymap's real Caps Lock.
pub fn toggle(shared: &Shared) {
    let mut state = shared.state();
    let Some(hold) = state.caps_lock.as_mut() else {
        return;
    };
    // A connection KWin dropped, as when it started over, is opened again once.
    for _ in 0..2 {
        if hold.keyboard.is_none() {
            match FakeKeyboard::open() {
                Ok(keyboard) => hold.keyboard = Some(keyboard),
                Err(error) => {
                    journal::warn(format_args!(
                        "can't turn Caps Lock on or off through KWin: {error}"
                    ));
                    return;
                }
            }
        }
        match hold.keyboard.as_mut().map(FakeKeyboard::press_caps_lock) {
            Some(Ok(())) => return,
            _ => hold.keyboard = None,
        }
    }
    journal::warn("KWin didn't take the Caps Lock press");
}
