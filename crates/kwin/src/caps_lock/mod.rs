//! Caps Lock as the Snappy key on KDE Plasma.
//!
//! - **The key.** Snappy's XKB option (`snappy_linux::xkb`) makes Caps Lock, held, set the
//!   Snappy key's modifiers, and reports one of them as its key, so letting go of it alone
//!   looks like letting go of those modifiers alone. KWin builds its keymap from `kxkbrc`,
//!   so Snappy adds the option there and tells KWin, which reloads the keymap at once. KWin
//!   finds the option only in an XKB folder that existed when it started, so Snappy makes
//!   the folder whenever it starts, and a hold checks the new keymap: without the option in
//!   it, Caps Lock waits for the next login, with the setting on and a notice saying so.
//! - **Presses.** While held as the Snappy key, KWin's keystate reports when Caps Lock goes
//!   down (`presses`), and a modifier-only shortcut of KDE's shortcut service, bound to the
//!   Snappy key's modifiers, fires when they come up with nothing else used meanwhile. That
//!   is a release of Caps Lock alone; the engine decides whether it was a tap.
//! - **Taps.** Snappy presses the keymap's real Caps Lock, Shift + Caps Lock, through
//!   `fake_input`.
//! - **The lock screen** gets Caps Lock as usual: the option comes out while the session is
//!   locked and goes back in after. When System Settings rewrites the keyboard options
//!   without Snappy's, or they changed while the session was locked, Snappy lets go and
//!   reports Caps Lock lost; the engine decides whether to hold it again.

mod fake_input;
mod hold;
mod kxkbrc;
mod presses;

pub use fake_input::{entry_path as applications_entry_path, remove_entry as remove_applications_entry};
pub use hold::{
    Hold, hold, modifiers_reported, prepare_xkb_folder, released_alone, remove_option, restore,
    screen_locked, settings_changed, toggle, undo,
};
pub use kxkbrc::{CHANGED_GROUP, CHANGED_KEYS};
