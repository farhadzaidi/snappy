//! Synthetic pointer and keyboard input, delivered the way real hardware input is.

use snappy_core::geometry::Point;
use snappy_core::keys::{Combo, KeyCode, Modifier, Mods};

pub trait Input {
    /// Press the modifier keys in `mods`, or release them with `down` false.
    fn modifiers(&mut self, mods: Mods, down: bool) -> Result<(), String>;
    fn pointer_down(&mut self, at: Point, mods: Mods) -> Result<(), String>;
    fn pointer_drag(&mut self, to: Point, mods: Mods) -> Result<(), String>;
    fn pointer_up(&mut self, at: Point, mods: Mods) -> Result<(), String>;
    fn key(&mut self, code: &KeyCode, mods: Mods) -> Result<(), String>;

    /// Press and release a key with its modifiers, the Snappy key already expanded.
    fn chord(&mut self, combo: &Combo) -> Result<(), String> {
        self.modifiers(combo.mods, true)?;
        let pressed = self.key(&combo.code, combo.mods);
        self.modifiers(combo.mods, false)?;
        pressed
    }

    /// Drag from `from` to `to` in `steps` moves, holding `mods` throughout.
    fn drag(&mut self, from: Point, to: Point, mods: Mods, steps: u32) -> Result<(), String> {
        self.modifiers(mods, true)?;
        self.pointer_down(from, mods)?;
        pause(30);
        for step in 1..=steps.max(1) {
            let t = f64::from(step) / f64::from(steps.max(1));
            let at = Point::new(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t);
            self.pointer_drag(at, mods)?;
            pause(16);
        }
        self.pointer_up(to, mods)?;
        self.modifiers(mods, false)
    }
}

pub fn pause(milliseconds: u64) {
    std::thread::sleep(std::time::Duration::from_millis(milliseconds));
}

/// Modifiers in the order they're pressed.
pub const MODIFIER_ORDER: [Modifier; 4] = [Modifier::Ctrl, Modifier::Alt, Modifier::Shift, Modifier::Meta];

#[cfg(target_os = "macos")]
pub use mac::MacInput as PlatformInput;

#[cfg(target_os = "macos")]
mod mac {
    use objc2_core_foundation::CGPoint;
    use objc2_core_graphics::{CGEvent, CGEventFlags, CGEventTapLocation, CGEventType, CGMouseButton};
    use snappy_core::geometry::Point;
    use snappy_core::keys::{KeyCode, Modifier, Mods};

    use super::{Input, MODIFIER_ORDER};

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
        let event = event.ok_or("could not create an input event")?;
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
                super::pause(10);
            }
            Ok(())
        }

        fn pointer_down(&mut self, at: Point, mods: Mods) -> Result<(), String> {
            mouse(CGEventType::MouseMoved, at, mods)?;
            super::pause(20);
            mouse(CGEventType::LeftMouseDown, at, mods)
        }

        fn pointer_drag(&mut self, to: Point, mods: Mods) -> Result<(), String> {
            mouse(CGEventType::LeftMouseDragged, to, mods)
        }

        fn pointer_up(&mut self, at: Point, mods: Mods) -> Result<(), String> {
            mouse(CGEventType::LeftMouseUp, at, mods)
        }

        fn key(&mut self, code: &KeyCode, mods: Mods) -> Result<(), String> {
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
            for down in [true, false] {
                let event =
                    CGEvent::new_keyboard_event(None, key, down).ok_or("could not create a key event")?;
                CGEvent::set_flags(Some(&event), flags(mods) | extra);
                CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
                super::pause(20);
            }
            Ok(())
        }
    }
}

#[cfg(windows)]
pub use win32::Win32Input as PlatformInput;

