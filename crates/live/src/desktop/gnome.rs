//! GNOME on Wayland, through the test helper extension in the throwaway GNOME Shell the
//! suite runs in (`tools/live/gnome/test-extension`). Wayland clients can't inject input or
//! read other windows, and fixture windows can't place or read their own, so the helper does
//! all of that from inside GNOME Shell.

use std::sync::OnceLock;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};
use snappy_core::geometry::{Point, Rect};
use snappy_core::keys::{KeyCode, Modifier, Mods};
use snappy_linux::key_codes::{self, CAPS_LOCK};
use zbus::blocking::Connection;

use super::wayland::{CLICK_MS, FIRST_STEP, FOCUS_TIMEOUT, HOVER_MS, MOVE_START_TIMEOUT, POLL_MS};
use crate::input::{
    DRAG_HOLD_MS, DRAG_STEP_MS, Input, KEY_GAP_MS, MODIFIER_GAP_MS, MODIFIER_ORDER, along, pause,
};
use crate::screen::ScreenWindow;

const DESTINATION: &str = "org.gnome.Shell";
const PATH: &str = "/io/github/farhadzaidi/SnappyTest";
const INTERFACE: &str = "io.github.farhadzaidi.SnappyTest";
const CALL_TIMEOUT: Duration = Duration::from_secs(10);

/// Whether the suite runs in a GNOME on Wayland session.
pub fn is_session() -> bool {
    snappy_linux::session::current() == snappy_linux::session::Session::GnomeWayland
}

