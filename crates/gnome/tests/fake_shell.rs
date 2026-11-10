//! The GNOME adapter against a fake GNOME Shell on a private session bus: what it asks the
//! extension, how it reads the answers, and how it turns the extension's events into engine
//! events. Needs `dbus-daemon`; prints "skipped" without it.
//!
//! The tests share one private bus, its own temporary home, and an in-memory settings
//! backend, so nothing reaches the session the tests run in.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Duration;

use serde_json::{Value, json};
use snappy_core::geometry::{Point, Rect};
use snappy_core::keys::{KeyChord, Modifier, Mods};
use snappy_gnome::protocol::{EXTENSION_UUID, INTERFACE, OBJECT_PATH, PROTOCOL_VERSION};
use snappy_gnome::{App, GnomePlatform, Stopper};
use snappy_platform::{
    Caption, DesktopGuard, HotKeys, Input, LiveControl, Overlay, OverlayScene, OverlayZone, Platform,
    PlatformEvent, Remedy, Tray, TrayMenu, WindowId, WindowSystem,
};
use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Type;
use zbus::zvariant::{OwnedValue, Value as Variant};

const WAIT: Duration = Duration::from_secs(5);
/// A shortcut ID no test registers, sent to mark a point in the event stream.
const MARKER: u32 = u32::MAX;

/// What the fake extension holds and has been asked.
#[derive(Default)]
struct FakeState {
    /// Whether the extension's object answers, and in which protocol version.
    version: Option<u32>,
    /// `GetExtensionInfo`'s state; `None` for an extension GNOME Shell doesn't know.
    extension_state: Option<f64>,
    windows: Vec<Value>,
    /// Accelerators GNOME Shell already binds.
    taken: Vec<String>,
    park_refusal: Option<String>,
    /// Other extensions running, by ID.
    running: Vec<String>,
    /// How many times the session manager was asked to log out.
    log_outs: usize,
    requests: Vec<Value>,
}

/// One fake GNOME Shell for every test in this file, on a bus of its own.
struct Fake {
    _daemon: Child,
    connection: Connection,
    state: Arc<Mutex<FakeState>>,
    /// Tests run one at a time; they share the fake.
    turn: Mutex<()>,
}

fn fake() -> Option<&'static Fake> {
    static FAKE: OnceLock<Option<Fake>> = OnceLock::new();
    FAKE.get_or_init(Fake::start).as_ref()
}

macro_rules! fake {
    () => {
        match fake() {
            Some(fake) => fake,
            None => {
                println!("skipped: dbus-daemon isn't installed");
                return;
            }
        }
    };
}

