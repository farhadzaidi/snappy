//! Connected displays from RandR 1.5 monitors, with work areas derived from panel struts
//! because `_NET_WORKAREA` is one rectangle for all monitors.

pub mod rules;

use snappy_core::geometry::{Rect, Size};
use snappy_platform::Display;
use x11rb::connection::RequestConnection;
use x11rb::protocol::randr::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _};
use x11rb::rust_connection::RustConnection;

use crate::x::{self, Atoms, ScreenInfo};
use rules::{Strut, edid_name, struts, work_area};
use snappy_linux::display_id::display_id;

/// EDID blocks are 128 bytes each; four cover every extension in practice.
const EDID_LONGS: u32 = 128;

struct Monitor {
    name: String,
    primary: bool,
    frame: Rect,
    edid: Option<Vec<u8>>,
}

/// Every display, primary first.
pub fn read(conn: &RustConnection, screen: &ScreenInfo, atoms: &Atoms) -> Vec<Display> {
    let size = screen_size(conn, screen);
    let struts = panel_struts(conn, screen, atoms, size);
    let scale = x::bytes(conn, screen.root, atoms.RESOURCE_MANAGER, AtomEnum::STRING)
        .map_or(1.0, |resources| {
            rules::scale(&String::from_utf8_lossy(&resources))
        });

    let mut monitors = monitors(conn, screen, atoms);
    if monitors.is_empty() {
        monitors.push(Monitor {
            name: format!("screen-{}", screen.number),
            primary: true,
            frame: Rect::new(0.0, 0.0, size.width, size.height),
            edid: None,
        });
    }
    monitors.sort_by_key(|monitor| !monitor.primary);
    monitors
        .into_iter()
        .map(|monitor| Display {
            id: display_id(&monitor.name, monitor.edid.as_deref()),
            name: monitor
                .edid
                .as_deref()
                .and_then(edid_name)
                .unwrap_or_else(|| monitor.name.clone()),
            frame: monitor.frame,
            work_area: work_area(monitor.frame, size, &struts),
            scale,
        })
        .collect()
}

fn screen_size(conn: &RustConnection, screen: &ScreenInfo) -> Size {
    conn.get_geometry(screen.root)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .map_or(Size::default(), |geometry| {
            Size::new(f64::from(geometry.width), f64::from(geometry.height))
        })
}

fn monitors(conn: &RustConnection, screen: &ScreenInfo, atoms: &Atoms) -> Vec<Monitor> {
    if !matches!(conn.extension_information(randr::X11_EXTENSION_NAME), Ok(Some(_))) {
        return Vec::new();
    }
    let Some(reply) = conn
        .randr_get_monitors(screen.root, true)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
    else {
        return Vec::new();
    };
    reply
        .monitors
        .iter()
        .filter(|monitor| monitor.width > 0 && monitor.height > 0)
        .map(|monitor| Monitor {
            name: atom_name(conn, monitor.name).unwrap_or_else(|| format!("monitor-{}", monitor.name)),
            primary: monitor.primary,
            frame: Rect::new(
                f64::from(monitor.x),
                f64::from(monitor.y),
                f64::from(monitor.width),
                f64::from(monitor.height),
            ),
            edid: monitor
                .outputs
                .first()
                .and_then(|output| edid(conn, *output, atoms)),
        })
        .collect()
}

fn atom_name(conn: &RustConnection, atom: u32) -> Option<String> {
    let reply = conn.get_atom_name(atom).ok()?.reply().ok()?;
    Some(String::from_utf8_lossy(&reply.name).into_owned())
}

fn edid(conn: &RustConnection, output: randr::Output, atoms: &Atoms) -> Option<Vec<u8>> {
    let reply = conn
        .randr_get_output_property(output, atoms.EDID, AtomEnum::ANY, 0, EDID_LONGS, false, false)
        .ok()?
        .reply()
        .ok()?;
    (!reply.data.is_empty()).then_some(reply.data)
}

/// Struts of every managed client; docks and panels are the ones that set them.
fn panel_struts(conn: &RustConnection, screen: &ScreenInfo, atoms: &Atoms, size: Size) -> Vec<Strut> {
    x::window_list(conn, screen.root, atoms._NET_CLIENT_LIST)
        .into_iter()
        .flat_map(|window| {
            let partial = x::property32(conn, window, atoms._NET_WM_STRUT_PARTIAL, AtomEnum::CARDINAL);
            let legacy = x::property32(conn, window, atoms._NET_WM_STRUT, AtomEnum::CARDINAL);
            struts(partial.as_deref(), legacy.as_deref(), size)
        })
        .collect()
}
