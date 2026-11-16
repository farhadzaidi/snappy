//! What KWin itself says about windows and on-screen displays, asked by throwaway scripts
//! and over its D-Bus interface, never through Snappy.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use snappy_core::geometry::Rect;
use zbus::zvariant::OwnedValue;

use crate::support::WAIT;

const PROBE_NAME: &str = "io.github.farhadzaidi.SnappyTest";
const PROBE_PATH: &str = "/io/github/farhadzaidi/SnappyTest";

/// An on-screen display window as KWin knows it: whose, where, and its KWin UUID, which
/// changes only when the window closes and another opens.
#[derive(Debug, Clone, PartialEq)]
pub struct OsdWindow {
    pub pid: u32,
    pub frame: Rect,
    pub uuid: String,
}

/// The on-screen display surfaces KWin shows, Snappy's overlay among them, by process: asked
/// of KWin by a throwaway script in the test session, never through Snappy.
fn on_screen_displays() -> Vec<OsdWindow> {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let next = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let out = std::env::var_os("SNAPPY_KWIN_OUT").expect("SNAPPY_KWIN_OUT is set in the test session");
    let path = std::path::PathBuf::from(out).join(format!("probe-{}-{next}.js", std::process::id()));
    std::fs::write(
        &path,
        format!(
            r#"const found = workspace.windowList().filter(w => w.onScreenDisplay).map(w => {{
                const g = w.frameGeometry;
                return [w.pid, g.x, g.y, g.width, g.height, String(w.internalId)];
            }});
            callDBus("{PROBE_NAME}", "{PROBE_PATH}", "{PROBE_NAME}", "Report", JSON.stringify(found));"#
        ),
    )
    .unwrap();
    let bus = zbus::blocking::connection::Builder::session()
        .unwrap()
        .name(PROBE_NAME)
        .unwrap()
        .build()
        .unwrap();
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::MethodCall)
        .path(PROBE_PATH)
        .unwrap()
        .build();
    let calls = zbus::blocking::MessageIterator::for_match_rule(rule, &bus, None).unwrap();
    let script = format!("snappy-probe-{next}");
    snappy_kwin::reference::run_plain_script(&bus, &path, &script).unwrap();
    let message = calls.into_iter().next().unwrap().unwrap();
    let json: String = message.body().deserialize().unwrap();
    bus.reply(&message.header(), &()).unwrap();
    snappy_kwin::reference::unload_script(&bus, &script);
    std::fs::remove_file(&path).unwrap();
    let found: Vec<(u32, f64, f64, f64, f64, String)> = serde_json::from_str(&json).unwrap();
    found
        .into_iter()
        .map(|(pid, x, y, width, height, uuid)| OsdWindow {
            pid,
            frame: Rect::new(x, y, width, height),
            uuid,
        })
        .collect()
}

/// Waits until `pid` has an on-screen display window at `frame`, and returns it.
pub fn wait_for_overlay(pid: u32, frame: Rect) -> OsdWindow {
    let deadline = Instant::now() + WAIT;
    loop {
        let found: Vec<OsdWindow> = on_screen_displays()
            .into_iter()
            .filter(|window| window.pid == pid)
            .collect();
        if let Some(window) = found.iter().find(|window| window.frame == frame) {
            return window.clone();
        }
        assert!(
            Instant::now() < deadline,
            "the overlay of {pid} is at {found:?}, not {frame:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// `pid`'s on-screen display windows, in the order KWin lists them.
pub fn overlay_windows(pid: u32) -> Vec<OsdWindow> {
    on_screen_displays()
        .into_iter()
        .filter(|window| window.pid == pid)
        .collect()
}

/// KWin's own view of a window, found by its title, so the end-to-end test checks frames
/// without going through Snappy.
pub fn kwin_frame(bus: &zbus::blocking::Connection, title: &str) -> Option<Rect> {
    let info = kwin_info(bus, title)?;
    let number = |key: &str| -> Option<f64> {
        let value = info.get(key)?;
        f64::try_from(value)
            .ok()
            .or_else(|| i32::try_from(value).ok().map(f64::from))
    };
    Some(Rect::new(
        number("x")?,
        number("y")?,
        number("width")?,
        number("height")?,
    ))
}

/// Whether KWin keeps the window off the task manager, as Snappy's parking does.
pub fn kwin_skips_taskbar(bus: &zbus::blocking::Connection, title: &str) -> Option<bool> {
    bool::try_from(kwin_info(bus, title)?.get("skipTaskbar")?).ok()
}

/// KWin's facts about the window with this title, from its D-Bus interface.
fn kwin_info(bus: &zbus::blocking::Connection, title: &str) -> Option<HashMap<String, OwnedValue>> {
    type Matches = Vec<(String, String, String, i32, f64, HashMap<String, OwnedValue>)>;
    let reply = bus
        .call_method(
            Some("org.kde.KWin"),
            "/WindowsRunner",
            Some("org.kde.krunner1"),
            "Match",
            &(title,),
        )
        .ok()?;
    let matches: Matches = reply.body().deserialize().ok()?;
    let (id, ..) = matches.into_iter().find(|(_, text, ..)| text == title)?;
    let uuid = id
        .split_once('_')
        .map_or(id.as_str(), |(_, uuid)| uuid)
        .to_owned();
    let reply = bus
        .call_method(
            Some("org.kde.KWin"),
            "/KWin",
            Some("org.kde.KWin"),
            "getWindowInfo",
            &(uuid,),
        )
        .ok()?;
    reply.body().deserialize().ok()
}