impl Fake {
    fn start() -> Option<Fake> {
        let home = std::env::temp_dir().join(format!("snappy-fake-shell-{}", std::process::id()));
        let extension = home.join("data/gnome-shell/extensions").join(EXTENSION_UUID);
        std::fs::create_dir_all(&extension).unwrap();
        // Installed already, so the adapter never turns it on in GNOME's settings.
        std::fs::write(extension.join("metadata.json"), "{}").unwrap();
        // SAFETY of the test environment: set before any thread reads it.
        unsafe {
            std::env::set_var("HOME", &home);
            std::env::set_var("XDG_DATA_HOME", home.join("data"));
            std::env::set_var("XDG_CONFIG_HOME", home.join("config"));
            std::env::set_var("GSETTINGS_BACKEND", "memory");
        }

        Command::new("dbus-daemon").arg("--version").output().ok()?;
        // The bus ends when this process does, however it ends: the shell stops it once the
        // pipe this process holds closes.
        let mut daemon = Command::new("sh")
            .args([
                "-c",
                "dbus-daemon --session --nofork --print-address & bus=$!; read _ignored; kill $bus",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let mut address = String::new();
        BufReader::new(daemon.stdout.take()?)
            .read_line(&mut address)
            .ok()?;
        // SAFETY of the test environment: as above.
        unsafe { std::env::set_var("DBUS_SESSION_BUS_ADDRESS", address.trim()) };

        let connection = Connection::session().unwrap();
        for name in [
            "org.gnome.Shell",
            "org.gnome.Mutter.DisplayConfig",
            "org.gnome.SessionManager",
        ] {
            connection.request_name(name).unwrap();
        }
        let state = Arc::new(Mutex::new(FakeState::default()));
        serve(connection.clone(), Arc::clone(&state));
        Some(Fake {
            _daemon: daemon,
            connection,
            state,
            turn: Mutex::new(()),
        })
    }

    fn state(&self) -> MutexGuard<'_, FakeState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn reset(&self, version: Option<u32>, extension_state: Option<f64>) {
        *self.state() = FakeState {
            version,
            extension_state,
            ..FakeState::default()
        };
    }

    /// Requests of `op` so far.
    fn requests(&self, op: &str) -> Vec<Value> {
        self.state()
            .requests
            .iter()
            .filter(|request| request["op"] == op)
            .cloned()
            .collect()
    }

    fn emit(&self, event: Value) {
        self.connection
            .emit_signal(None::<()>, OBJECT_PATH, INTERFACE, "Event", &(event.to_string(),))
            .unwrap();
    }

    fn extension_state_changed(&self) {
        self.extension_changed(EXTENSION_UUID);
    }

    fn extension_changed(&self, uuid: &str) {
        let info: HashMap<String, OwnedValue> = HashMap::new();
        self.connection
            .emit_signal(
                None::<()>,
                "/org/gnome/Shell",
                "org.gnome.Shell.Extensions",
                "ExtensionStateChanged",
                &(uuid, info),
            )
            .unwrap();
    }
}

fn owned(value: Variant<'_>) -> OwnedValue {
    value.try_to_owned().unwrap()
}

/// Answers every call as GNOME Shell, Mutter, and Snappy's extension would.
fn serve(connection: Connection, state: Arc<Mutex<FakeState>>) {
    let calls = MessageIterator::from(&connection);
    std::thread::spawn(move || {
        for message in calls {
            let Ok(message) = message else { continue };
            let header = message.header();
            if header.message_type() != Type::MethodCall {
                continue;
            }
            let member = header
                .member()
                .map(|member| member.to_string())
                .unwrap_or_default();
            let path = header.path().map(|path| path.to_string()).unwrap_or_default();
            let mut state = state.lock().unwrap_or_else(PoisonError::into_inner);
            let replied = match (path.as_str(), member.as_str()) {
                (OBJECT_PATH, "Get") => match state.version {
                    Some(version) => connection.reply(&header, &owned(Variant::from(version))),
                    None => connection.reply_error(
                        &header,
                        "org.freedesktop.DBus.Error.UnknownMethod",
                        &"no object",
                    ),
                },
                (OBJECT_PATH, "Call") => match state.version {
                    Some(_) => {
                        let request: Option<Value> = message
                            .body()
                            .deserialize::<(String,)>()
                            .ok()
                            .and_then(|(text,)| serde_json::from_str(&text).ok());
                        let reply = match request {
                            Some(request) => {
                                let reply = answer(&mut state, &request);
                                state.requests.push(request);
                                reply
                            }
                            None => json!({"error": "not a request"}),
                        };
                        connection.reply(&header, &(reply.to_string(),))
                    }
                    None => connection.reply_error(
                        &header,
                        "org.freedesktop.DBus.Error.UnknownMethod",
                        &"no object",
                    ),
                },
                ("/org/gnome/Shell", "GetExtensionInfo") => {
                    let uuid = message
                        .body()
                        .deserialize::<(String,)>()
                        .map(|(uuid,)| uuid)
                        .unwrap_or_default();
                    let extension_state = if uuid == EXTENSION_UUID {
                        state.extension_state
                    } else {
                        state.running.contains(&uuid).then_some(1.0)
                    };
                    let mut info: HashMap<String, OwnedValue> = HashMap::new();
                    if let Some(extension_state) = extension_state {
                        info.insert("state".into(), owned(Variant::from(extension_state)));
                        info.insert("error".into(), owned(Variant::from("")));
                    }
                    connection.reply(&header, &info)
                }
                ("/org/gnome/SessionManager", "Logout") => {
                    state.log_outs += 1;
                    connection.reply(&header, &())
                }
                ("/org/gnome/Shell", "Get") => connection.reply(&header, &owned(Variant::from(true))),
                ("/org/gnome/Mutter/DisplayConfig", "GetCurrentState") => {
                    connection.reply(&header, &display_state())
                }
                _ => connection.reply_error(&header, "org.freedesktop.DBus.Error.UnknownMethod", &member),
            };
            // A reply that can't go out fails the test waiting for it, with its own message.
            if let Err(error) = replied {
                eprintln!("fake shell: couldn't reply to {member}: {error}");
            }
        }
    });
}

fn answer(state: &mut FakeState, request: &Value) -> Value {
    let window = |id: &Value| state.windows.iter().find(|window| window["id"] == *id).cloned();
    match request["op"].as_str().unwrap_or_default() {
        "windows" => json!({"ok": state.windows}),
        "window" => json!({"ok": window(&request["id"])}),
        "focused" => json!({"ok": state.windows.first()}),
        "setPosition" | "setSize" | "activate" | "unpark" => match window(&request["id"]) {
            Some(_) => json!({"ok": null}),
            None => json!({"error": "gone"}),
        },
        "register" => {
            let refused: Vec<Value> = request["keys"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|key| {
                    state
                        .taken
                        .iter()
                        .any(|taken| key["accelerator"] == *taken.as_str())
                })
                .map(|key| key["id"].clone())
                .collect();
            json!({"ok": refused})
        }
        "isFree" => json!({"ok": !state.taken.iter().any(|taken| request["accelerator"] == *taken.as_str())}),
        "park" => match window(&request["id"]) {
            Some(_) => json!({"ok": state.park_refusal}),
            None => json!({"error": "gone"}),
        },
        "monitors" => json!({"ok": [{
            "index": 0, "primary": true, "scale": 1,
            "frame": {"x": 0, "y": 0, "width": 1920, "height": 1080},
            "workArea": {"x": 0, "y": 32, "width": 1920, "height": 1048},
        }]}),
        "onBoundWorkspace" | "clearWorkspaces" => json!({"ok": true}),
        "pointer" => json!({"ok": {"x": 5, "y": 6}}),
        _ => json!({"ok": null}),
    }
}

type Spec = (String, String, String, String);
type Properties = HashMap<String, OwnedValue>;
type Mode = (String, i32, i32, f64, f64, Vec<f64>, Properties);
type Monitor = (Spec, Vec<Mode>, Properties);
type LogicalMonitor = (i32, i32, f64, u32, bool, Vec<Spec>, Properties);

/// Mutter's `GetCurrentState` for one 1920×1080 monitor in the logical layout.
fn display_state() -> (u32, Vec<Monitor>, Vec<LogicalMonitor>, Properties) {
    let spec: Spec = ("DP-1".into(), "DEL".into(), "U2720Q".into(), "1".into());
    let current = HashMap::from([("is-current".to_owned(), owned(Variant::from(true)))]);
    (
        1,
        vec![(
            spec.clone(),
            vec![("mode".into(), 1920, 1080, 60.0, 1.0, vec![1.0], current)],
            HashMap::from([("display-name".to_owned(), owned(Variant::from("Dell 27\"")))]),
        )],
        vec![(0, 0, 1.0, 0, true, vec![spec], HashMap::new())],
        HashMap::from([("layout-mode".to_owned(), owned(Variant::from(1u32)))]),
    )
}

fn record(id: u64, pid: u32, x: f64) -> Value {
    json!({
        "id": id, "pid": pid, "appId": "org.example.Editor.desktop", "wmClass": "editor",
        "frame": {"x": x, "y": 100, "width": 600, "height": 400},
        "normal": true, "transient": false, "allowsMove": true, "allowsResize": true,
        "fullScreen": false, "maximized": false, "minimized": false,
        "onActiveWorkspace": true, "onAllWorkspaces": false, "parked": false,
    })
}

/// The adapter on the fake shell, with the events it reports.
struct Adapter {
    platform: GnomePlatform,
    events: Receiver<PlatformEvent>,
    stopper: Stopper,
    run: Option<std::thread::JoinHandle<()>>,
}

impl Adapter {
    fn start() -> Adapter {
        let (sender, events) = mpsc::channel();
        let app = App::new(sender).unwrap();
        let platform = app.platform();
        let stopper = app.stopper();
        let run = std::thread::spawn(move || app.run().unwrap());
        Adapter {
            platform,
            events,
            stopper,
            run: Some(run),
        }
    }

