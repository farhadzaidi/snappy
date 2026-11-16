//! KDE Plasma on Wayland, in the sealed-off headless KWin the suite runs in
//! (`tools/live/kwin`). Wayland clients can't read or place other windows, so throwaway KWin
//! scripts do it, each answering over D-Bus; input goes through KWin's
//! `org_kde_kwin_fake_input`, which KWin handles like a device's.
//!
//! The headless KWin composites nothing, so whether Snappy's overlays show can't be seen:
//! their surfaces stay and only their alpha changes. The daemon's surfaces count as shown
//! while its debug log says the zones or the selection's outlines show; the adapter's own
//! tests check that hiding sets the alpha to 0.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde::de::DeserializeOwned;
use snappy_core::geometry::{Point, Rect};
use snappy_core::keys::{KeyCode, Modifier, Mods};
use snappy_engine::LogEvent;
use snappy_kwin::wire::{fake_input, keystate};
use snappy_linux::key_codes::{self, CAPS_LOCK};
use snappy_linux::wayland::{Arg, Connection};
use snappy_platform::Display;
use zbus::blocking::{Connection as Bus, MessageIterator};

use super::wayland::{CLICK_MS, FIRST_STEP, FOCUS_TIMEOUT, HOVER_MS, MOVE_START_TIMEOUT, POLL_MS};
use crate::input::{
    DRAG_HOLD_MS, DRAG_STEP_MS, Input, KEY_GAP_MS, MODIFIER_GAP_MS, MODIFIER_ORDER, along, pause,
};
use crate::screen::{FLOATING_LAYER, NORMAL_LAYER, ScreenWindow};

/// The bus name the probe scripts answer to.
const PROBE_NAME: &str = "io.github.farhadzaidi.SnappyLive";
const PROBE_PATH: &str = "/io/github/farhadzaidi/SnappyLive";
/// KWin's `MaximizeArea` client area, a display minus panels, as the script names it too.
const MAXIMIZE_AREA: u32 = 2;
/// How long a probe script may take to answer.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// The primary button's Linux input code.
const BUTTON_LEFT: u32 = 0x110;

/// Whether the suite runs in a KDE Plasma on Wayland session.
pub fn is_session() -> bool {
    snappy_linux::session::current() == snappy_linux::session::Session::KdeWayland
}

static DEBUG_LOG: OnceLock<PathBuf> = OnceLock::new();

/// Where the daemon's debug log is, which says whether its overlay shows.
pub fn set_debug_log(path: &Path) {
    let _ = DEBUG_LOG.set(path.to_owned());
}

/// The bus the probe scripts answer on, and their answers as they arrive, each with the
/// number of the probe that sent it.
struct ProbeBus {
    bus: Bus,
    answers: Mutex<Receiver<(u32, serde_json::Value)>>,
}

fn probe_bus() -> Result<&'static ProbeBus, String> {
    static BUS: OnceLock<Result<ProbeBus, String>> = OnceLock::new();
    BUS.get_or_init(|| {
        let bus = zbus::blocking::connection::Builder::session()
            .and_then(|builder| builder.name(PROBE_NAME))
            .and_then(zbus::blocking::connection::Builder::build)
            .map_err(|error| format!("the session bus can't be reached: {error}"))?;
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::MethodCall)
            .path(PROBE_PATH)
            .map_err(|error| error.to_string())?
            .build();
        let calls = MessageIterator::for_match_rule(rule, &bus, None).map_err(|error| error.to_string())?;
        let (answers, received) = mpsc::channel();
        let replier = bus.clone();
        std::thread::spawn(move || {
            for message in calls.flatten() {
                let _ = replier.reply(&message.header(), &());
                let answer = message
                    .body()
                    .deserialize::<String>()
                    .ok()
                    .and_then(|json| serde_json::from_str(&json).ok());
                if let Some(answer) = answer
                    && answers.send(answer).is_err()
                {
                    return;
                }
            }
        });
        Ok(ProbeBus {
            bus,
            answers: Mutex::new(received),
        })
    })
    .as_ref()
    .map_err(Clone::clone)
}

