//! Caps Lock as the Snappy key through XKB, the keyboard description every Linux desktop
//! builds its keymap from.
//!
//! On Wayland the compositor builds the keymap from XKB rules with libxkbcommon, which looks
//! in the user's XKB folder (`$XDG_CONFIG_HOME/xkb`) before the system's. Snappy adds an
//! option of its own there, `snappy:caps`: its symbols make Caps Lock set the Snappy key's
//! modifiers while held, without repeating, and keep a real Caps Lock on Shift + Caps Lock,
//! which is also the key Snappy presses to turn Caps Lock on or off after a tap. The rules
//! file that names the option includes the system's rules, so every other option and layout
//! works as before. GNOME Shell and KWin each add the option to the keymap themselves.
//!
//! Every adapter, X11's included, refuses when the user's own options already remap Caps
//! Lock, in the words of `options_refusal`. On Wayland the keymap the compositor built is
//! read too (`layout_refusal`), so a layout that makes Caps Lock another key is refused as
//! well; X11 reads the key's definition from the server itself.

mod keymap;
mod option;
mod refusal;

pub use crate::messages::LAYOUT_REMAPS_CAPS_LOCK;
pub use keymap::{CapsLockKey, caps_lock_key};
pub use option::{
    CapsLockSymbol, OPTION, install, is_installed, rules_with_option, rules_without_option, symbols,
    uninstall, user_folder,
};
pub use refusal::{layout_refusal, options_refusal};