    fn next(&self) -> PlatformEvent {
        self.events.recv_timeout(WAIT).expect("an event")
    }

    /// Everything reported before a marker the fake sends now: events are delivered in order,
    /// so when the marker arrives, nothing sent before it is still on its way.
    fn settle(&self, fake: &Fake) -> Vec<PlatformEvent> {
        fake.emit(json!({"type": "hotkey", "id": MARKER}));
        let mut seen = Vec::new();
        loop {
            match self.next() {
                PlatformEvent::HotKey(MARKER) => return seen,
                event => seen.push(event),
            }
        }
    }
}

/// Waits for the adapter to finish letting go, so the next test's requests are its own.
impl Drop for Adapter {
    fn drop(&mut self) {
        self.stopper.stop();
        if let Some(run) = self.run.take() {
            let _ = run.join();
        }
    }
}

#[test]
fn an_installed_extension_gnome_hasnt_loaded_asks_to_log_in_again() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(None, None);
    let mut adapter = Adapter::start();
    let LiveControl::Unavailable { reason, remedy } = adapter.platform.live_control() else {
        panic!("the extension isn't answering");
    };
    assert!(reason.starts_with("One last step: log out of "), "{reason}");
    assert_eq!(remedy, Some(Remedy::LogOut));
    adapter.platform.apply_remedy(Remedy::LogOut).unwrap();
    assert_eq!(fake.state().log_outs, 1, "GNOME's log-out was asked for");

