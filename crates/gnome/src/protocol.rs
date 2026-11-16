//! What Snappy and its GNOME Shell extension say to each other. Requests go to the
//! extension's `Call` method as JSON and come back as `{"ok": ...}` or `{"error": ...}`;
//! events arrive in order on its `Event` signal, one JSON object each.

use serde::{Deserialize, Serialize};
#[cfg(test)]
use snappy_core::geometry::Rect;
pub use snappy_linux::scene::{Frame, OverlayMarks, OverlayOutline, OverlayScene, OverlayText, OverlayZone};
use snappy_platform::CapsLockInput;

/// Matches `PROTOCOL_VERSION` in the extension's `service.js`.
pub const PROTOCOL_VERSION: u32 = 5;

pub const EXTENSION_UUID: &str = "snappy@farhadzaidi.github.io";
/// GNOME Shell owns this name; the extension exports its object on Shell's connection.
pub const SHELL_BUS_NAME: &str = "org.gnome.Shell";
pub const OBJECT_PATH: &str = "/io/github/farhadzaidi/Snappy/Shell";
pub const INTERFACE: &str = "io.github.farhadzaidi.Snappy.Shell";
/// The interface's members: the method requests go through, the signal events come on, and
/// the property holding the protocol version.
pub const CALL_METHOD: &str = "Call";
pub const EVENT_SIGNAL: &str = "Event";
pub const VERSION_PROPERTY: &str = "Version";

