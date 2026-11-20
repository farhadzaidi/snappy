//! Windows: input through `SendInput`, the window list, and screenshots.

use snappy_core::geometry::Point;
use snappy_core::keys::{KeyCode, Modifier, Mods};
use snappy_windows::reference;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MOUSE_EVENT_FLAGS, MOUSEEVENTF_ABSOLUTE,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT,
    SendInput, VIRTUAL_KEY, VK_CAPITAL,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

use crate::input::{Input, MODIFIER_GAP_MS, MODIFIER_ORDER, POINTER_REST_MS, pause};
use crate::screen::{FLOATING_LAYER, NORMAL_LAYER, ScreenWindow};

/// Caps Lock's scan code.
const CAPS_LOCK_SCAN: u16 = 0x3a;

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
        let mut pressed: Vec<Modifier> = MODIFIER_ORDER.into_iter().filter(|m| mods.contains(*m)).collect();
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
            pause(MODIFIER_GAP_MS);
        }
        Ok(())
    }

    fn pointer_down(&mut self, at: Point, _mods: Mods) -> Result<(), String> {
        mouse(at, MOUSE_EVENT_FLAGS(0))?;
        pause(POINTER_REST_MS);
        mouse(at, MOUSEEVENTF_LEFTDOWN)
    }

    fn pointer_drag(&mut self, to: Point, _mods: Mods) -> Result<(), String> {
        mouse(to, MOUSE_EVENT_FLAGS(0))
    }

    fn pointer_up(&mut self, at: Point, _mods: Mods) -> Result<(), String> {
        mouse(at, MOUSE_EVENT_FLAGS(0))?;
        mouse(at, MOUSEEVENTF_LEFTUP)
    }

    fn send_key(&mut self, code: &KeyCode, _mods: Mods, down: bool) -> Result<(), String> {
        let (scan, extended) =
            reference::scan_code(code).ok_or_else(|| format!("no scan code for {}", code.as_str()))?;
        let mut flags = KEYEVENTF_SCANCODE;
        if extended {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        if !down {
            flags |= KEYEVENTF_KEYUP;
        }
        keyboard(0, scan, flags)
    }

    fn caps_lock(&mut self, down: bool) -> Result<(), String> {
        let up = if down {
            KEYBD_EVENT_FLAGS(0)
        } else {
            KEYEVENTF_KEYUP
        };
        keyboard(0, CAPS_LOCK_SCAN, KEYEVENTF_SCANCODE | up)
    }

    fn caps_lock_on(&mut self) -> Result<bool, String> {
        // SAFETY: a plain query; the low bit is the key's toggle state.
        Ok(unsafe { GetKeyState(i32::from(VK_CAPITAL.0)) } & 1 == 1)
    }
}

/// Windows has two layers: topmost windows count as floating, the rest as normal.
pub fn on_screen_windows() -> Result<Vec<ScreenWindow>, String> {
    Ok(snappy_windows::reference::on_screen_windows()
        .into_iter()
        .filter_map(|window| {
            Some(ScreenWindow {
                pid: i32::try_from(window.pid).ok()?,
                layer: if window.topmost {
                    FLOATING_LAYER
                } else {
                    NORMAL_LAYER
                },
                frame: window.frame,
            })
        })
        .collect())
}

/// Saves the whole virtual screen as a PNG at `path`, through .NET's screen copy.
pub fn screenshot(path: &std::path::Path) {
    let script = "Add-Type -AssemblyName System.Windows.Forms, System.Drawing; \
        $area = [System.Windows.Forms.SystemInformation]::VirtualScreen; \
        $image = New-Object System.Drawing.Bitmap $area.Width, $area.Height; \
        $graphics = [System.Drawing.Graphics]::FromImage($image); \
        $graphics.CopyFromScreen($area.Location, [System.Drawing.Point]::Empty, $area.Size); \
        $image.Save($args[0], [System.Drawing.Imaging.ImageFormat]::Png)";
    let path = path.display().to_string().replace('\'', "''");
    let _ = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command"])
        .arg(format!("& {{ {script} }} '{path}'"))
        .status();
}