    fake.reset(None, Some(2.0));
    fake.extension_state_changed();
    assert_eq!(adapter.next(), PlatformEvent::LiveControlChanged);
    let LiveControl::Unavailable { reason, .. } = adapter.platform.live_control() else {
        panic!("the extension is off");
    };
    assert!(reason.contains("turned off"), "{reason}");
}

#[test]
fn the_extension_starting_makes_snappy_live_and_gets_back_what_it_held() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    let mut adapter = Adapter::start();
    assert_eq!(adapter.platform.live_control(), LiveControl::Available);
    adapter.platform.set_drag_modifier(Modifier::Shift);
    adapter.platform.set_press_monitoring(true);
    assert!(
        adapter
            .platform
            .register(&[(3, KeyChord::snappy("Digit1", []).combo())])
            .is_empty()
    );
    adapter.platform.update(&TrayMenu::default());
    fake.state().requests.clear();

    fake.emit(json!({"type": "ready"}));
    assert_eq!(adapter.next(), PlatformEvent::LiveControlChanged);
    assert_eq!(adapter.platform.live_control(), LiveControl::Available);
    let monitoring = fake.requests("monitoring");
    assert_eq!(monitoring.last().unwrap()["press"], true);
    assert_eq!(
        monitoring.last().unwrap()["follow"],
        1,
        "Shift keeps dragged windows under the pointer"
    );
    let registered = fake.requests("register");
    assert_eq!(
        registered.last().unwrap()["keys"],
        json!([{"id": 3, "accelerator": "<Control><Alt><Super>0x0a"}])
    );
    assert_eq!(fake.requests("menu").len(), 1);

    fake.reset(Some(PROTOCOL_VERSION + 1), Some(1.0));
    fake.extension_state_changed();
    assert_eq!(adapter.next(), PlatformEvent::LiveControlChanged);
    let LiveControl::Unavailable { reason, remedy } = adapter.platform.live_control() else {
        panic!("another version answers");
    };
    assert!(reason.starts_with("Snappy was updated."), "{reason}");
    assert_eq!(remedy, Some(Remedy::LogOut));
}

