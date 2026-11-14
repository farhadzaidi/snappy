//! User-facing messages for refusals and errors, worded the same on every platform where the
//! situation is the same.

use snappy_core::error::WindowError;

pub const FULL_SCREEN: &str = "Leave full screen before tiling this window.";
pub const NOT_STANDARD: &str = "Choose a standard application window.";
pub const NOT_MOVABLE: &str = "This window does not allow moving and resizing.";
pub const NO_FOCUSED_WINDOW: &str = "No movable window is focused.";
pub const WINDOW_GONE: &str = "The window is no longer available.";
pub const KWIN_UNREACHABLE: &str = "Snappy can't reach KWin right now. Try again in a moment.";
pub const ON_ALL_DESKTOPS: &str = "This window shows on every desktop, so Snappy can't hide it.";
pub const HIDE_REFUSED: &str = "KWin didn't let Snappy hide this window.";
pub const UNEXPECTED_REPLY: &str = "Snappy and its KWin script don't agree. Quit and reopen Snappy.";

pub const KWIN_NOT_RUNNING: &str = "Snappy can't find KWin. Snappy works with KDE Plasma's window manager.";
pub const SCRIPT_NOT_LOADED: &str = "KWin didn't start Snappy's script. Quit and reopen Snappy.";
pub const SCRIPT_STARTING: &str = "Snappy's KWin script is starting.";
pub const SCRIPT_OTHER_VERSION: &str =
    "A different version of Snappy's KWin script is running. Quit and reopen Snappy.";

pub const CAPS_LOCK_NOT_SET_UP: &str =
    "Snappy couldn't change KDE's keyboard settings to make Caps Lock the Snappy key.";

/// Caps Lock is remapped by the keyboard options `options` already.
pub fn caps_lock_remapped(options: &[&str]) -> String {
    let named = match options {
        [only] => format!("the keyboard option {only}"),
        [first @ .., last] => format!("the keyboard options {} and {last}", first.join(", ")),
        [] => "a keyboard option".to_owned(),
    };
    format!("Caps Lock is already remapped by {named}. Remove that to use Caps Lock as the Snappy key.")
}

pub fn unavailable(message: &str) -> WindowError {
    WindowError::unavailable(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_caps_lock_refusal_names_each_option_that_remaps_it() {
        assert_eq!(
            caps_lock_remapped(&["caps:escape"]),
            "Caps Lock is already remapped by the keyboard option caps:escape. Remove that to use Caps Lock as the Snappy key."
        );
        assert_eq!(
            caps_lock_remapped(&["caps:escape", "ctrl:nocaps"]),
            "Caps Lock is already remapped by the keyboard options caps:escape and ctrl:nocaps. Remove that to use Caps Lock as the Snappy key."
        );
    }
}