/// Runs `body`, JavaScript whose value is the answer, as a throwaway KWin script, and reads
/// the answer back as JSON.
fn probe<T: DeserializeOwned>(body: &str) -> Result<T, String> {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let next = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let ProbeBus { bus, answers } = probe_bus()?;
    let answers = answers.lock().unwrap_or_else(PoisonError::into_inner);

    let folder = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let path = folder.join(format!("snappy-live-probe-{}-{next}.js", std::process::id()));
    let script = format!(
        "const answer = (() => {{ {body} }})();\n\
         callDBus(\"{PROBE_NAME}\", \"{PROBE_PATH}\", \"{PROBE_NAME}\", \"Report\", JSON.stringify([{next}, answer]));"
    );
    std::fs::write(&path, script).map_err(|error| error.to_string())?;
    let name = format!("snappy-live-probe-{next}");
    let result = snappy_kwin::reference::run_plain_script(bus, &path, &name).and_then(|()| {
        // An answer from an earlier probe that gave up waiting is passed over.
        let deadline = Instant::now() + PROBE_TIMEOUT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match answers.recv_timeout(left) {
                Ok((probe, answer)) if probe == next => break Ok(answer),
                Ok(_) => {}
                Err(_) => break Err("KWin's probe script didn't answer".to_owned()),
            }
        }
    });
    snappy_kwin::reference::unload_script(bus, &name);
    let _ = std::fs::remove_file(&path);
    serde_json::from_value(result?).map_err(|error| error.to_string())
}

/// A JavaScript string literal for `text`.
fn quoted(text: &str) -> String {
    serde_json::Value::String(text.to_owned()).to_string()
}

#[derive(Deserialize)]
struct Frame {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl From<Frame> for Rect {
    fn from(frame: Frame) -> Rect {
        Rect::new(frame.x, frame.y, frame.width, frame.height)
    }
}

/// JavaScript finding the fixture window with this title, as `window`, or answering null.
fn find(title: &str) -> String {
    format!(
        "const window = workspace.windowList().find(w => w.caption === {}); if (!window) return null;",
        quoted(title)
    )
}

const FRAME: &str =
    "const g = window.frameGeometry; return {x: g.x, y: g.y, width: g.width, height: g.height};";

/// The frame of the window with this title, as KWin knows it.
pub fn window_frame(title: &str) -> Result<Option<Rect>, String> {
    Ok(probe::<Option<Frame>>(&format!("{} {FRAME}", find(title)))?.map(Rect::from))
}

/// Moves the window with this title so its frame starts at `origin`.
pub fn place_window(title: &str, origin: Point) -> Result<Rect, String> {
    let body = format!(
        "{} const size = window.frameGeometry; \
         window.frameGeometry = {{x: {}, y: {}, width: size.width, height: size.height}}; {FRAME}",
        find(title),
        origin.x,
        origin.y
    );
    probe::<Option<Frame>>(&body)?
        .map(Rect::from)
        .ok_or_else(|| format!("no window {title}"))
}

/// Activates the window, then clicks in its middle, as a person would.
pub fn focus_window(title: &str) -> Result<(), String> {
    let body = format!("{} workspace.activeWindow = window; {FRAME}", find(title));
    let frame: Rect = probe::<Option<Frame>>(&body)?
        .map(Rect::from)
        .ok_or_else(|| format!("no window {title}"))?;
    device()?.pointer_to(frame.center())?;
    pause(HOVER_MS);
    device()?.button(true)?;
    pause(CLICK_MS);
    device()?.button(false)?;
    let focused = format!(
        "const active = workspace.activeWindow; return active !== null && active.caption === {};",
        quoted(title)
    );
    let started = Instant::now();
    while !probe::<bool>(&focused)? {
        if started.elapsed() > FOCUS_TIMEOUT {
            return Err(format!("{title} never got focus"));
        }
        pause(POLL_MS);
    }
    Ok(())
}

/// Whether KWin is moving or resizing a window for the pointer.
fn grabbing() -> Result<bool, String> {
    probe("return workspace.windowList().some(w => w.move || w.resize);")
}

/// Whether the daemon's debug log last said its zones or its outlines show.
fn overlay_showing() -> bool {
    let Some(path) = DEBUG_LOG.get() else {
        return false;
    };
    let log = std::fs::read_to_string(path).unwrap_or_default();
    let records: Vec<serde_json::Value> = log
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    let last = |shown: LogEvent, hidden: LogEvent| {
        records
            .iter()
            .rev()
            .find_map(|record| match record["event"].as_str() {
                Some(event) if event == shown.as_str() => Some(true),
                Some(event) if event == hidden.as_str() => Some(false),
                _ => None,
            })
            .unwrap_or(false)
    };
    last(LogEvent::OverlayShown, LogEvent::OverlayHidden) || last(LogEvent::MarksShown, LogEvent::MarksHidden)
}

/// Normal windows on the desktop in use and the daemon's overlay while it shows, front to
/// back.
pub fn on_screen_windows(daemon: Option<i32>) -> Result<Vec<ScreenWindow>, String> {
    #[derive(Deserialize)]
    struct Shown {
        pid: i32,
        overlay: bool,
        frame: Frame,
    }
    let shown: Vec<Shown> = probe(
        "return workspace.stackingOrder.slice().reverse()
            .filter(w => !w.minimized && (w.onAllDesktops || w.desktops.includes(workspace.currentDesktop)))
            .filter(w => w.normalWindow || w.onScreenDisplay)
            .map(w => { const g = w.frameGeometry; return {pid: w.pid, overlay: w.onScreenDisplay,
                frame: {x: g.x, y: g.y, width: g.width, height: g.height}}; });",
    )?;
    let overlay_showing = overlay_showing();
    Ok(shown
        .into_iter()
        .filter(|window| !window.overlay || (Some(window.pid) == daemon && overlay_showing))
        .map(|window| ScreenWindow {
            pid: window.pid,
            layer: if window.overlay {
                FLOATING_LAYER
            } else {
                NORMAL_LAYER
            },
            frame: window.frame.into(),
        })
        .collect())
}

/// Every display, primary first, as Snappy's adapter reads them from KWin.
pub fn displays() -> Result<Vec<Display>, String> {
    let monitors: Vec<snappy_kwin::protocol::Monitor> = probe(&format!(
        "return workspace.screenOrder.map(output => {{
            const frame = output.geometry;
            const work = workspace.clientArea({MAXIMIZE_AREA}, output, workspace.currentDesktop);
            return {{name: output.name, manufacturer: output.manufacturer, model: output.model,
                serial: output.serialNumber, scale: output.devicePixelRatio,
                frame: {{x: frame.x, y: frame.y, width: frame.width, height: frame.height}},
                workArea: {{x: work.x, y: work.y, width: work.width, height: work.height}}}};
        }});"
    ))?;
    Ok(snappy_kwin::reference::displays(&monitors))
}

