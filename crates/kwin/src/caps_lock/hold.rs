//! Holding Caps Lock as the Snappy key and giving it back: Snappy's option in and out of
//! `kxkbrc`, the lock screen, and the presses and taps while it is held.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use snappy_core::keys::Mods;
use snappy_linux::journal;
use snappy_linux::wayland::Keymaps;
use snappy_linux::xkb::{self, CapsLockKey, CapsLockSymbol, OPTION};
use snappy_platform::{CapsLockRefusal, CapsLockUndo};

use super::fake_input::{self, FakeKeyboard};
use super::kxkbrc::{self, OptionEntries};
use super::presses::Presses;
use crate::input::keystate::{self, Watch};
use crate::messages::{CAPS_LOCK_AFTER_LOGIN, CAPS_LOCK_NOT_SET_UP};
use crate::shared::{Incoming, Shared};
use crate::{hotkeys, keys};

/// How long KWin may take to build its keymap again after hearing the options changed.
const KEYMAP_RELOAD_TIMEOUT: Duration = Duration::from_secs(2);

/// Caps Lock held as the Snappy key, while it is.
pub struct Hold {
    /// The entries before Snappy's, which restoring writes back.
    prior: OptionEntries,
    /// The options Snappy wrote, which tell its own entries from a later change of the user's.
    written: String,
    /// The session is locked, and Caps Lock is as usual meanwhile.
    locked: bool,
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

/// Makes the user's XKB folder if it isn't there. KWin searches it only if it existed when
/// KWin started, so a folder made now lets Caps Lock become the Snappy key without waiting
/// once the user has logged in again, as most have by the time they turn it on.
pub fn prepare_xkb_folder() {
    if let Some(folder) = xkb::user_folder()
        && let Err(error) = std::fs::create_dir_all(&folder)
    {
        journal::warn(format_args!("can't make the XKB folder for Caps Lock: {error}"));
    }
}

/// Make Caps Lock, held, the Snappy key's `modifiers`, replacing an earlier hold.
pub fn hold(shared: &Shared, modifiers: Mods) -> Result<CapsLockUndo, CapsLockRefusal> {
    restore(shared);
    let path = kxkbrc::path().ok_or_else(|| CapsLockRefusal::Refused(CAPS_LOCK_NOT_SET_UP.to_owned()))?;
    let text = kxkbrc::load(&path).map_err(not_set_up)?;
    let prior = kxkbrc::read(&text);
    let effective = prior.effective_options(session_options().as_deref());
    if let Some(reason) = xkb::options_refusal(&effective) {
        return Err(CapsLockRefusal::Refused(reason));
    }
    if let Some(reason) = xkb::layout_refusal() {
        return Err(CapsLockRefusal::Refused(reason.to_owned()));
    }

    let written = kxkbrc::with_option(&effective, OPTION);
    let undo = Undo {
        prior: prior.clone(),
        written: written.clone(),
    };
    let undo = CapsLockUndo::of(&undo).map_err(|error| not_set_up(std::io::Error::other(error)))?;

    let folder =
        xkb::user_folder().ok_or_else(|| CapsLockRefusal::Refused(CAPS_LOCK_NOT_SET_UP.to_owned()))?;
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
    // Opened before KWin hears of the change, so the keymap it builds next can't be missed.
    let keymaps = watch_keymaps();
    notify_kwin(shared);
    if keymaps.is_some_and(|keymaps| !kwin_took_option(keymaps)) {
        take_option_out(shared, &prior, &written);
        remove_files();
        return Err(CapsLockRefusal::AfterLogin(CAPS_LOCK_AFTER_LOGIN.to_owned()));
    }
    hotkeys::register_tap(&shared.bus, keys::modifiers_only(modifiers));

    shared.state().caps_lock = Some(Hold {
        prior,
        written,
        locked: false,
        presses: Presses::new(modifiers),
        _watch: watch(shared),
        keyboard: None,
    });
    Ok(undo)
}

/// Whether the keymap KWin builds next, after hearing the options changed, has Snappy's
/// option. KWin's XKB context searches the user's XKB folder only if it existed as KWin
/// started, so an option written into a folder made since is ignored until the next login.
/// A keymap that can't be read is taken as having it, with a warning.
fn kwin_took_option(mut keymaps: Keymaps) -> bool {
    match keymaps.next(KEYMAP_RELOAD_TIMEOUT) {
        Ok(keymap) => xkb::caps_lock_key(&keymap) == Some(CapsLockKey::Snappys),
        Err(error) if error.kind() == std::io::ErrorKind::TimedOut => false,
        Err(error) => {
            journal::warn(format_args!(
                "can't read the keymap to check KWin took Snappy's keyboard option: {error}"
            ));
            true
        }
    }
}

/// A keyboard that hears the keymaps KWin builds; `None`, with a warning, when there is none.
fn watch_keymaps() -> Option<Keymaps> {
    Keymaps::open()
        .map(|(keymaps, _)| keymaps)
        .inspect_err(|error| {
            journal::warn(format_args!(
                "can't read the keymap to check KWin takes Snappy's keyboard option: {error}"
            ));
        })
        .ok()
}

fn not_set_up(error: std::io::Error) -> CapsLockRefusal {
    journal::warn(format_args!("can't make Caps Lock the Snappy key: {error}"));
    CapsLockRefusal::Refused(CAPS_LOCK_NOT_SET_UP.to_owned())
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
    if !hold.locked {
        take_option_out(shared, &hold.prior, &hold.written);
    }
    hotkeys::unregister_tap(&shared.bus);
    remove_files();
}

/// Let go of Caps Lock after its option left `kxkbrc` without Snappy taking it out, so the
/// keyboard settings there now are someone else's and stay as they are. False when it
/// wasn't held, as when restoring got there first.
fn let_go(shared: &Shared) -> bool {
    let held = shared.state().caps_lock.take().is_some();
    if held {
        hotkeys::unregister_tap(&shared.bus);
        remove_files();
    }
    held
}

/// Undo what a run that ended without restoring left behind.
pub fn undo(shared: &Shared, undo: &CapsLockUndo) {
    match undo.read::<Undo>() {
        Ok(left) => take_option_out(shared, &left.prior, &left.written),
        Err(error) => journal::warn(format_args!("can't read what undoes Caps Lock: {error}")),
    }
    hotkeys::unregister_tap(&shared.bus);
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

/// Takes Snappy's option out of `kxkbrc` and tells KWin.
fn take_option_out(shared: &Shared, prior: &OptionEntries, written: &str) {
    match remove_option(Some((prior, written))) {
        Ok(true) => notify_kwin(shared),
        Ok(false) => {}
        Err(error) => journal::warn(format_args!("can't take Snappy's option out of kxkbrc: {error}")),
    }
}

/// Takes Snappy's option out of `kxkbrc`, without telling KWin. Given `left`, the entries
/// before Snappy's and the options it wrote, and when the entries are still the ones Snappy
/// wrote, the earlier entries come back exactly; otherwise, as when the user changed the
/// options since, only Snappy's option goes and the rest stays. True when the file changed.
pub fn remove_option(left: Option<(&OptionEntries, &str)>) -> std::io::Result<bool> {
    let Some(path) = kxkbrc::path() else {
        return Ok(false);
    };
    let text = kxkbrc::load(&path)?;
    let current = kxkbrc::read(&text);
    let options = current.options.clone().unwrap_or_default();
    if !kxkbrc::has_option(&options, OPTION) {
        return Ok(false);
    }
    let entries = match left {
        Some((prior, written)) if current == OptionEntries::using(written) => prior.clone(),
        _ => OptionEntries {
            options: Some(kxkbrc::without_option(&options, OPTION)),
            ..current
        },
    };
    kxkbrc::store(&path, &kxkbrc::write(&text, &entries))?;
    Ok(true)
}

/// Puts Snappy's entries back as `hold` wrote them, over the `prior` ones taking the option
/// out left, so the engine's undo record still holds. Returns false, leaving the file
/// alone, when the entries there are no longer those: the keyboard settings changed
/// meanwhile.
fn put_option_back(shared: &Shared, hold: &Hold) -> bool {
    let Some(path) = kxkbrc::path() else {
        return false;
    };
    let Ok(text) = kxkbrc::load(&path) else {
        return false;
    };
    if kxkbrc::read(&text) != hold.prior {
        return false;
    }
    match kxkbrc::store(&path, &kxkbrc::write(&text, &OptionEntries::using(&hold.written))) {
        Ok(()) => {
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

/// The session was locked or unlocked. The lock screen gets Caps Lock as usual. True when
/// the keyboard settings changed while it was locked, so Caps Lock was let go on unlocking
/// and the engine has to hear it was lost.
pub fn screen_locked(shared: &Shared, locked: bool) -> bool {
    let mut state = shared.state();
    let Some(hold) = state.caps_lock.as_mut() else {
        return false;
    };
    if hold.locked == locked {
        return false;
    }
    hold.locked = locked;
    if locked {
        take_option_out(shared, &hold.prior, &hold.written);
        return false;
    }
    let back = put_option_back(shared, hold);
    drop(state);
    !back && let_go(shared)
}

/// Someone else changed `kxkbrc`'s layout entries, as System Settings does when the keyboard
/// settings are applied. True when they dropped Snappy's option, so Caps Lock was let go and
/// the engine has to hear it was lost.
pub fn settings_changed(shared: &Shared) -> bool {
    if shared.state().caps_lock.as_ref().is_none_or(|hold| hold.locked) {
        return false;
    }
    let still_there = kxkbrc::path()
        .and_then(|path| kxkbrc::load(&path).ok())
        .map(|text| kxkbrc::read(&text))
        .is_some_and(|entries| {
            entries.resets_old_options()
                && kxkbrc::has_option(entries.options.as_deref().unwrap_or(""), OPTION)
        });
    !still_there && let_go(shared)
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
pub fn toggle(shared: &Shared) -> Result<(), String> {
    let mut state = shared.state();
    let hold = state
        .caps_lock
        .as_mut()
        .ok_or_else(|| "Caps Lock isn't the Snappy key".to_owned())?;
    // A connection KWin dropped, as when it started over, is opened again once.
    let mut failure = String::new();
    for _ in 0..2 {
        let keyboard = match hold.keyboard.take() {
            Some(keyboard) => keyboard,
            None => {
                FakeKeyboard::open().map_err(|error| format!("can't reach KWin's fake input: {error}"))?
            }
        };
        let keyboard = hold.keyboard.insert(keyboard);
        match keyboard.press_caps_lock() {
            Ok(()) => return Ok(()),
            Err(error) => {
                hold.keyboard = None;
                failure = format!("KWin didn't take the Caps Lock press: {error}");
            }
        }
    }
    Err(failure)
}
