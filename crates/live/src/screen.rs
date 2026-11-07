//! What the window server shows: owner, stacking layer, and bounds of on-screen windows.
//! Layers follow macOS: 0 for normal windows, higher for floating ones.

use snappy_core::geometry::Rect;

/// Normal application windows.
pub const NORMAL_LAYER: i64 = 0;
/// Floating windows, such as overlays, above every normal window.
pub const FLOATING_LAYER: i64 = 3;
/// The menu bar's status items.
pub const STATUS_BAR_LAYER: i64 = 25;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenWindow {
    pub pid: i32,
    pub layer: i64,
    pub frame: Rect,
}

/// On-screen windows, front to back.
#[cfg(target_os = "macos")]
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

#[cfg(target_os = "linux")]
pub use x11::on_screen_windows;

/// The names on the D-Bus session bus, where X11 desktops find status items.
#[cfg(target_os = "linux")]
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

/// Windows has two layers: topmost windows count as floating, the rest as normal.
#[cfg(windows)]
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

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub fn on_screen_windows() -> Result<Vec<ScreenWindow>, String> {
    Err("reading the window list isn't implemented on this platform yet".to_owned())
}

/// Saves the whole screen as a PNG at `path`.
#[cfg(target_os = "macos")]
pub fn screenshot(path: &std::path::Path) {
    let _ = std::process::Command::new("screencapture")
        .arg("-x")
        .arg(path)
        .status();
}

/// Saves the whole X screen, every monitor included, as a PNG at `path`.
#[cfg(target_os = "linux")]
pub fn screenshot(path: &std::path::Path) {
    let _ = std::process::Command::new("sh")
        .arg("-c")
        .arg(r#"xwd -root -silent | xwdtopnm 2>/dev/null | pnmtopng >"$1""#)
        .arg("screenshot")
        .arg(path)
        .status();
}

/// Saves the whole virtual screen as a PNG at `path`, through .NET's screen copy.
#[cfg(windows)]
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

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub fn screenshot(_path: &std::path::Path) {}

/// X11 has no layers. A managed client counts as normal, and an override-redirect
/// window counts as floating while it is stacked above every managed client.
#[cfg(target_os = "linux")]
mod x11 {
    use std::collections::HashMap;

    use snappy_core::geometry::Rect;
    use x11rb::connection::Connection;
    use x11rb::protocol::res::{self, ConnectionExt as _};
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, MapState, Window};
    use x11rb::rust_connection::RustConnection;

    use super::{FLOATING_LAYER, NORMAL_LAYER, ScreenWindow};

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

    /// Mapped managed clients and override-redirect windows, front to back. A managed
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
}