/// Lets go of the button, every modifier, and Caps Lock, so a failed scenario leaves nothing
/// held.
pub fn release_all() {
    let Ok(mut device) = device() else { return };
    let _ = device.button(false);
    for modifier in MODIFIER_ORDER {
        let _ = device.key_event(key_codes::modifier_keycode(modifier), false);
    }
    let _ = device.key_event(CAPS_LOCK, false);
}

/// The headless KWin composites nothing, so there is nothing to save.
pub fn screenshot(_path: &Path) {}

/// Input through KWin's fake input device, which it handles like hardware: window moves,
/// shortcuts, and the keystate Snappy reads all see it.
pub struct KWinInput;

impl KWinInput {
    /// Makes the run's device, before any fixture window opens.
    pub fn new() -> Result<Self, String> {
        device().map(|_| KWinInput)
    }
}

/// The run's one fake input device. KWin gives the seat a pointer and a keyboard when a device
/// appears, and an app already running can trip over that, so it is made once and shared.
fn device() -> Result<MutexGuard<'static, Device>, String> {
    static DEVICE: OnceLock<Result<Mutex<Device>, String>> = OnceLock::new();
    DEVICE
        .get_or_init(|| Device::open().map(Mutex::new))
        .as_ref()
        .map(|device| device.lock().unwrap_or_else(PoisonError::into_inner))
        .map_err(Clone::clone)
}

struct Device {
    connection: Connection,
    device: u32,
}

impl Device {
    fn open() -> Result<Self, String> {
        let mut connection = Connection::open().map_err(|error| error.to_string())?;
        let (device, version) = connection
            .bind(fake_input::INTERFACE, fake_input::VERSION)
            .map_err(|error| error.to_string())?;
        if version < fake_input::VERSION {
            return Err("KWin's fake input can't press keys".to_owned());
        }
        connection
            .send(
                device,
                fake_input::AUTHENTICATE,
                &[Arg::Str("Snappy live tests"), Arg::Str("Drive the test session")],
            )
            .map_err(|error| error.to_string())?;
        let mut input = Device { connection, device };
        input.sync()?;
        Ok(input)
    }

    /// Waits until KWin has handled everything sent so far.
    fn sync(&mut self) -> Result<(), String> {
        self.connection
            .roundtrip()
            .map(drop)
            .map_err(|error| error.to_string())
    }

    fn send(&mut self, opcode: u16, args: &[Arg]) -> Result<(), String> {
        self.connection
            .send(self.device, opcode, args)
            .map_err(|error| error.to_string())?;
        self.sync()
    }

    fn pointer_to(&mut self, at: Point) -> Result<(), String> {
        self.send(
            fake_input::POINTER_MOTION_ABSOLUTE,
            &[Arg::Fixed(at.x), Arg::Fixed(at.y)],
        )
    }

    fn button(&mut self, pressed: bool) -> Result<(), String> {
        self.send(
            fake_input::BUTTON,
            &[Arg::Uint(BUTTON_LEFT), Arg::Uint(state(pressed))],
        )
    }

    fn key_event(&mut self, keycode: u8, pressed: bool) -> Result<(), String> {
        let code = keycode
            .checked_sub(key_codes::EVDEV_OFFSET)
            .ok_or_else(|| format!("keycode {keycode} has no input code"))?;
        self.send(
            fake_input::KEYBOARD_KEY,
            &[Arg::Uint(u32::from(code)), Arg::Uint(state(pressed))],
        )
    }
}

