//! macOS: input posted at the HID level, the window server's list of windows, screenshots,
//! and the main display's modes.

use objc2_core_foundation::{CFArray, CFRetained, CGPoint};
use objc2_core_graphics::{
    CGDisplayCopyAllDisplayModes, CGDisplayCopyDisplayMode, CGDisplayMode, CGDisplaySetDisplayMode, CGError,
    CGEvent, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventTapLocation, CGEventType,
    CGMainDisplayID, CGMouseButton,
};
use snappy_core::geometry::{Point, Size};
use snappy_core::keys::{KeyCode, Modifier, Mods};

use crate::input::{Input, MODIFIER_GAP_MS, MODIFIER_ORDER, POINTER_REST_MS, pause};
use crate::screen::ScreenWindow;

/// Posts events at the HID level, where window drags and global shortcuts see them.
pub struct MacInput;

impl MacInput {
    pub fn new() -> Result<Self, String> {
        Ok(MacInput)
    }
}

fn flags(mods: Mods) -> CGEventFlags {
    let mut flags = CGEventFlags::empty();
    for (modifier, flag) in [
        (Modifier::Shift, CGEventFlags::MaskShift),
        (Modifier::Ctrl, CGEventFlags::MaskControl),
        (Modifier::Alt, CGEventFlags::MaskAlternate),
        (Modifier::Meta, CGEventFlags::MaskCommand),
    ] {
        if mods.contains(modifier) {
            flags |= flag;
        }
    }
    flags
}

/// F18, the key Snappy has macOS make of Caps Lock. Posted events join the stream past
/// the HID remap, so the scenarios press what Caps Lock arrives as.
const CAPS_LOCK_AS_SNAPPY_KEY: u16 = 79;

/// The left-hand key for each modifier.
fn modifier_key(modifier: Modifier) -> u16 {
    match modifier {
        Modifier::Meta => 55,
        Modifier::Shift => 56,
        Modifier::Alt => 58,
        Modifier::Ctrl => 59,
    }
}

fn post(event: Option<objc2_core_foundation::CFRetained<CGEvent>>, mods: Mods) -> Result<(), String> {
    let event = event.ok_or("couldn't create an input event")?;
    CGEvent::set_flags(Some(&event), flags(mods));
    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
    Ok(())
}

fn mouse(kind: CGEventType, at: Point, mods: Mods) -> Result<(), String> {
    let point = CGPoint::new(at.x, at.y);
    post(
        CGEvent::new_mouse_event(None, kind, point, CGMouseButton::Left),
        mods,
    )
}

impl Input for MacInput {
    fn modifiers(&mut self, mods: Mods, down: bool) -> Result<(), String> {
        let pressed: Vec<Modifier> = MODIFIER_ORDER.into_iter().filter(|m| mods.contains(*m)).collect();
        for index in 0..pressed.len() {
            // Press in order and release in reverse; flags show what's still held.
            let (modifier, held) = if down {
                (pressed[index], Mods::from_iter(pressed[..=index].iter().copied()))
            } else {
                let last = pressed.len() - 1 - index;
                (pressed[last], Mods::from_iter(pressed[..last].iter().copied()))
            };
            let event = CGEvent::new_keyboard_event(None, modifier_key(modifier), down);
            if let Some(event) = &event {
                CGEvent::set_type(Some(event), CGEventType::FlagsChanged);
            }
            post(event, held)?;
            pause(MODIFIER_GAP_MS);
        }
        Ok(())
    }

    fn pointer_down(&mut self, at: Point, mods: Mods) -> Result<(), String> {
        mouse(CGEventType::MouseMoved, at, mods)?;
        pause(POINTER_REST_MS);
        mouse(CGEventType::LeftMouseDown, at, mods)
    }

    fn pointer_drag(&mut self, to: Point, mods: Mods) -> Result<(), String> {
        mouse(CGEventType::LeftMouseDragged, to, mods)
    }

    fn pointer_up(&mut self, at: Point, mods: Mods) -> Result<(), String> {
        mouse(CGEventType::LeftMouseUp, at, mods)
    }

