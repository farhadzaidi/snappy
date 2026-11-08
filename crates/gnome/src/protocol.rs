//! What Snappy and its GNOME Shell extension say to each other. Requests go to the
//! extension's `Call` method as JSON and come back as `{"ok": ...}` or `{"error": ...}`;
//! events arrive in order on its `Event` signal, one JSON object each.

use serde::{Deserialize, Serialize};
use snappy_core::geometry::Rect;

/// Matches `PROTOCOL_VERSION` in the extension's `service.js`.
pub const PROTOCOL_VERSION: u32 = 2;

pub const EXTENSION_UUID: &str = "snappy@farhadzaidi.github.io";
/// GNOME Shell owns this name; the extension exports its object on Shell's connection.
pub const SHELL_BUS_NAME: &str = "org.gnome.Shell";
pub const OBJECT_PATH: &str = "/io/github/farhadzaidi/Snappy/Shell";
pub const INTERFACE: &str = "io.github.farhadzaidi.Snappy.Shell";

/// The refusal `park` returns for a window GNOME shows on every workspace.
pub const REFUSED_ALL_WORKSPACES: &str = "allWorkspaces";
/// The error a request names a window with when the window is gone.
pub const GONE: &str = "gone";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Request {
    Hello,
    Windows,
    Window {
        id: u64,
    },
    Focused,
    Follow {
        ids: Vec<u64>,
    },
    SetPosition {
        id: u64,
        x: f64,
        y: f64,
    },
    SetSize {
        id: u64,
        width: f64,
        height: f64,
    },
    Unmaximize {
        id: u64,
    },
    Activate {
        id: u64,
    },
    ClearFocus,
    Pointer,
    Monitors,
    /// `follow`: the modifier mask that keeps a dragged window under the pointer while held.
    Monitoring {
        press: bool,
        drag: bool,
        follow: u32,
    },
    Register {
        keys: Vec<Accelerator>,
    },
    UnregisterAll,
    IsFree {
        accelerator: String,
    },
    OverlayPrepare {
        display: String,
        scene: OverlayScene,
    },
    OverlayShow {
        display: String,
        scene: OverlayScene,
    },
    OverlayHide,
    OverlayRetain {
        displays: Vec<String>,
    },
    Menu {
        tooltip: String,
        entries: Vec<MenuEntry>,
    },
    Bind,
    OnBoundWorkspace,
    ClearWorkspaces,
    Park {
        id: u64,
    },
    Unpark {
        id: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Accelerator {
    pub id: u32,
    pub accelerator: String,
}

/// A frame in stage coordinates, as GNOME Shell reports it.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Frame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl From<Frame> for Rect {
    fn from(frame: Frame) -> Rect {
        Rect::new(frame.x, frame.y, frame.width, frame.height)
    }
}

impl From<Rect> for Frame {
    fn from(rect: Rect) -> Frame {
        Frame {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        }
    }
}

/// The facts the extension reports about a managed window.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WindowRecord {
    /// Mutter's stable sequence, unique for the session.
    pub id: u64,
    pub pid: u32,
    /// The app's desktop file ID, when the window belongs to an installed app.
    pub app_id: Option<String>,
    pub wm_class: Option<String>,
    pub frame: Frame,
    pub normal: bool,
    pub transient: bool,
    pub allows_move: bool,
    pub allows_resize: bool,
    pub full_screen: bool,
    pub maximized: bool,
    pub minimized: bool,
    pub on_active_workspace: bool,
    pub on_all_workspaces: bool,
    pub parked: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Monitor {
    pub index: u32,
    pub primary: bool,
    pub frame: Frame,
    pub work_area: Frame,
    pub scale: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct PointerPosition {
    pub x: f64,
    pub y: f64,
}

/// One display's overlay, every length in stage pixels and relative to the work area.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayScene {
    pub work_area: Frame,
    pub zones: Vec<OverlayZone>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayZone {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub radius: f64,
    /// Red, green, and blue from 0 to 255, then opacity from 0 to 1.
    pub fill: [f64; 4],
    pub border: [f64; 4],
    pub border_width: f64,
    pub number: OverlayText,
    pub caption: Option<OverlayText>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayText {
    pub text: String,
    pub center_y: f64,
    pub size: f64,
    pub shadow_blur: f64,
    pub shadow_opacity: f64,
}

/// A menu entry as the extension draws it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum MenuEntry {
    Label {
        title: String,
    },
    Separator,
    Item {
        id: u32,
        title: String,
        checked: Option<bool>,
        enabled: bool,
    },
    Submenu {
        title: String,
        entries: Vec<MenuEntry>,
    },
}

/// What the extension reports, in the order it happened.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Event {
    /// The extension started, or started again.
    Ready,
    PointerDown {
        x: f64,
        y: f64,
        mods: u32,
        window: u64,
    },
    PointerDragged {
        x: f64,
        y: f64,
        mods: u32,
    },
    PointerUp {
        x: f64,
        y: f64,
        mods: u32,
    },
    Modifiers {
        x: f64,
        y: f64,
        mods: u32,
    },
    Hotkey {
        id: u32,
    },
    WindowAdded {
        id: u64,
        pid: u32,
    },
    WindowRemoved {
        id: u64,
    },
    WindowMoved {
        id: u64,
    },
    Focus {
        pid: u32,
    },
    Displays,
    Workspace,
    Menu {
        item: u32,
    },
    /// Anything a newer extension reports that this version doesn't know.
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_are_tagged_with_their_operation() {
        let json = serde_json::to_string(&Request::SetPosition {
            id: 7,
            x: 10.0,
            y: 20.0,
        })
        .unwrap();
        assert_eq!(json, r#"{"op":"setPosition","id":7,"x":10.0,"y":20.0}"#);
        assert_eq!(
            serde_json::to_string(&Request::Hello).unwrap(),
            r#"{"op":"hello"}"#
        );
        let json = serde_json::to_string(&Request::Monitoring {
            press: true,
            drag: false,
            follow: 1,
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"op":"monitoring","press":true,"drag":false,"follow":1}"#
        );
    }

    #[test]
    fn events_read_from_the_extensions_json() {
        let event: Event =
            serde_json::from_str(r#"{"type":"pointerDown","x":3,"y":4,"mods":1,"window":12}"#).unwrap();
        assert_eq!(
            event,
            Event::PointerDown {
                x: 3.0,
                y: 4.0,
                mods: 1,
                window: 12
            }
        );
        let event: Event = serde_json::from_str(r#"{"type":"somethingNew","detail":1}"#).unwrap();
        assert_eq!(event, Event::Unknown);
    }

    #[test]
    fn window_records_tolerate_missing_facts() {
        let record: WindowRecord = serde_json::from_str(
            r#"{"id":4,"pid":99,"appId":null,"wmClass":"gjs","frame":{"x":1,"y":2,"width":3,"height":4},
                "normal":true,"allowsMove":true,"allowsResize":true,"futureFact":true}"#,
        )
        .unwrap();
        assert_eq!(record.id, 4);
        assert_eq!(record.wm_class.as_deref(), Some("gjs"));
        assert!(!record.parked);
        assert_eq!(Rect::from(record.frame), Rect::new(1.0, 2.0, 3.0, 4.0));
    }

    #[test]
    fn menu_entries_are_tagged_with_their_kind() {
        let json = serde_json::to_string(&MenuEntry::Item {
            id: 3,
            title: "Quit Snappy".into(),
            checked: None,
            enabled: true,
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"kind":"item","id":3,"title":"Quit Snappy","checked":null,"enabled":true}"#
        );
    }

    /// The extension's source declares the same protocol version.
    #[test]
    fn the_extension_speaks_this_protocol_version() {
        let service = include_str!("../../../apps/gnome-extension/service.js");
        assert!(service.contains(&format!("export const PROTOCOL_VERSION = {PROTOCOL_VERSION};")));
        assert!(service.contains(OBJECT_PATH));
        assert!(service.contains(&format!("<interface name=\"{INTERFACE}\">")));
    }
}
