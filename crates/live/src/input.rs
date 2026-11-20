//! Synthetic pointer and keyboard input, delivered the way real hardware input is. Each
//! desktop's own input is in `desktop`; `platform_input` picks the one in use.

use snappy_core::geometry::Point;
use snappy_core::keys::{Combo, KeyCode, Modifier, Mods};

#[cfg(target_os = "macos")]
pub use crate::desktop::macos::MacInput as PlatformInput;
#[cfg(windows)]
pub use crate::desktop::windows::Win32Input as PlatformInput;
#[cfg(target_os = "linux")]
pub use crate::desktop::x11::X11Input;

/// How long between one modifier key and the next, pressing or letting go.
pub const MODIFIER_GAP_MS: u64 = 10;
/// How long a key stays down, and how long after it comes up before anything else.
pub const KEY_GAP_MS: u64 = 20;
/// How long the pointer rests where it moved before the button goes down.
pub const POINTER_REST_MS: u64 = 20;
/// How long a drag holds the button before its first move.
pub const DRAG_HOLD_MS: u64 = 30;
/// How long between a drag's moves: about a frame at 60 Hz.
pub const DRAG_STEP_MS: u64 = 16;

pub trait Input {
    /// Press the modifier keys in `mods`, or release them with `down` false.
    fn modifiers(&mut self, mods: Mods, down: bool) -> Result<(), String>;
    fn pointer_down(&mut self, at: Point, mods: Mods) -> Result<(), String>;
    fn pointer_drag(&mut self, to: Point, mods: Mods) -> Result<(), String>;
    fn pointer_up(&mut self, at: Point, mods: Mods) -> Result<(), String>;
    /// Press a key, or let it go with `down` false, with `mods` held.
    fn send_key(&mut self, code: &KeyCode, mods: Mods, down: bool) -> Result<(), String>;
    /// Press Caps Lock, or let it go with `down` false.
    fn caps_lock(&mut self, down: bool) -> Result<(), String>;
    /// Whether Caps Lock is on.
    fn caps_lock_on(&mut self) -> Result<bool, String>;

    /// Press and release a key with `mods` held.
    fn key(&mut self, code: &KeyCode, mods: Mods) -> Result<(), String> {
        for down in [true, false] {
            self.send_key(code, mods, down)?;
            pause(KEY_GAP_MS);
        }
        Ok(())
    }

    /// Press and release a key with its modifiers, the Snappy key already expanded.
    fn chord(&mut self, combo: &Combo) -> Result<(), String> {
        self.modifiers(combo.mods, true)?;
        let pressed = self.key(&combo.code, combo.mods);
        self.modifiers(combo.mods, false)?;
        pressed
    }

    /// Press a chord and keep it down, as a hand holding a shortcut does, until
    /// `let_go_of_chord`.
    fn hold_chord(&mut self, combo: &Combo) -> Result<(), String> {
        self.modifiers(combo.mods, true)?;
        self.send_key(&combo.code, combo.mods, true)
    }

    /// Let go of a chord `hold_chord` pressed: its key, then its modifiers.
    fn let_go_of_chord(&mut self, combo: &Combo) -> Result<(), String> {
        let released = self.send_key(&combo.code, combo.mods, false);
        pause(KEY_GAP_MS);
        self.modifiers(combo.mods, false)?;
        released
    }

    /// Drag from `from` to `to` in `steps` moves, holding `mods` throughout.
    fn drag(&mut self, from: Point, to: Point, mods: Mods, steps: u32) -> Result<(), String> {
        self.modifiers(mods, true)?;
        self.pointer_down(from, mods)?;
        pause(DRAG_HOLD_MS);
        for step in 1..=steps.max(1) {
            let t = f64::from(step) / f64::from(steps.max(1));
            self.pointer_drag(along(from, to, t), mods)?;
            pause(DRAG_STEP_MS);
        }
        self.pointer_up(to, mods)?;
        self.modifiers(mods, false)
    }
}

pub fn pause(milliseconds: u64) {
    std::thread::sleep(std::time::Duration::from_millis(milliseconds));
}

/// The point `t` of the way from `from` to `to`.
pub fn along(from: Point, to: Point, t: f64) -> Point {
    Point::new(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t)
}

/// Modifiers in the order they're pressed.
pub const MODIFIER_ORDER: [Modifier; 4] = [Modifier::Ctrl, Modifier::Alt, Modifier::Shift, Modifier::Meta];

/// The platform's input: XTest on X11, the desktop shell's virtual devices on Wayland.
#[cfg(target_os = "linux")]
pub fn platform_input() -> Result<Box<dyn Input>, String> {
    match crate::desktop::wayland::Shell::current() {
        Some(shell) => shell.input(),
        None => Ok(Box::new(X11Input::new()?)),
    }
}

#[cfg(not(target_os = "linux"))]
pub fn platform_input() -> Result<Box<dyn Input>, String> {
    Ok(Box::new(PlatformInput::new()?))
}