#[test]
fn the_extensions_events_arrive_in_order_as_engine_events() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    fake.state().windows = vec![record(1, 100, 0.0)];
    let mut adapter = Adapter::start();
    adapter.platform.update(&TrayMenu::default());
    adapter.settle(fake);
    let generation = fake.requests("menu")[0]["generation"].clone();

    for event in [
        json!({"type": "pointerDown", "x": 10, "y": 20, "mods": 1, "window": 1}),
        json!({"type": "pointerDragged", "x": 30, "y": 40, "mods": 1 | 4}),
        json!({"type": "modifiers", "x": 30, "y": 40, "mods": 64}),
        json!({"type": "pointerUp", "x": 50, "y": 60, "mods": 0}),
        json!({"type": "hotkey", "id": 4}),
        json!({"type": "windowAdded", "id": 2, "pid": 200}),
        json!({"type": "windowMoved", "id": 2}),
        json!({"type": "focus", "pid": 200}),
        json!({"type": "windowRemoved", "id": 2}),
        json!({"type": "workspace"}),
        json!({"type": "displays"}),
        json!({"type": "somethingFromANewerExtension"}),
        json!({"type": "menu", "item": 0, "generation": generation}),
    ] {
        fake.emit(event);
    }
    let shift = Mods::from_iter([Modifier::Shift]);
    let expected = vec![
        PlatformEvent::PointerDown {
            at: Point::new(10.0, 20.0),
            mods: shift,
            window: Some(WindowId(1)),
        },
        PlatformEvent::PointerDragged {
            at: Point::new(30.0, 40.0),
            mods: Mods::from_iter([Modifier::Shift, Modifier::Ctrl]),
        },
        PlatformEvent::ModifiersChanged {
            at: Point::new(30.0, 40.0),
            mods: Mods::from_iter([Modifier::Meta]),
        },
        PlatformEvent::PointerUp {
            at: Point::new(50.0, 60.0),
            mods: Mods::NONE,
        },
        PlatformEvent::HotKey(4),
        PlatformEvent::AppLaunched { pid: 200 },
        PlatformEvent::WindowCreated { pid: 200 },
        PlatformEvent::WindowMoved(WindowId(2)),
        PlatformEvent::FocusChanged {
            pid: 200,
            by_activation: true,
        },
        PlatformEvent::WindowDestroyed(WindowId(2)),
        PlatformEvent::AppTerminated { pid: 200 },
        PlatformEvent::NativeDesktopChanged,
        PlatformEvent::DisplaysChanged,
        PlatformEvent::OpenSettingsRequested,
    ];
    let seen: Vec<PlatformEvent> = (0..expected.len()).map(|_| adapter.next()).collect();
    assert_eq!(seen, expected);
}

#[test]
fn shortcuts_sharing_an_accelerator_share_a_grab_and_refusals_name_every_id() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    fake.state().taken = vec!["<Super>0x26".into()];
    let mut adapter = Adapter::start();
    let one = KeyChord::snappy("Digit1", []).combo();
    let super_a = KeyChord::new("KeyA", [Modifier::Meta]).combo();
    let refused = adapter.platform.register(&[
        (1, one.clone()),
        (2, one.clone()),
        (3, super_a.clone()),
        (4, super_a.clone()),
    ]);
    assert_eq!(refused, vec![3, 4]);
    let keys = fake.requests("register").last().unwrap()["keys"].clone();
    assert_eq!(
        keys.as_array().unwrap().len(),
        2,
        "one grab per accelerator: {keys}"
    );

    fake.state().requests.clear();
    assert!(
        adapter.platform.is_available(&one),
        "Snappy's own shortcut is free to it"
    );
    assert!(
        fake.requests("isFree").is_empty(),
        "no probe for its own shortcut"
    );
    assert!(!adapter.platform.is_available(&super_a));
}

