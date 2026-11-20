//! Sending the focused window to the back, on one shortcut: each press sends it behind the
//! windows it covers (`sends`), by raising the windows over it where a platform can't lower
//! it (`raising`), and a press held long enough puts it back as it's let go (`put_back`).
//! `press` times the presses and keeps what lasts between events. The rules, which windows
//! a window covers, which takes focus, which one the next press sends, and when a press is
//! a hold, are core's.

mod press;
mod put_back;
mod raising;
mod sends;

pub(crate) use press::{SendingToBack, focus_changed, pressed, released};
pub(crate) use put_back::{cancel_put_back, cancels_put_back};
