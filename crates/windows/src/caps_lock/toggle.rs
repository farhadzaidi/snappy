//! Turning Caps Lock on or off for a tap, by injecting a Caps Lock press marked as Snappy's.

use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VK_CAPITAL,
};

/// Marks Snappy's own Caps Lock presses in their extra information, so the hook passes
/// them on.
pub const SNAPPY_CAPS_LOCK: usize = 0x534E_4150;

/// Turn Caps Lock on or off as its key would, for every keyboard and its light.
pub fn toggle() -> bool {
    let press = |flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VK_CAPITAL,
                dwFlags: flags,
                dwExtraInfo: SNAPPY_CAPS_LOCK,
                ..Default::default()
            },
        },
    };
    let inputs = [press(KEYBD_EVENT_FLAGS(0)), press(KEYEVENTF_KEYUP)];
    // SAFETY: two fully initialized input records.
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    sent as usize == inputs.len()
}