/// The refusal `park` returns for a window GNOME shows on every workspace.
pub const REFUSED_ALL_WORKSPACES: &str = "allWorkspaces";
/// The refusal `park` returns when GNOME didn't move the window to Snappy's workspace.
pub const REFUSED_NOT_MOVED: &str = "notMoved";
/// The error a request names a window with when the window is gone.
pub const GONE: &str = "gone";
/// The error Caps Lock requests fail with on a GNOME Shell whose keyboard manager lacks the
/// calls Snappy adds its XKB option through.
pub const UNSUPPORTED: &str = "unsupported";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Request {
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
    /// The XKB options GNOME builds from the user's settings, without Snappy's.
    KeyboardOptions,
    /// Add Snappy's XKB option to GNOME's keymap and report presses of Caps Lock alone.
    /// `modifiers` is the Snappy key as an accelerator prefix and `mask` its modifier bits;
    /// `keycodes` are grabbed with `modifiers` while Caps Lock is down, to learn that
    /// something else was used.
    CapsLockHold {
        modifiers: String,
        mask: u32,
        keycodes: Vec<u8>,
    },
    /// Take Snappy's option out of the keymap and stop reporting Caps Lock.
    CapsLockRestore,
    /// Turn Caps Lock on or off as its key does.
    CapsLockToggle,
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
    /// Outline windows on a display apart from its zones; no outlines removes them.
    OverlayMark {
        display: String,
        marks: OverlayMarks,
    },
    /// `name` is what assistive technology calls the item. Choices come back with
    /// `generation`, so a click on a menu that was replaced meanwhile is ignored.
    Menu {
        name: String,
        generation: u32,
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
        generation: u32,
    },
    /// Caps Lock, while it is the Snappy key, went down or up alone, or something else was
    /// used meanwhile.
    CapsLock {
        phase: CapsLockInput,
    },
    /// The XKB options GNOME builds from the user's settings changed while Snappy's option
    /// is in the keymap; these are the new ones, without Snappy's.
    KeyboardOptions {
        options: String,
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
        assert_eq!(serde_json::to_string(&Request::Bind).unwrap(), r#"{"op":"bind"}"#);
        let json = serde_json::to_string(&Request::CapsLockHold {
            modifiers: "<Control><Alt><Super>".into(),
            mask: 76,
            keycodes: vec![10, 113],
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"op":"capsLockHold","modifiers":"<Control><Alt><Super>","mask":76,"keycodes":[10,113]}"#
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
        let event: Event = serde_json::from_str(r#"{"type":"capsLock","phase":"otherInput"}"#).unwrap();
        assert_eq!(
            event,
            Event::CapsLock {
                phase: CapsLockInput::OtherInput
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

    /// The names in `source` that follow each `prefix`, up to the closing quote.
    fn quoted_after<'a>(source: &'a str, prefix: &str) -> Vec<&'a str> {
        source
            .split(prefix)
            .skip(1)
            .filter_map(|rest| rest.split('\'').next())
            .collect()
    }

    /// One request of each kind. The match stops compiling when a kind is added, as a
    /// reminder to list it here too.
    fn one_of_each_request() -> Vec<Request> {
        let scene = OverlayScene {
            work_area: Frame::default(),
            zones: Vec::new(),
        };
        let marks = OverlayMarks {
            work_area: Frame::default(),
            outlines: Vec::new(),
        };
        let display = String::new();
        let all = vec![
            Request::Windows,
            Request::Window { id: 0 },
            Request::Focused,
            Request::Follow { ids: Vec::new() },
            Request::SetPosition {
                id: 0,
                x: 0.0,
                y: 0.0,
            },
            Request::SetSize {
                id: 0,
                width: 0.0,
                height: 0.0,
            },
            Request::Unmaximize { id: 0 },
            Request::Activate { id: 0 },
            Request::ClearFocus,
            Request::Pointer,
            Request::Monitors,
            Request::Monitoring {
                press: false,
                drag: false,
                follow: 0,
            },
            Request::Register { keys: Vec::new() },
            Request::UnregisterAll,
            Request::IsFree {
                accelerator: String::new(),
            },
            Request::KeyboardOptions,
            Request::CapsLockHold {
                modifiers: String::new(),
                mask: 0,
                keycodes: Vec::new(),
            },
            Request::CapsLockRestore,
            Request::CapsLockToggle,
            Request::OverlayPrepare {
                display: display.clone(),
                scene: scene.clone(),
            },
            Request::OverlayShow {
                display: display.clone(),
                scene,
            },
            Request::OverlayHide,
            Request::OverlayRetain { displays: Vec::new() },
            Request::OverlayMark { display, marks },
            Request::Menu {
                name: String::new(),
                generation: 0,
                entries: Vec::new(),
            },
            Request::Bind,
            Request::OnBoundWorkspace,
            Request::ClearWorkspaces,
            Request::Park { id: 0 },
            Request::Unpark { id: 0 },
        ];
        for request in &all {
            match request {
                Request::Windows
                | Request::Window { .. }
                | Request::Focused
                | Request::Follow { .. }
                | Request::SetPosition { .. }
                | Request::SetSize { .. }
                | Request::Unmaximize { .. }
                | Request::Activate { .. }
                | Request::ClearFocus
                | Request::Pointer
                | Request::Monitors
                | Request::Monitoring { .. }
                | Request::Register { .. }
                | Request::UnregisterAll
                | Request::IsFree { .. }
                | Request::KeyboardOptions
                | Request::CapsLockHold { .. }
                | Request::CapsLockRestore
                | Request::CapsLockToggle
                | Request::OverlayPrepare { .. }
                | Request::OverlayShow { .. }
                | Request::OverlayHide
                | Request::OverlayRetain { .. }
                | Request::OverlayMark { .. }
                | Request::Menu { .. }
                | Request::Bind
                | Request::OnBoundWorkspace
                | Request::ClearWorkspaces
                | Request::Park { .. }
                | Request::Unpark { .. } => {}
            }
        }
        all
    }

    /// Every file of the extension, for the names they share with the daemon.
    const EXTENSION: [&str; 10] = [
        include_str!("../../../apps/gnome-extension/service.js"),
        include_str!("../../../apps/gnome-extension/windows.js"),
        include_str!("../../../apps/gnome-extension/input.js"),
        include_str!("../../../apps/gnome-extension/capsLock.js"),
        include_str!("../../../apps/gnome-extension/parking.js"),
        include_str!("../../../apps/gnome-extension/shortcuts.js"),
        include_str!("../../../apps/gnome-extension/menu.js"),
        include_str!("../../../apps/gnome-extension/overlay.js"),
        include_str!("../../../apps/gnome-extension/compat.js"),
        include_str!("../../../apps/gnome-extension/extension.js"),
    ];

    #[test]
    fn the_extension_handles_every_request_and_reports_only_known_events() {
        let service = EXTENSION[0];
        let handled = quoted_after(service, "case '");
        for request in one_of_each_request() {
            let json = serde_json::to_value(&request).unwrap();
            let op = json["op"].as_str().unwrap();
            assert!(handled.contains(&op), "the extension doesn't handle {op}");
        }
        assert_eq!(handled.len(), one_of_each_request().len(), "{handled:?}");
        for source in EXTENSION {
            for kind in quoted_after(source, "type: '") {
                let event = serde_json::from_value::<Event>(serde_json::json!({ "type": kind }));
                assert!(
                    !matches!(event, Ok(Event::Unknown)),
                    "the daemon doesn't know {kind}"
                );
            }
            for phase in quoted_after(source, "phase: '") {
                let input = serde_json::from_value::<CapsLockInput>(serde_json::json!(phase));
                assert!(
                    input.is_ok(),
                    "the daemon doesn't know the Caps Lock phase {phase}"
                );
            }
        }
        let option = snappy_linux::xkb::OPTION;
        assert!(
            EXTENSION[3].contains(&format!("const OPTION = '{option}';")),
            "{option}"
        );
        for (name, source) in [(GONE, EXTENSION[0]), (UNSUPPORTED, EXTENSION[3])] {
            assert!(source.contains(&format!("'{name}'")), "{name}");
        }
    }

    /// The extension's source declares the same protocol version.
    #[test]
    fn the_extension_speaks_this_protocol_version() {
        let service = include_str!("../../../apps/gnome-extension/service.js");
        assert!(service.contains(&format!("export const PROTOCOL_VERSION = {PROTOCOL_VERSION};")));
        assert_eq!(OBJECT_PATH, snappy_linux::names::object_path("Shell"));
        assert_eq!(INTERFACE, format!("{}.Shell", snappy_linux::names::APP_ID));
        assert!(service.contains(OBJECT_PATH));
        assert!(service.contains(&format!("<interface name=\"{INTERFACE}\">")));
        assert!(service.contains(&format!("<method name=\"{CALL_METHOD}\">")));
        assert!(service.contains(&format!("<signal name=\"{EVENT_SIGNAL}\">")));
        assert!(service.contains(&format!("<property name=\"{VERSION_PROPERTY}\"")));
        let parking = include_str!("../../../apps/gnome-extension/parking.js");
        for refusal in [REFUSED_ALL_WORKSPACES, REFUSED_NOT_MOVED] {
            assert!(parking.contains(&format!("'{refusal}'")), "{refusal}");
        }
    }
}
