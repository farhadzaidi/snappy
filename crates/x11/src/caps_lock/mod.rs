//! Caps Lock as the Snappy key on X11.
//!
//! Holding works through the server's own keyboard description (XKB): the Caps Lock key's
//! definition is replaced, for this session only, by one whose action sets the Snappy key's
//! modifiers while held, without repeating and without locking, so Snappy's grabs and every
//! other client see the modifiers themselves. Its symbol becomes `VoidSymbol`, which no
//! client acts on. The definition the key had is kept to put back exactly.
//!
//! Presses are learned without watching the keyboard at idle: a passive grab on the Caps
//! Lock key alone reports its press, and the keyboard grab that press starts is let go at
//! once. Only while Caps Lock is down are raw key and button presses selected, to notice
//! that something else was used and when Caps Lock comes up; which key it was is never
//! kept. A tap turns Caps Lock on or off through XKB's lock state, which its light follows.

mod holding;
mod key_definition;
mod presses;
mod refusal;
mod state;
mod toggle;

pub use holding::{hold, keymap_changed, restore, undo};
pub use presses::{key_pressed, key_released, raw_pressed};
pub use state::CapsLockState;
pub use toggle::toggle;