    fn send_key(&mut self, code: &KeyCode, mods: Mods, down: bool) -> Result<(), String> {
        let key = snappy_macos::reference::virtual_key_code(code)
            .and_then(|key| u16::try_from(key).ok())
            .ok_or_else(|| format!("no key code for {}", code.as_str()))?;
        // Real arrow keys report as keypad keys with the function flag; shortcuts match
        // on those flags too.
        let extra = if code.as_str().starts_with("Arrow") {
            CGEventFlags::MaskNumericPad | CGEventFlags::MaskSecondaryFn
        } else {
            CGEventFlags::empty()
        };
        let event = CGEvent::new_keyboard_event(None, key, down).ok_or("couldn't create a key event")?;
        CGEvent::set_flags(Some(&event), flags(mods) | extra);
        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
        Ok(())
    }

    /// Function keys report with the function flag, as real arrow keys do; shortcuts match
    /// on it.
    fn caps_lock(&mut self, down: bool) -> Result<(), String> {
        let event = CGEvent::new_keyboard_event(None, CAPS_LOCK_AS_SNAPPY_KEY, down)
            .ok_or("couldn't create a key event")?;
        CGEvent::set_flags(Some(&event), CGEventFlags::MaskSecondaryFn);
        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
        Ok(())
    }

    fn caps_lock_on(&mut self) -> Result<bool, String> {
        let flags = CGEventSource::flags_state(CGEventSourceStateID::HIDSystemState);
        Ok(flags.contains(CGEventFlags::MaskAlphaShift))
    }
}

/// On-screen windows, front to back.
pub fn on_screen_windows() -> Result<Vec<ScreenWindow>, String> {
    let windows = snappy_macos::reference::on_screen_windows().ok_or("the window list can't be read")?;
    Ok(windows
        .into_iter()
        .map(|window| ScreenWindow {
            pid: window.pid,
            layer: window.layer,
            frame: window.frame,
        })
        .collect())
}

/// Saves the whole screen as a PNG at `path`.
pub fn screenshot(path: &std::path::Path) {
    let _ = std::process::Command::new("screencapture")
        .arg("-x")
        .arg(path)
        .status();
}

/// The main display's size, in points.
pub fn display_size() -> Option<Size> {
    CGDisplayCopyDisplayMode(CGMainDisplayID()).map(|mode| size_of(&mode))
}

/// The sizes the main display can show the desktop at.
pub fn usable_display_sizes() -> Vec<Size> {
    usable_modes().iter().map(|mode| size_of(mode)).collect()
}

/// Show the desktop on the main display at `size`.
pub fn set_display_size(size: Size) -> Result<(), String> {
    let mode = usable_modes()
        .into_iter()
        .find(|mode| size_of(mode) == size)
        .ok_or_else(|| format!("the display has no {}×{} mode", size.width, size.height))?;
    // SAFETY: no options are passed.
    let error = unsafe { CGDisplaySetDisplayMode(CGMainDisplayID(), Some(&mode), None) };
    if error == CGError::Success {
        Ok(())
    } else {
        Err(format!(
            "the display refused {}×{}: error {}",
            size.width, size.height, error.0
        ))
    }
}

fn usable_modes() -> Vec<CFRetained<CGDisplayMode>> {
    // SAFETY: no options are passed.
    let Some(modes) = (unsafe { CGDisplayCopyAllDisplayModes(CGMainDisplayID(), None) }) else {
        return Vec::new();
    };
    // SAFETY: the array CoreGraphics returns holds display modes.
    let modes = unsafe { CFRetained::cast_unchecked::<CFArray<CGDisplayMode>>(modes) };
    modes
        .iter()
        .filter(|mode| CGDisplayMode::is_usable_for_desktop_gui(Some(mode)))
        .collect()
}

/// Display modes measure the desktop in points.
fn size_of(mode: &CGDisplayMode) -> Size {
    Size::new(
        CGDisplayMode::width(Some(mode)) as f64,
        CGDisplayMode::height(Some(mode)) as f64,
    )
}