fn connection() -> Result<&'static Connection, String> {
    static CONNECTION: OnceLock<Result<Connection, String>> = OnceLock::new();
    CONNECTION
        .get_or_init(|| {
            zbus::blocking::connection::Builder::session()
                .and_then(|builder| builder.method_timeout(CALL_TIMEOUT).build())
                .map_err(|error| format!("the session bus can't be reached: {error}"))
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// Sends one request to the test helper and returns the `ok` value of its reply.
pub fn call(request: Value) -> Result<Value, String> {
    let reply = connection()?
        .call_method(
            Some(DESTINATION),
            PATH,
            Some(INTERFACE),
            "Call",
            &(request.to_string(),),
        )
        .map_err(|error| format!("the test helper didn't answer: {error}"))?;
    let (text,): (String,) = reply.body().deserialize().map_err(|error| error.to_string())?;
    let mut reply: serde_json::Map<String, Value> =
        serde_json::from_str(&text).map_err(|error| error.to_string())?;
    if let Some(error) = reply.remove("error") {
        return Err(format!("the test helper refused: {error}"));
    }
    Ok(reply.remove("ok").unwrap_or(Value::Null))
}

fn frame(value: Value) -> Result<Option<Rect>, String> {
    #[derive(Deserialize)]
    struct Frame {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    }
    let frame: Option<Frame> = serde_json::from_value(value).map_err(|error| error.to_string())?;
    Ok(frame.map(|frame| Rect::new(frame.x, frame.y, frame.width, frame.height)))
}

/// The frame of the window with this title, as Mutter knows it.
pub fn window_frame(title: &str) -> Result<Option<Rect>, String> {
    frame(call(json!({"op": "windowFrame", "title": title}))?)
}

/// Moves the window with this title so its frame starts at `origin`.
pub fn place_window(title: &str, origin: Point) -> Result<Rect, String> {
    frame(call(
        json!({"op": "placeWindow", "title": title, "x": origin.x, "y": origin.y}),
    )?)?
    .ok_or_else(|| format!("no window {title}"))
}

/// Activates the window, then clicks in its middle at a hand's pace, as a person would:
/// GNOME 49 lets a window that was only activated ignore drags on its title bar until it has
/// been clicked.
pub fn focus_window(title: &str) -> Result<(), String> {
    let frame = frame(call(json!({"op": "focusWindow", "title": title}))?)?
        .ok_or_else(|| format!("no window {title}"))?;
    let center = frame.center();
    call(json!({"op": "pointerMove", "x": center.x, "y": center.y}))?;
    pause(HOVER_MS);
    call(json!({"op": "button", "pressed": true}))?;
    pause(CLICK_MS);
    call(json!({"op": "button", "pressed": false}))?;
    let started = std::time::Instant::now();
    while !call(json!({"op": "focused", "title": title}))?
        .as_bool()
        .unwrap_or(false)
    {
        if started.elapsed() > FOCUS_TIMEOUT {
            return Err(format!("{title} never got focus"));
        }
        pause(POLL_MS);
    }
    Ok(())
}

/// Whether Snappy's item shows in the top bar.
pub fn indicator_shown() -> Result<bool, String> {
    Ok(call(json!({"op": "indicator"}))?.as_bool().unwrap_or(false))
}

/// Normal windows and Snappy's overlays on screen, front to back. GNOME Shell draws the
/// overlays itself on Snappy's behalf, so they count as the daemon's.
pub fn on_screen_windows(daemon: Option<i32>) -> Result<Vec<ScreenWindow>, String> {
    #[derive(Deserialize)]
    struct Shown {
        pid: i32,
        layer: i64,
        frame: Value,
        overlay: bool,
    }
    let shown: Vec<Shown> =
        serde_json::from_value(call(json!({"op": "screenWindows"}))?).map_err(|error| error.to_string())?;
    shown
        .into_iter()
        .map(|window| {
            Ok(ScreenWindow {
                pid: if window.overlay {
                    daemon.unwrap_or(window.pid)
                } else {
                    window.pid
                },
                layer: window.layer,
                frame: frame(window.frame)?.unwrap_or_default(),
            })
        })
        .collect()
}

/// Lets go of the button, every modifier, and Caps Lock, so a failed scenario leaves nothing
/// held.
pub fn release_all() {
    let _ = call(release_all_request());
}

/// The request that lets go of the button and every key the tests hold down.
fn release_all_request() -> Value {
    let keycodes: Vec<u8> = MODIFIER_ORDER
        .into_iter()
        .map(key_codes::modifier_keycode)
        .chain([CAPS_LOCK])
        .map(|keycode| keycode - key_codes::EVDEV_OFFSET)
        .collect();
    json!({"op": "releaseAll", "keycodes": keycodes})
}

pub fn screenshot(path: &std::path::Path) {
    let _ = call(json!({"op": "screenshot", "path": path.display().to_string()}));
}

/// Injects input through a Clutter virtual pointer and keyboard, which GNOME Shell handles
/// like hardware: window moves, shortcuts, and Snappy's extension all see it.
pub struct GnomeInput;

impl GnomeInput {
    pub fn new() -> Result<Self, String> {
        call(json!({"op": "ready"}))?;
        Ok(GnomeInput)
    }

    fn key_event(&self, keycode: u8, pressed: bool) -> Result<(), String> {
        let code = keycode
            .checked_sub(key_codes::EVDEV_OFFSET)
            .ok_or_else(|| format!("keycode {keycode} has no input code"))?;
        call(json!({"op": "key", "keycode": code, "pressed": pressed})).map(drop)
    }

    fn pointer_to(&self, at: Point) -> Result<(), String> {
        call(json!({"op": "pointerMove", "x": at.x, "y": at.y})).map(drop)
    }

    fn button(&self, pressed: bool) -> Result<(), String> {
        call(json!({"op": "button", "pressed": pressed})).map(drop)
    }
}

impl Input for GnomeInput {
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

    /// Rests on the spot first, as a hand does, so the app under the pointer knows it's there
    /// before the press.
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

    /// A mouse reports a press's first movement in small steps. GNOME draws the title bars of
    /// Xwayland windows with GTK, which starts a move once the pointer passes its drag threshold
    /// while still on the title bar; a first step that jumps far past it starts nothing. So the
    /// drag begins with one such step before covering the distance as other platforms' drags do.
    ///
    /// Every drag here moves a window, so one GNOME Shell never starts moving is an error.
    fn drag(&mut self, from: Point, to: Point, mods: Mods, steps: u32) -> Result<(), String> {
        self.modifiers(mods, true)?;
        self.pointer_down(from, mods)?;
        pause(DRAG_HOLD_MS);
        let distance = ((to.x - from.x).powi(2) + (to.y - from.y).powi(2)).sqrt();
        if distance > FIRST_STEP {
            self.pointer_drag(along(from, to, FIRST_STEP / distance), mods)?;
            pause(DRAG_STEP_MS);
        }
        // A hand follows the window: the rest of the way waits until GNOME Shell moves it.
        let started = std::time::Instant::now();
        while !call(json!({"op": "grabbing"}))?.as_bool().unwrap_or(false) {
            if started.elapsed() > MOVE_START_TIMEOUT {
                self.pointer_up(from, mods)?;
                self.modifiers(mods, false)?;
                return Err(format!(
                    "GNOME Shell didn't start moving the window pressed at {from:?}"
                ));
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

    /// The Caps Lock state of GNOME Shell's keymap, which every keyboard's light follows.
    fn caps_lock_on(&mut self) -> Result<bool, String> {
        Ok(call(json!({"op": "capsLockOn"}))?.as_bool().unwrap_or(false))
    }
}
