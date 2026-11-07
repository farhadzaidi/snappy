//! Reading what Snappy needs to know about clients, with every request for a batch of
//! windows sent before the first reply is awaited.

use std::collections::BTreeSet;

use snappy_core::geometry::Rect;
use snappy_platform::{RecoveryKey, WindowId, WindowInfo};
use x11rb::cookie::Cookie;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ConnectionExt, GetGeometryReply, GetPropertyReply, TranslateCoordinatesReply, Window,
};
use x11rb::rust_connection::RustConnection;

use crate::eligibility::{WindowFacts, WindowType};
use crate::frames::{Extents, FrameGeometry};
use crate::process;
use crate::x::Atoms;

/// `_NET_WM_DESKTOP` for windows shown on every desktop.
const ALL_DESKTOPS: u32 = 0xFFFF_FFFF;

/// One client as Snappy sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowRecord {
    pub window: Window,
    /// The visible frame in root coordinates.
    pub frame: Rect,
    pub geometry: FrameGeometry,
    pub facts: WindowFacts,
    pub desktop: Option<u32>,
    pub pid: Option<u32>,
    /// The `WM_CLASS` class name.
    pub class: Option<String>,
}

impl WindowRecord {
    pub fn id(&self) -> WindowId {
        WindowId(u64::from(self.window))
    }

    /// On the current native desktop, or on all of them.
    pub fn on_desktop(&self, current: Option<u32>) -> bool {
        match (self.desktop, current) {
            (Some(ALL_DESKTOPS), _) | (None, _) | (_, None) => true,
            (Some(desktop), Some(current)) => desktop == current,
        }
    }

    pub fn is_visible(&self, current_desktop: Option<u32>) -> bool {
        !self.facts.minimized && self.on_desktop(current_desktop)
    }

    /// The `WM_CLASS` class, else the executable name, else the process ID.
    pub fn app(&self) -> String {
        self.class
            .clone()
            .or_else(|| self.pid.and_then(process::executable))
            .unwrap_or_else(|| format!("pid-{}", self.pid.unwrap_or(0)))
    }

    pub fn info(&self) -> WindowInfo {
        WindowInfo {
            id: self.id(),
            app: self.app(),
            pid: self.pid.unwrap_or(0),
            frame: self.frame,
            recovery_key: self.recovery_key(),
        }
    }

    /// The process launch plus the X window ID identify the window across a daemon
    /// restart.
    fn recovery_key(&self) -> Option<RecoveryKey> {
        let pid = self.pid?;
        Some(RecoveryKey {
            pid,
            launch_time: process::start_time(pid)?,
            window: self.window.to_string(),
        })
    }
}

/// The properties read for every client, in request order.
const PROPERTIES: usize = 9;

struct Pending<'c> {
    window: Window,
    geometry: Cookie<'c, RustConnection, GetGeometryReply>,
    origin: Cookie<'c, RustConnection, TranslateCoordinatesReply>,
    properties: Vec<Cookie<'c, RustConnection, GetPropertyReply>>,
}

/// Reads `windows`; unreadable ones, usually just destroyed, are `None`.
pub fn read(
    conn: &RustConnection,
    root: Window,
    atoms: &Atoms,
    windows: &[Window],
    client_list: &BTreeSet<Window>,
) -> Vec<Option<WindowRecord>> {
    let names: [(Atom, Atom); PROPERTIES] = [
        (atoms._NET_FRAME_EXTENTS, AtomEnum::CARDINAL.into()),
        (atoms._GTK_FRAME_EXTENTS, AtomEnum::CARDINAL.into()),
        (atoms._NET_WM_WINDOW_TYPE, AtomEnum::ATOM.into()),
        (atoms._NET_WM_STATE, AtomEnum::ATOM.into()),
        (atoms._NET_WM_DESKTOP, AtomEnum::CARDINAL.into()),
        (AtomEnum::WM_TRANSIENT_FOR.into(), AtomEnum::WINDOW.into()),
        (atoms._NET_WM_ALLOWED_ACTIONS, AtomEnum::ATOM.into()),
        (atoms._NET_WM_PID, AtomEnum::CARDINAL.into()),
        (AtomEnum::WM_CLASS.into(), AtomEnum::STRING.into()),
    ];
    let pending: Vec<Option<Pending<'_>>> = windows
        .iter()
        .map(|&window| {
            Some(Pending {
                window,
                geometry: conn.get_geometry(window).ok()?,
                origin: conn.translate_coordinates(window, root, 0, 0).ok()?,
                properties: names
                    .iter()
                    .map(|(name, kind)| conn.get_property(false, window, *name, *kind, 0, 1024).ok())
                    .collect::<Option<Vec<_>>>()?,
            })
        })
        .collect();
    pending
        .into_iter()
        .map(|pending| finish(pending?, atoms, client_list))
        .collect()
}