#[test]
fn identical_overlay_scenes_cross_the_bus_once() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    let mut adapter = Adapter::start();
    let displays = adapter.platform.displays();
    let display = &displays[0];
    let scene = |caption| OverlayScene {
        work_area: display.work_area,
        zones: vec![OverlayZone {
            frame: Rect::new(12.0, 44.0, 942.0, 1024.0),
            number: 1,
            highlighted: true,
            caption,
            swap_source: false,
        }],
    };
    for _ in 0..5 {
        adapter.platform.show(&display.id, &scene(None));
    }
    adapter.platform.show(&display.id, &scene(Some(Caption::Stack)));
    adapter.platform.hide();
    adapter.platform.hide();
    assert_eq!(fake.requests("overlayShow").len(), 2);
    assert_eq!(fake.requests("overlayHide").len(), 1);
    let shown = &fake.requests("overlayShow")[1]["scene"];
    assert_eq!(shown["zones"][0]["caption"]["text"], "Stack");
    assert_eq!(shown["workArea"]["y"], 32.0);
}

#[test]
fn displays_join_mutters_names_with_the_extensions_work_areas() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    let mut adapter = Adapter::start();
    let displays = adapter.platform.displays();
    assert_eq!(displays.len(), 1);
    assert_eq!(displays[0].name, "Dell 27\"");
    assert!(displays[0].id.starts_with("DP-1-"));
    assert_eq!(displays[0].work_area, Rect::new(0.0, 32.0, 1920.0, 1048.0));
}

#[test]
fn windows_are_followed_once_and_refusals_read_as_messages() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    fake.state().windows = vec![record(7, 0, 0.0), record(8, 0, 700.0)];
    let mut adapter = Adapter::start();
    assert_eq!(
        adapter.platform.window_at(Point::new(800.0, 200.0)).unwrap().id,
        WindowId(8)
    );
    adapter.platform.visible_windows(None);
    adapter.platform.visible_windows(None);
    let followed: Vec<Value> = fake
        .requests("follow")
        .iter()
        .map(|request| request["ids"].clone())
        .collect();
    assert_eq!(
        followed,
        vec![json!([8]), json!([7])],
        "each window is followed once"
    );

    let error = adapter
        .platform
        .set_position(WindowId(99), Point::new(0.0, 0.0))
        .unwrap_err();
    assert!(error.to_string().contains("no longer available"), "{error}");
    let displays = adapter.platform.displays();
    adapter.platform.bind(&displays);
    fake.state().park_refusal = Some("allWorkspaces".into());
    let error = adapter
        .platform
        .hide_for_workspace(WindowId(7), Point::default())
        .unwrap_err();
    assert!(error.to_string().contains("every workspace"), "{error}");
}

#[test]
fn showing_a_parked_window_puts_it_back_in_place_before_it_returns() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    let mut parked = record(5, 0, 300.0);
    parked["parked"] = json!(true);
    fake.state().windows = vec![parked];
    let mut adapter = Adapter::start();
    let origin = Point::new(300.0, 100.0);
    adapter.platform.show_for_workspace(WindowId(5), origin).unwrap();
    let order: Vec<String> = fake
        .state()
        .requests
        .iter()
        .filter_map(|request| request["op"].as_str().map(str::to_owned))
        .filter(|op| op == "setPosition" || op == "unpark")
        .collect();
    assert_eq!(order, vec!["unpark"], "already in place, so it only comes back");

    fake.state().requests.clear();
    let elsewhere = Point::new(40.0, 50.0);
    adapter
        .platform
        .show_for_workspace(WindowId(5), elsewhere)
        .unwrap();
    let order: Vec<String> = fake
        .state()
        .requests
        .iter()
        .filter_map(|request| request["op"].as_str().map(str::to_owned))
        .filter(|op| op == "setPosition" || op == "unpark")
        .collect();
    assert_eq!(
        order,
        vec!["setPosition", "unpark"],
        "moved while hidden, then shown"
    );
}