#[cfg(windows)]
mod win32 {
    use snappy_core::geometry::Point;
    use snappy_core::keys::{KeyCode, Modifier, Mods};
    use snappy_windows::reference;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
        KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MOUSE_EVENT_FLAGS, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN,
        MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT, SendInput, VIRTUAL_KEY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    };

    use super::{Input, MODIFIER_ORDER};

    /// Injects input with `SendInput`, which the system handles like device input: move
    /// loops, hotkeys, and low-level hooks all see it. Keys go by scan code, so the
    /// keyboard layout turns them into virtual keys as it would a real keyboard's.
    pub struct Win32Input;

    impl Win32Input {
        pub fn new() -> Result<Self, String> {
            reference::set_dpi_aware();
            Ok(Win32Input)
        }
    }

    fn send(input: INPUT) -> Result<(), String> {
        // SAFETY: one fully initialized input record.
        let sent = unsafe { SendInput(&[input], size_of::<INPUT>() as i32) };
        if sent == 1 {
            Ok(())
        } else {
            Err("the system blocked synthetic input".to_owned())
        }
    }

    /// Absolute pointer coordinates span 0 to 65535 across the virtual screen.
    fn mouse(at: Point, flags: MOUSE_EVENT_FLAGS) -> Result<(), String> {
        // SAFETY: plain queries.
        let (left, top, width, height) = unsafe {
            (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
            )
        };
        let normalize = |value: f64, start: i32, extent: i32| {
            ((value - f64::from(start)) * 65535.0 / f64::from((extent - 1).max(1))).round() as i32
        };
        send(INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: normalize(at.x, left, width),
                    dy: normalize(at.y, top, height),
                    dwFlags: flags | MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                    ..Default::default()
                },
            },
        })
    }

    fn keyboard(vk: u16, scan: u16, flags: KEYBD_EVENT_FLAGS) -> Result<(), String> {
        send(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    wScan: scan,
                    dwFlags: flags,
                    ..Default::default()
                },
            },
        })
    }

    impl Input for Win32Input {
        fn modifiers(&mut self, mods: Mods, down: bool) -> Result<(), String> {
            let mut pressed: Vec<Modifier> =
                MODIFIER_ORDER.into_iter().filter(|m| mods.contains(*m)).collect();
            if !down {
                pressed.reverse();
            }
            for modifier in pressed {
                let mut flags = if down {
                    KEYBD_EVENT_FLAGS(0)
                } else {
                    KEYEVENTF_KEYUP
                };
                // The Windows keys are extended keys.
                if modifier == Modifier::Meta {
                    flags |= KEYEVENTF_EXTENDEDKEY;
                }
                keyboard(reference::modifier_key(modifier), 0, flags)?;
                super::pause(10);
            }
            Ok(())
        }

        fn pointer_down(&mut self, at: Point, _mods: Mods) -> Result<(), String> {
            mouse(at, MOUSE_EVENT_FLAGS(0))?;
            super::pause(20);
            mouse(at, MOUSEEVENTF_LEFTDOWN)
        }

        fn pointer_drag(&mut self, to: Point, _mods: Mods) -> Result<(), String> {
            mouse(to, MOUSE_EVENT_FLAGS(0))
        }

        fn pointer_up(&mut self, at: Point, _mods: Mods) -> Result<(), String> {
            mouse(at, MOUSE_EVENT_FLAGS(0))?;
            mouse(at, MOUSEEVENTF_LEFTUP)
        }

        fn key(&mut self, code: &KeyCode, _mods: Mods) -> Result<(), String> {
            let (scan, extended) =
                reference::scan_code(code).ok_or_else(|| format!("no scan code for {}", code.as_str()))?;
            let mut flags = KEYEVENTF_SCANCODE;
            if extended {
                flags |= KEYEVENTF_EXTENDEDKEY;
            }
            for up in [KEYBD_EVENT_FLAGS(0), KEYEVENTF_KEYUP] {
                keyboard(0, scan, flags | up)?;
                super::pause(20);
            }
            Ok(())
        }
    }
}

#[cfg(target_os = "linux")]
pub use x11::X11Input as PlatformInput;

