//! Following Caps Lock's presses while it is held, and whether something else was used
//! during one, without keeping which key it was.

use snappy_platform::CapsLockInput;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::ConnectionExt as _;

use super::state::CapsLockState;
use crate::input;
use crate::shared::{Shared, lock};

/// A key press the event thread received. Caps Lock's own, through its grab, starts a
/// press: raw key and button presses are selected before the keyboard grab the press
/// started is let go, so its release is caught either way. Other presses arrive here only
/// while that grab lasts.
pub fn key_pressed(shared: &Shared, keycode: u8) -> Option<CapsLockInput> {
    let mut state = lock(&shared.caps_lock);
    let caps_lock = state.held.as_ref()?.keycode;
    if keycode != caps_lock {
        return other_input(&mut state);
    }
    if std::mem::replace(&mut state.down, true) {
        return None;
    }
    state.other_input = false;
    drop(state);
    // Without XInput 2 the grab stays until Caps Lock comes up, which ends it.
    if shared.xinput {
        input::set(shared, |monitoring| monitoring.caps_lock_down = true);
        if let Ok(cookie) = shared.conn.ungrab_keyboard(x11rb::CURRENT_TIME) {
            cookie.ignore_error();
        }
        let _ = shared.conn.flush();
    }
    Some(CapsLockInput::Pressed)
}

/// A key release, core or raw: Caps Lock's own ends the press.
pub fn key_released(shared: &Shared, keycode: u8) -> Option<CapsLockInput> {
    let mut state = lock(&shared.caps_lock);
    if state.held.as_ref()?.keycode != keycode || !std::mem::take(&mut state.down) {
        return None;
    }
    drop(state);
    input::set(shared, |monitoring| monitoring.caps_lock_down = false);
    Some(CapsLockInput::Released)
}

/// A raw key press while Caps Lock is down, or a raw button press.
pub fn raw_pressed(shared: &Shared, keycode: Option<u8>) -> Option<CapsLockInput> {
    let mut state = lock(&shared.caps_lock);
    let caps_lock = state.held.as_ref()?.keycode;
    if keycode == Some(caps_lock) {
        return None;
    }
    other_input(&mut state)
}

/// Something else was used while Caps Lock is down; the engine hears it once per press.
fn other_input(state: &mut CapsLockState) -> Option<CapsLockInput> {
    (state.down && !std::mem::replace(&mut state.other_input, true)).then_some(CapsLockInput::OtherInput)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn something_else_used_is_reported_once_per_press_and_only_while_down() {
        let mut state = CapsLockState::default();
        assert_eq!(other_input(&mut state), None);
        state.down = true;
        assert_eq!(other_input(&mut state), Some(CapsLockInput::OtherInput));
        assert_eq!(other_input(&mut state), None);
    }
}