/// A button's or key's state, as fake input takes it.
fn state(pressed: bool) -> u32 {
    if pressed {
        fake_input::PRESSED
    } else {
        fake_input::RELEASED
    }
}

impl KWinInput {
    fn pointer_to(&mut self, at: Point) -> Result<(), String> {
        device()?.pointer_to(at)
    }

    fn button(&mut self, pressed: bool) -> Result<(), String> {
        device()?.button(pressed)
    }

    fn key_event(&mut self, keycode: u8, pressed: bool) -> Result<(), String> {
        device()?.key_event(keycode, pressed)
    }
}

impl Input for KWinInput {
    fn modifiers(&mut self, mods: Mods, down: bool) -> Result<(), String> {
        let mut pressed: Vec<Modifier> = MODIFIER_ORDER.into_iter().filter(|m| mods.contains(*m)).collect();
        if !down {
            pressed.reverse();
        }
        for modifier in pressed {
            self.key_event(key_codes::modifier_keycode(modifier), down)?;
            pause(MODIFIER_GAP_MS);
        }
        Ok(())
    }

    /// Rests on the spot first, as a hand does, so the window under the pointer knows it's
    /// there before the press.
    fn pointer_down(&mut self, at: Point, _mods: Mods) -> Result<(), String> {
        self.pointer_to(at)?;
        pause(HOVER_MS);
        self.button(true)
    }

    fn pointer_drag(&mut self, to: Point, _mods: Mods) -> Result<(), String> {
        self.pointer_to(to)
    }

    fn pointer_up(&mut self, at: Point, _mods: Mods) -> Result<(), String> {
        self.pointer_to(at)?;
        self.button(false)
    }

    /// KWin starts moving a window once the pointer passes its drag threshold on the title
    /// bar, so the drag takes one short step, waits for the move to start, and covers the
    /// rest. Every drag here moves a window, so one KWin never starts moving is an error.
    fn drag(&mut self, from: Point, to: Point, mods: Mods, steps: u32) -> Result<(), String> {
        self.modifiers(mods, true)?;
        self.pointer_down(from, mods)?;
        pause(DRAG_HOLD_MS);
        let distance = ((to.x - from.x).powi(2) + (to.y - from.y).powi(2)).sqrt();
        if distance > FIRST_STEP {
            self.pointer_drag(along(from, to, FIRST_STEP / distance), mods)?;
            pause(DRAG_STEP_MS);
        }
        let started = Instant::now();
        while !grabbing()? {
            if started.elapsed() > MOVE_START_TIMEOUT {
                self.pointer_up(from, mods)?;
                self.modifiers(mods, false)?;
                return Err(format!("KWin didn't start moving the window pressed at {from:?}"));
            }
            pause(POLL_MS);
        }
        let steps = steps.max(1);
        for step in 1..=steps {
            self.pointer_drag(along(from, to, f64::from(step) / f64::from(steps)), mods)?;
            pause(DRAG_STEP_MS);
        }
        self.pointer_up(to, mods)?;
        self.modifiers(mods, false)
    }

    fn key(&mut self, code: &KeyCode, _mods: Mods) -> Result<(), String> {
        let keycode = key_codes::evdev_keycode(code.as_str())
            .ok_or_else(|| format!("no key code for {}", code.as_str()))?;
        for pressed in [true, false] {
            self.key_event(keycode, pressed)?;
            pause(KEY_GAP_MS);
        }
        Ok(())
    }

    fn caps_lock(&mut self, down: bool) -> Result<(), String> {
        self.key_event(CAPS_LOCK, down)
    }

    /// Caps Lock's state in KWin's keymap, as its keystate protocol reports it.
    fn caps_lock_on(&mut self) -> Result<bool, String> {
        let mut keystate = Connection::open().map_err(|error| error.to_string())?;
        let (object, _) = keystate
            .bind(keystate::INTERFACE, keystate::VERSION)
            .map_err(|error| error.to_string())?;
        keystate
            .send(object, keystate::FETCH_STATES, &[])
            .map_err(|error| error.to_string())?;
        let events = keystate.roundtrip().map_err(|error| error.to_string())?;
        Ok(events
            .iter()
            .filter(|event| event.object == object && event.opcode == keystate::STATE_CHANGED)
            .filter_map(|event| {
                let mut reader = event.reader();
                Some((reader.uint()?, reader.uint()?))
            })
            .any(|(key, state)| key == keystate::KEY_CAPS_LOCK && state == keystate::LOCKED))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probes_read_work_areas_as_snappys_script_does() {
        let service = include_str!("../../../kwin/script/service.js");
        assert!(service.contains(&format!("var MAXIMIZE_AREA = {MAXIMIZE_AREA};")));
    }
}