fn finish(pending: Pending<'_>, atoms: &Atoms, client_list: &BTreeSet<Window>) -> Option<WindowRecord> {
    let geometry = pending.geometry.reply().ok()?;
    let origin = pending.origin.reply().ok()?;
    let replies: Vec<Option<GetPropertyReply>> = pending
        .properties
        .into_iter()
        .map(|cookie| cookie.reply().ok().filter(|reply| reply.type_ != x11rb::NONE))
        .collect();
    let values =
        |index: usize| -> Option<Vec<u32>> { Some(replies.get(index)?.as_ref()?.value32()?.collect()) };

    let frame_geometry = FrameGeometry {
        decorations: Extents::from_cardinals(values(0).as_deref()),
        shadows: Extents::from_cardinals(values(1).as_deref()),
    };
    let client = Rect::new(
        f64::from(origin.dst_x),
        f64::from(origin.dst_y),
        f64::from(geometry.width),
        f64::from(geometry.height),
    );
    let states = values(3).unwrap_or_default();
    let has_state = |atom: Atom| states.contains(&atom);
    let allowed = values(6);
    let facts = WindowFacts {
        in_client_list: client_list.contains(&pending.window),
        types: values(2)
            .unwrap_or_default()
            .iter()
            .map(|atom| window_type(atoms, *atom))
            .collect(),
        full_screen: has_state(atoms._NET_WM_STATE_FULLSCREEN),
        maximized: has_state(atoms._NET_WM_STATE_MAXIMIZED_VERT)
            || has_state(atoms._NET_WM_STATE_MAXIMIZED_HORZ),
        minimized: has_state(atoms._NET_WM_STATE_HIDDEN),
        transient: values(5)
            .and_then(|owners| owners.first().copied())
            .is_some_and(|owner| owner != 0),
        fixed: allowed.is_some_and(|actions| {
            !(actions.contains(&atoms._NET_WM_ACTION_MOVE) && actions.contains(&atoms._NET_WM_ACTION_RESIZE))
        }),
    };
    Some(WindowRecord {
        window: pending.window,
        frame: frame_geometry.visible(client),
        geometry: frame_geometry,
        facts,
        desktop: values(4).and_then(|desktops| desktops.first().copied()),
        pid: values(7)
            .and_then(|pids| pids.first().copied())
            .filter(|pid| *pid > 0),
        class: replies
            .get(8)
            .and_then(Option::as_ref)
            .and_then(|reply| class_name(&reply.value)),
    })
}

/// The class from `WM_CLASS`, which holds the instance and class as two
/// null-terminated strings.
pub fn class_name(value: &[u8]) -> Option<String> {
    let class = value.split(|byte| *byte == 0).nth(1)?;
    let class = String::from_utf8_lossy(class).trim().to_owned();
    (!class.is_empty()).then_some(class)
}

fn window_type(atoms: &Atoms, atom: Atom) -> WindowType {
    let special = [
        atoms._NET_WM_WINDOW_TYPE_COMBO,
        atoms._NET_WM_WINDOW_TYPE_DESKTOP,
        atoms._NET_WM_WINDOW_TYPE_DIALOG,
        atoms._NET_WM_WINDOW_TYPE_DND,
        atoms._NET_WM_WINDOW_TYPE_DOCK,
        atoms._NET_WM_WINDOW_TYPE_DROPDOWN_MENU,
        atoms._NET_WM_WINDOW_TYPE_MENU,
        atoms._NET_WM_WINDOW_TYPE_NOTIFICATION,
        atoms._NET_WM_WINDOW_TYPE_POPUP_MENU,
        atoms._NET_WM_WINDOW_TYPE_SPLASH,
        atoms._NET_WM_WINDOW_TYPE_TOOLBAR,
        atoms._NET_WM_WINDOW_TYPE_TOOLTIP,
        atoms._NET_WM_WINDOW_TYPE_UTILITY,
    ];
    if atom == atoms._NET_WM_WINDOW_TYPE_NORMAL {
        WindowType::Normal
    } else if special.contains(&atom) {
        WindowType::Special
    } else {
        WindowType::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(desktop: Option<u32>, minimized: bool) -> WindowRecord {
        WindowRecord {
            window: 0x0040_0007,
            frame: Rect::new(0.0, 0.0, 100.0, 100.0),
            geometry: FrameGeometry::default(),
            facts: WindowFacts {
                minimized,
                ..WindowFacts::default()
            },
            desktop,
            pid: None,
            class: Some("Firefox".into()),
        }
    }

    #[test]
    fn visibility_follows_the_native_desktop() {
        assert!(record(Some(1), false).is_visible(Some(1)));
        assert!(!record(Some(1), false).is_visible(Some(0)));
        assert!(record(Some(ALL_DESKTOPS), false).is_visible(Some(3)));
        assert!(record(None, false).is_visible(Some(3)));
        assert!(!record(Some(0), true).is_visible(Some(0)));
    }

    #[test]
    fn window_ids_are_the_x_window_ids() {
        assert_eq!(record(None, false).id(), WindowId(0x0040_0007));
        assert_eq!(record(None, false).app(), "Firefox");
        assert_eq!(record(None, false).info().recovery_key, None);
    }

    #[test]
    fn app_ids_come_from_the_wm_class_class() {
        assert_eq!(class_name(b"navigator\0Firefox\0"), Some("Firefox".into()));
        assert_eq!(class_name(b"xterm\0"), None);
        assert_eq!(class_name(b""), None);
    }
}
