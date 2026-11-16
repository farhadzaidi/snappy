//! X11: input faked through XTest, what the X server shows, screenshots, and the session bus
//! names where status items show.

use std::collections::HashMap;

use snappy_core::geometry::{Point, Rect};
use snappy_core::keys::{KeyCode, Modifier, Mods};
use snappy_linux::key_codes::CAPS_LOCK;
use snappy_x11::reference::Keys;
use x11rb::connection::Connection;
use x11rb::protocol::res::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{
    AtomEnum, BUTTON_PRESS_EVENT, BUTTON_RELEASE_EVENT, ConnectionExt as _, KEY_PRESS_EVENT,
    KEY_RELEASE_EVENT, KeyButMask, MOTION_NOTIFY_EVENT, MapState, Window,
};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use crate::input::{Input, KEY_GAP_MS, MODIFIER_GAP_MS, MODIFIER_ORDER, POINTER_REST_MS, pause};
use crate::screen::{FLOATING_LAYER, NORMAL_LAYER, ScreenWindow};

/// The primary button, as XTest numbers it.
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
        let mut pressed: Vec<Modifier> = MODIFIER_ORDER.into_iter().filter(|m| mods.contains(*m)).collect();
        if !down {
            pressed.reverse();
        }
        let kind = if down { KEY_PRESS_EVENT } else { KEY_RELEASE_EVENT };
        for modifier in pressed {
            self.fake(kind, self.modifier_key(modifier)?, None)?;
            pause(MODIFIER_GAP_MS);
        }
        Ok(())
    }

    fn pointer_down(&mut self, at: Point, _mods: Mods) -> Result<(), String> {
        self.fake(MOTION_NOTIFY_EVENT, 0, Some(at))?;
        pause(POINTER_REST_MS);
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
            pause(KEY_GAP_MS);
        }
        Ok(())
    }

    fn caps_lock(&mut self, down: bool) -> Result<(), String> {
        let kind = if down { KEY_PRESS_EVENT } else { KEY_RELEASE_EVENT };
        self.fake(kind, CAPS_LOCK, None)
    }

    /// The core protocol's state includes locked modifiers, Lock among them.
    fn caps_lock_on(&mut self) -> Result<bool, String> {
        let pointer = self
            .conn
            .query_pointer(self.root)
            .map_err(|error| error.to_string())?
            .reply()
            .map_err(|error| error.to_string())?;
        Ok(pointer.mask.contains(KeyButMask::LOCK))
    }
}

/// Saves the whole screen, every monitor included, as a PNG at `path`.
pub fn screenshot(path: &std::path::Path) {
    let _ = std::process::Command::new("sh")
        .arg("-c")
        .arg(r#"xwd -root -silent | xwdtopnm 2>/dev/null | pnmtopng >"$1""#)
        .arg("screenshot")
        .arg(path)
        .status();
}

/// Below the managed windows: drawn, but not floating.
const BELOW_LAYER: i64 = -1;

fn failed(error: impl std::fmt::Display) -> String {
    format!("the window list can't be read: {error}")
}

fn atom(conn: &RustConnection, name: &str) -> Result<u32, String> {
    Ok(conn
        .intern_atom(false, name.as_bytes())
        .map_err(failed)?
        .reply()
        .map_err(failed)?
        .atom)
}

fn cardinals(conn: &RustConnection, window: Window, property: u32, kind: AtomEnum) -> Vec<u32> {
    conn.get_property(false, window, property, kind, 0, u32::from(u16::MAX))
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .and_then(|reply| reply.value32().map(Iterator::collect))
        .unwrap_or_default()
}

/// The child of the root that holds `window`: the window manager's frame around it.
fn top_level(conn: &RustConnection, root: Window, mut window: Window) -> Option<Window> {
    loop {
        let parent = conn.query_tree(window).ok()?.reply().ok()?.parent;
        if parent == root || parent == x11rb::NONE {
            return Some(window);
        }
        window = parent;
    }
}

/// The process of the client that created `window`, from the X-Resource extension.
fn creator_pid(conn: &RustConnection, window: Window) -> Option<i32> {
    let spec = res::ClientIdSpec {
        client: window,
        mask: res::ClientIdMask::LOCAL_CLIENT_PID,
    };
    let reply = conn.res_query_client_ids(&[spec]).ok()?.reply().ok()?;
    let pid = *reply.ids.first()?.value.first()?;
    i32::try_from(pid).ok()
}

fn bounds(conn: &RustConnection, window: Window) -> Option<Rect> {
    let geometry = conn.get_geometry(window).ok()?.reply().ok()?;
    Some(Rect::new(
        f64::from(geometry.x),
        f64::from(geometry.y),
        f64::from(geometry.width) + 2.0 * f64::from(geometry.border_width),
        f64::from(geometry.height) + 2.0 * f64::from(geometry.border_width),
    ))
}

/// Mapped managed clients and override-redirect windows, front to back. X11 has no layers:
/// a managed client counts as normal, and an override-redirect window counts as floating
/// while it is stacked above every managed client. A managed
/// client is reported with its frame's bounds and its `_NET_WM_PID`.
pub fn on_screen_windows() -> Result<Vec<ScreenWindow>, String> {
    let (conn, screen) = x11rb::connect(None).map_err(failed)?;
    let root = conn
        .setup()
        .roots
        .get(screen)
        .map(|screen| screen.root)
        .ok_or_else(|| failed("no such screen"))?;
    let client_list = atom(&conn, "_NET_CLIENT_LIST")?;
    let wm_pid = atom(&conn, "_NET_WM_PID")?;

    let mut client_pids: HashMap<Window, i32> = HashMap::new();
    for client in cardinals(&conn, root, client_list, AtomEnum::WINDOW) {
        let pid = cardinals(&conn, client, wm_pid, AtomEnum::CARDINAL)
            .first()
            .and_then(|pid| i32::try_from(*pid).ok());
        if let (Some(frame), Some(pid)) = (top_level(&conn, root, client), pid) {
            client_pids.insert(frame, pid);
        }
    }

    let mut stack = conn
        .query_tree(root)
        .map_err(failed)?
        .reply()
        .map_err(failed)?
        .children;
    stack.reverse();
    let mut windows = Vec::new();
    let mut below_a_client = false;
    for window in stack {
        let Some(attributes) = conn
            .get_window_attributes(window)
            .ok()
            .and_then(|c| c.reply().ok())
        else {
            continue;
        };
        if attributes.map_state != MapState::VIEWABLE {
            continue;
        }
        let Some(frame) = bounds(&conn, window) else {
            continue;
        };
        if let Some(pid) = client_pids.get(&window) {
            below_a_client = true;
            windows.push(ScreenWindow {
                pid: *pid,
                layer: NORMAL_LAYER,
                frame,
            });
        } else if attributes.override_redirect
            && let Some(pid) = creator_pid(&conn, window)
        {
            windows.push(ScreenWindow {
                pid,
                layer: if below_a_client {
                    BELOW_LAYER
                } else {
                    FLOATING_LAYER
                },
                frame,
            });
        }
    }
    Ok(windows)
}

/// The names on the D-Bus session bus, where X11 desktops find status items.
pub fn session_bus_names() -> Result<Vec<String>, String> {
    let output = std::process::Command::new("dbus-send")
        .args([
            "--session",
            "--print-reply",
            "--dest=org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus.ListNames",
        ])
        .output()
        .map_err(|error| format!("dbus-send didn't run: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "the session bus can't be read: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().strip_prefix("string \"")?.strip_suffix('"'))
        .map(str::to_owned)
        .collect())
}
