//! What Snappy and its KWin script say to each other.
//!
//! KWin scripts can call D-Bus methods but can't offer any, so the daemon is the one with a
//! bus name. The script reports events by calling `Event`. Requests wait in the daemon until
//! the script collects them: the daemon rings a doorbell, a global shortcut action the script
//! registers that KDE's shortcut service can trigger over D-Bus, and the script calls `Take`.
//! It answers with `Answer`, whose reply carries any requests queued meanwhile, so a burst of
//! requests needs one ring. Nothing runs while nobody asks.

use serde::{Deserialize, Serialize};
pub use snappy_linux::scene::Frame;

/// Matches `PROTOCOL_VERSION` in the script's `service.js`.
pub const PROTOCOL_VERSION: u32 = 2;

/// The name KWin knows the loaded script by.
pub const SCRIPT_NAME: &str = "snappy";
/// The daemon's name on the session bus, where the script finds it.
pub const BUS_NAME: &str = "io.github.farhadzaidi.Snappy";
pub const OBJECT_PATH: &str = "/io/github/farhadzaidi/Snappy/KWin";
pub const INTERFACE: &str = "io.github.farhadzaidi.Snappy.KWin";
/// The shortcut action the script registers as its doorbell, in KWin's component.
pub const DOORBELL: &str = "snappy-requests";

/// The refusal `park` returns for a window KWin shows on every desktop.
pub const REFUSED_ALL_DESKTOPS: &str = "allDesktops";
/// The error a request names a window with when the window is gone.
pub const GONE: &str = "gone";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Request {
    Windows,
    Window { id: u64 },
    Focused,
    Follow { ids: Vec<u64> },
    SetPosition { id: u64, x: f64, y: f64 },
    SetSize { id: u64, width: f64, height: f64 },
    Unmaximize { id: u64 },
    Activate { id: u64 },
    ClearFocus,
    Pointer,
    Monitors,
    Monitoring { press: bool, drag: bool },
    Bind,
    OnBoundDesktop,
    ClearDesktops,
    Park { id: u64 },
    Unpark { id: u64 },
}

/// A request as it waits for the script, numbered so its answer finds its way back.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Numbered<'a> {
    pub seq: u64,
    #[serde(flatten)]
    pub request: &'a Request,
}

/// The script's answer to one numbered request: `ok` with a value, or `error`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Answer {
    pub seq: u64,
    #[serde(default)]
    pub ok: serde_json::Value,
    #[serde(default)]
    pub error: Option<String>,
}

/// The facts the script reports about a managed window.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WindowRecord {
    /// Taken from KWin's internal UUID, so it survives the script and the daemon restarting.
    pub id: u64,
    pub uuid: String,
    pub pid: u32,
    /// The app's desktop file name, when KWin knows it.
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
    pub on_current_desktop: bool,
    pub on_all_desktops: bool,
    pub parked: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Monitor {
    /// The connector, such as `DP-1`.
    pub name: String,
    pub manufacturer: String,
    pub model: String,
    pub serial: String,
    pub frame: Frame,
    pub work_area: Frame,
    pub scale: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct PointerPosition {
    pub x: f64,
    pub y: f64,
}

/// What the script reports, in the order it happened. Pointer events carry no modifiers:
/// KWin scripts can't read them, so the daemon follows them itself during drags.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Event {
    /// The script started, or started again.
    Ready {
        version: u32,
    },
    PointerDown {
        x: f64,
        y: f64,
        window: u64,
    },
    PointerDragged {
        x: f64,
        y: f64,
    },
    PointerUp {
        x: f64,
        y: f64,
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
    Desktop,
    /// Anything a newer script reports that this version doesn't know.
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use snappy_core::geometry::Rect;

    use super::*;

    #[test]
    fn requests_carry_their_number_and_operation() {
        let request = Request::SetPosition {
            id: 7,
            x: 10.0,
            y: 20.0,
        };
        let json = serde_json::to_string(&Numbered {
            seq: 3,
            request: &request,
        })
        .unwrap();
        assert_eq!(json, r#"{"seq":3,"op":"setPosition","id":7,"x":10.0,"y":20.0}"#);
    }

    #[test]
    fn answers_carry_a_value_or_an_error() {
        let answers: Vec<Answer> =
            serde_json::from_str(r#"[{"seq":1,"ok":[1,2]},{"seq":2,"error":"gone"},{"seq":3,"ok":null}]"#)
                .unwrap();
        assert_eq!(answers[0].ok, serde_json::json!([1, 2]));
        assert_eq!(answers[1].error.as_deref(), Some(GONE));
        assert_eq!(answers[2].error, None);
    }

    #[test]
    fn events_read_from_the_scripts_json() {
        let event: Event = serde_json::from_str(r#"{"type":"pointerDown","x":3,"y":4,"window":12}"#).unwrap();
        assert_eq!(
            event,
            Event::PointerDown {
                x: 3.0,
                y: 4.0,
                window: 12
            }
        );
        let event: Event = serde_json::from_str(r#"{"type":"somethingNew","detail":1}"#).unwrap();
        assert_eq!(event, Event::Unknown);
    }

    #[test]
    fn window_records_tolerate_missing_facts() {
        let record: WindowRecord = serde_json::from_str(
            r#"{"id":4,"pid":99,"appId":null,"wmClass":"konsole","frame":{"x":1,"y":2,"width":3,"height":4},
                "normal":true,"allowsMove":true,"allowsResize":true,"futureFact":true}"#,
        )
        .unwrap();
        assert_eq!(record.id, 4);
        assert_eq!(record.wm_class.as_deref(), Some("konsole"));
        assert!(!record.parked);
        assert_eq!(Rect::from(record.frame), Rect::new(1.0, 2.0, 3.0, 4.0));
    }

    /// The script's source declares the same protocol version and names.
    #[test]
    fn the_script_speaks_this_protocol_version() {
        let service = include_str!("../../../apps/kwin-script/contents/ui/service.js");
        assert!(service.contains(&format!("var PROTOCOL_VERSION = {PROTOCOL_VERSION};")));
        for name in [BUS_NAME, OBJECT_PATH, INTERFACE, DOORBELL] {
            assert!(service.contains(&format!("'{name}'")), "{name}");
        }
    }
}