#[cfg(target_os = "linux")]
mod x11 {
    use snappy_core::geometry::Point;
    use snappy_core::keys::{KeyCode, Modifier, Mods};
    use snappy_x11::reference::Keys;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{
        BUTTON_PRESS_EVENT, BUTTON_RELEASE_EVENT, KEY_PRESS_EVENT, KEY_RELEASE_EVENT, MOTION_NOTIFY_EVENT,
        Window,
    };
    use x11rb::protocol::xtest::ConnectionExt as _;
    use x11rb::rust_connection::RustConnection;
    use x11rb::wrapper::ConnectionExt as _;

    use super::{Input, MODIFIER_ORDER};

    const PRIMARY_BUTTON: u8 = 1;

    /// Fakes device input through XTest, which the server handles like hardware input:
    /// the window manager's moves, grabs, and XInput raw events all see it.
    pub struct X11Input {
        conn: RustConnection,
        root: Window,
        keys: Keys,
    }

    impl X11Input {
        /// Connects to the display named by `DISPLAY`.
        pub fn new() -> Result<Self, String> {
            let (conn, screen) = x11rb::connect(None).map_err(|error| error.to_string())?;
            let root = conn
                .setup()
                .roots
                .get(screen)
                .map(|screen| screen.root)
                .ok_or("the X server has no such screen")?;
            let keys = Keys::read(&conn);
            Ok(Self { conn, root, keys })
        }

        /// Sends one fake event and waits until the server has processed it.
        fn fake(&self, kind: u8, detail: u8, at: Option<Point>) -> Result<(), String> {
            let (x, y) = at.map_or((0, 0), |at| (at.x.round() as i16, at.y.round() as i16));
            self.conn
                .xtest_fake_input(kind, detail, x11rb::CURRENT_TIME, self.root, x, y, 0)
                .map_err(|error| error.to_string())?;
            self.conn.sync().map_err(|error| error.to_string())
        }

        fn modifier_key(&self, modifier: Modifier) -> Result<u8, String> {
            self.keys
                .modifier_keycode(modifier)
                .ok_or_else(|| format!("no key for {modifier:?}"))
        }
    }

    impl Input for X11Input {
        fn modifiers(&mut self, mods: Mods, down: bool) -> Result<(), String> {
            let mut pressed: Vec<Modifier> =
                MODIFIER_ORDER.into_iter().filter(|m| mods.contains(*m)).collect();
            if !down {
                pressed.reverse();
            }
            let kind = if down { KEY_PRESS_EVENT } else { KEY_RELEASE_EVENT };
            for modifier in pressed {
                self.fake(kind, self.modifier_key(modifier)?, None)?;
                super::pause(10);
            }
            Ok(())
        }

        fn pointer_down(&mut self, at: Point, _mods: Mods) -> Result<(), String> {
            self.fake(MOTION_NOTIFY_EVENT, 0, Some(at))?;
            super::pause(20);
            self.fake(BUTTON_PRESS_EVENT, PRIMARY_BUTTON, None)
        }

        fn pointer_drag(&mut self, to: Point, _mods: Mods) -> Result<(), String> {
            self.fake(MOTION_NOTIFY_EVENT, 0, Some(to))
        }

        fn pointer_up(&mut self, at: Point, _mods: Mods) -> Result<(), String> {
            self.fake(MOTION_NOTIFY_EVENT, 0, Some(at))?;
            self.fake(BUTTON_RELEASE_EVENT, PRIMARY_BUTTON, None)
        }

        fn key(&mut self, code: &KeyCode, _mods: Mods) -> Result<(), String> {
            let key = self
                .keys
                .keycode(code)
                .ok_or_else(|| format!("no key code for {}", code.as_str()))?;
            for kind in [KEY_PRESS_EVENT, KEY_RELEASE_EVENT] {
                self.fake(kind, key, None)?;
                super::pause(20);
            }
            Ok(())
        }
    }
}