#[test]
fn a_running_tiling_assistant_is_noted_and_its_changes_are_reported() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    let mut adapter = Adapter::start();
    assert!(adapter.platform.notes().is_empty());

    fake.state().running = vec!["tiling-assistant@ubuntu.com".into()];
    fake.extension_changed("tiling-assistant@ubuntu.com");
    assert_eq!(adapter.next(), PlatformEvent::NotesChanged);
    let notes = adapter.platform.notes();
    assert_eq!(notes.len(), 1);
    assert!(notes[0].contains("Enhanced Tiling"), "{notes:?}");

    fake.extension_changed("someone-else@example.com");
    assert!(adapter.settle(fake).is_empty(), "other extensions don't matter");
}

#[test]
fn changing_the_drag_key_tells_the_extension_once() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    let mut adapter = Adapter::start();
    adapter.platform.set_press_monitoring(true);
    fake.state().requests.clear();
    adapter.platform.set_drag_modifier(Modifier::Ctrl);
    adapter.platform.set_drag_modifier(Modifier::Ctrl);
    let follows: Vec<Value> = fake
        .requests("monitoring")
        .iter()
        .map(|request| request["follow"].clone())
        .collect();
    assert_eq!(follows, vec![json!(4)], "Ctrl's mask, sent once");
}

#[test]
fn shortcuts_registered_while_gnome_is_away_are_kept_and_grabbed_when_it_returns() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(None, None);
    let mut adapter = Adapter::start();
    let one = KeyChord::snappy("Digit1", []).combo();
    assert!(
        adapter.platform.register(&[(1, one.clone())]).is_empty(),
        "an unanswered request says nothing about the chord"
    );
    assert!(fake.requests("register").is_empty());

    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    fake.emit(json!({"type": "ready"}));
    assert_eq!(adapter.next(), PlatformEvent::LiveControlChanged);
    let registered = fake.requests("register");
    assert_eq!(
        registered.last().unwrap()["keys"],
        json!([{"id": 1, "accelerator": "<Control><Alt><Super>0x0a"}])
    );
}

#[test]
fn windows_that_came_and_went_while_the_extension_was_away_are_reported() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    fake.state().windows = vec![record(1, 100, 0.0), record(2, 200, 700.0)];
    let adapter = Adapter::start();
    assert!(
        adapter.settle(fake).is_empty(),
        "the first list is taken as it is"
    );

    fake.state().windows = vec![record(1, 100, 0.0), record(3, 300, 700.0)];
    fake.emit(json!({"type": "ready"}));
    let events = adapter.settle(fake);
    assert!(
        events.contains(&PlatformEvent::WindowDestroyed(WindowId(2))),
        "{events:?}"
    );
    assert!(
        events.contains(&PlatformEvent::AppTerminated { pid: 200 }),
        "{events:?}"
    );
    assert!(
        events.contains(&PlatformEvent::AppLaunched { pid: 300 }),
        "{events:?}"
    );
    assert!(
        events.contains(&PlatformEvent::WindowCreated { pid: 300 }),
        "{events:?}"
    );
    assert_eq!(events.last(), Some(&PlatformEvent::LiveControlChanged));
}

#[test]
fn a_choice_from_a_replaced_menu_is_ignored() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    let mut adapter = Adapter::start();
    adapter.platform.update(&TrayMenu::default());
    let first = fake.requests("menu")[0]["generation"].clone();
    adapter.platform.update(&TrayMenu {
        live: true,
        ..TrayMenu::default()
    });
    let second = fake.requests("menu")[1]["generation"].clone();
    assert_ne!(first, second);

    fake.emit(json!({"type": "menu", "item": 0, "generation": first}));
    assert!(
        adapter.settle(fake).is_empty(),
        "the old menu's choice does nothing"
    );
    fake.emit(json!({"type": "menu", "item": 0, "generation": second}));
    assert_eq!(adapter.settle(fake), vec![PlatformEvent::OpenSettingsRequested]);
}
