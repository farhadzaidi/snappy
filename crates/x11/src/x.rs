//! Shared X plumbing: interned atoms, property reads, and the screen Snappy works on.

use std::sync::{Mutex, MutexGuard, PoisonError};

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{Atom, AtomEnum, ConnectionExt, Screen, VisualClass, Visualid, Window};
use x11rb::rust_connection::RustConnection;

use crate::error::X11Error;

x11rb::atom_manager! {
    pub Atoms: AtomsCookie {
        _NET_ACTIVE_WINDOW,
        _NET_CLIENT_LIST,
        _NET_CLIENT_LIST_STACKING,
        _NET_CURRENT_DESKTOP,
        _NET_DESKTOP_NAMES,
        _NET_FRAME_EXTENTS,
        _NET_MOVERESIZE_WINDOW,
        _NET_NUMBER_OF_DESKTOPS,
        _NET_WM_ACTION_MOVE,
        _NET_WM_ACTION_RESIZE,
        _NET_WM_ALLOWED_ACTIONS,
        _NET_WM_DESKTOP,
        _NET_WM_PID,
        _NET_WM_STATE,
        _NET_WM_STATE_FULLSCREEN,
        _NET_WM_STATE_HIDDEN,
        _NET_WM_STATE_MAXIMIZED_HORZ,
        _NET_WM_STATE_MAXIMIZED_VERT,
        _NET_WM_STRUT,
        _NET_WM_STRUT_PARTIAL,
        _NET_WM_WINDOW_TYPE,
        _NET_WM_WINDOW_TYPE_COMBO,
        _NET_WM_WINDOW_TYPE_DESKTOP,
        _NET_WM_WINDOW_TYPE_DIALOG,
        _NET_WM_WINDOW_TYPE_DND,
        _NET_WM_WINDOW_TYPE_DOCK,
        _NET_WM_WINDOW_TYPE_DROPDOWN_MENU,
        _NET_WM_WINDOW_TYPE_MENU,
        _NET_WM_WINDOW_TYPE_NORMAL,
        _NET_WM_WINDOW_TYPE_NOTIFICATION,
        _NET_WM_WINDOW_TYPE_POPUP_MENU,
        _NET_WM_WINDOW_TYPE_SPLASH,
        _NET_WM_WINDOW_TYPE_TOOLBAR,
        _NET_WM_WINDOW_TYPE_TOOLTIP,
        _NET_WM_WINDOW_TYPE_UTILITY,
        _NET_WORKAREA,
        _GTK_FRAME_EXTENTS,
        EDID,
        RESOURCE_MANAGER,
        _SNAPPY_PARKING_DESKTOP,
        _SNAPPY_STOP,
        UTF8_STRING,
    }
}

/// The screen's root window and what overlays need from it.
#[derive(Debug, Clone, Copy)]
pub struct ScreenInfo {
    pub number: usize,
    pub root: Window,
    /// A 32-bit TrueColor visual for translucent overlays, when the server has one.
    pub argb_visual: Option<Visualid>,
    /// Images go out most significant byte first.
    pub msb_first: bool,
}

impl ScreenInfo {
    pub fn read(conn: &RustConnection, number: usize) -> Result<ScreenInfo, X11Error> {
        let setup = conn.setup();
        let screen: &Screen = setup.roots.get(number).ok_or(X11Error::NoScreen)?;
        Ok(ScreenInfo {
            number,
            root: screen.root,
            argb_visual: argb_visual(screen),
            msb_first: setup.image_byte_order == x11rb::protocol::xproto::ImageOrder::MSB_FIRST,
        })
    }
}

fn argb_visual(screen: &Screen) -> Option<Visualid> {
    screen
        .allowed_depths
        .iter()
        .filter(|depth| depth.depth == 32)
        .flat_map(|depth| &depth.visuals)
        .find(|visual| visual.class == VisualClass::TRUE_COLOR && visual.red_mask == 0x00ff_0000)
        .map(|visual| visual.visual_id)
}

/// A property's 32-bit values of `kind`, or `None` when it's absent or unreadable.
pub fn property32(
    conn: &impl Connection,
    window: Window,
    property: Atom,
    kind: impl Into<Atom>,
) -> Option<Vec<u32>> {
    let reply = conn
        .get_property(false, window, property, kind, 0, u32::from(u16::MAX))
        .ok()?
        .reply()
        .ok()?;
    let values: Vec<u32> = reply.value32()?.collect();
    (reply.type_ != x11rb::NONE).then_some(values)
}

/// The first 32-bit value of a property.
pub fn cardinal(conn: &impl Connection, window: Window, property: Atom) -> Option<u32> {
    property32(conn, window, property, AtomEnum::CARDINAL)?
        .first()
        .copied()
}

/// A window-list property such as `_NET_CLIENT_LIST`; empty when absent.
pub fn window_list(conn: &impl Connection, window: Window, property: Atom) -> Vec<Window> {
    property32(conn, window, property, AtomEnum::WINDOW).unwrap_or_default()
}

/// A string property's bytes.
pub fn bytes(
    conn: &impl Connection,
    window: Window,
    property: Atom,
    kind: impl Into<Atom>,
) -> Option<Vec<u8>> {
    let reply = conn
        .get_property(false, window, property, kind, 0, u32::from(u16::MAX))
        .ok()?
        .reply()
        .ok()?;
    (reply.type_ != x11rb::NONE).then_some(reply.value)
}

/// Locks a mutex, recovering the data if a panicking thread poisoned it.
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
