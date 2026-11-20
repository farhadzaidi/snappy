//! The fake GNOME Shell the tests share, answering as GNOME Shell, Mutter, and Snappy's
//! extension would, and the adapter running on it.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, mpsc};
use std::time::Duration;

use serde_json::{Value, json};
use snappy_gnome::protocol::{CALL_METHOD, EVENT_SIGNAL, EXTENSION_UUID, INTERFACE, OBJECT_PATH};
use snappy_gnome::{App, GnomePlatform, Stopper};
use snappy_platform::PlatformEvent;
use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Type;
use zbus::zvariant::{OwnedValue, Value as Variant};

pub const WAIT: Duration = Duration::from_secs(5);
/// A shortcut ID no test registers, sent to mark a point in the event stream.
pub const MARKER: u32 = u32::MAX;

/// What the fake extension holds and has been asked.
#[derive(Default)]
pub struct FakeState {
    /// Whether the extension's object answers, and in which protocol version.
    pub version: Option<u32>,
    /// `GetExtensionInfo`'s state; `None` for an extension GNOME Shell doesn't know.
    pub extension_state: Option<f64>,
    pub windows: Vec<Value>,
    /// Accelerators GNOME Shell already binds.
    pub taken: Vec<String>,
    pub park_refusal: Option<String>,
    /// Other extensions running, by ID.
    pub running: Vec<String>,
    /// How many times the session manager was asked to log out.
    pub log_outs: usize,
    /// The XKB options GNOME builds from the user's settings.
    pub keyboard_options: String,
    pub requests: Vec<Value>,
}

/// One fake GNOME Shell for every test in this file, on a bus of its own.
pub struct Fake {
    pub _daemon: Child,
    pub connection: Connection,
    pub state: Arc<Mutex<FakeState>>,
    /// Tests run one at a time; they share the fake.
    pub turn: Mutex<()>,
}

pub fn shared_fake() -> Option<&'static Fake> {
    static FAKE: OnceLock<Option<Fake>> = OnceLock::new();
    FAKE.get_or_init(Fake::start).as_ref()
}

macro_rules! fake {
    () => {
        match $crate::support::shared_fake() {
            Some(fake) => fake,
            None => {
                println!("skipped: dbus-daemon isn't installed");
                return;
            }
        }
    };
}
pub(crate) use fake;

impl Fake {
    pub fn start() -> Option<Fake> {
        Command::new("dbus-daemon").arg("--version").output().ok()?;
        let home = std::env::temp_dir().join(format!("snappy-fake-shell-{}", std::process::id()));
        let extension = home
            .join("data")
            .join(snappy_gnome::extension::EXTENSIONS_FOLDER)
            .join(EXTENSION_UUID);
        std::fs::create_dir_all(&extension).unwrap();
        // Installed already, so the adapter never turns it on in GNOME's settings.
        std::fs::write(extension.join(snappy_gnome::extension::METADATA), "{}").unwrap();
        // SAFETY of the test environment: set before any thread reads it.
        unsafe {
            std::env::set_var("HOME", &home);
            std::env::set_var("XDG_DATA_HOME", home.join("data"));
            std::env::set_var("XDG_CONFIG_HOME", home.join("config"));
            std::env::set_var("GSETTINGS_BACKEND", "memory");
            // No compositor: the keymap can't be read, so only the options are checked.
            std::env::set_var("WAYLAND_DISPLAY", home.join("no-compositor"));
        }

        // The bus and the home end when this process does, however it ends: the shell stops
        // the bus and removes the home once the pipe this process holds closes.
        let mut daemon = Command::new("sh")
            .args([
                "-c",
                r#"dbus-daemon --session --nofork --print-address & bus=$!; read _ignored; kill $bus; rm -r -- "$1""#,
                "fake-shell",
            ])
            .arg(&home)
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

    pub fn state(&self) -> MutexGuard<'_, FakeState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn reset(&self, version: Option<u32>, extension_state: Option<f64>) {
        *self.state() = FakeState {
            version,
            extension_state,
            ..FakeState::default()
        };
    }

    /// Requests of `op` so far.
    pub fn requests(&self, op: &str) -> Vec<Value> {
        self.state()
            .requests
            .iter()
            .filter(|request| request["op"] == op)
            .cloned()
            .collect()
    }

    pub fn emit(&self, event: Value) {
        self.connection
            .emit_signal(
                None::<()>,
                OBJECT_PATH,
                INTERFACE,
                EVENT_SIGNAL,
                &(event.to_string(),),
            )
            .unwrap();
    }

    pub fn extension_state_changed(&self) {
        self.extension_changed(EXTENSION_UUID);
    }

    pub fn extension_changed(&self, uuid: &str) {
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

pub fn owned(value: Variant<'_>) -> OwnedValue {
    value.try_to_owned().unwrap()
}

/// Answers every call as GNOME Shell, Mutter, and Snappy's extension would.
pub fn serve(connection: Connection, state: Arc<Mutex<FakeState>>) {
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
                (OBJECT_PATH, CALL_METHOD) => match state.version {
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

pub fn answer(state: &mut FakeState, request: &Value) -> Value {
    let window = |id: &Value| state.windows.iter().find(|window| window["id"] == *id).cloned();
    match request["op"].as_str().unwrap_or_default() {
        "windows" => json!({"ok": state.windows}),
        "window" => json!({"ok": window(&request["id"])}),
        "focused" => json!({"ok": state.windows.first()}),
        "setPosition" | "setSize" | "activate" | "unpark" | "lower" => match window(&request["id"]) {
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
        "keyboardOptions" => json!({"ok": state.keyboard_options}),
        _ => json!({"ok": null}),
    }
}

pub type Spec = (String, String, String, String);
pub type Properties = HashMap<String, OwnedValue>;
pub type Mode = (String, i32, i32, f64, f64, Vec<f64>, Properties);
pub type Monitor = (Spec, Vec<Mode>, Properties);
pub type LogicalMonitor = (i32, i32, f64, u32, bool, Vec<Spec>, Properties);

/// Mutter's `GetCurrentState` for one 1920×1080 monitor in the logical layout.
pub fn display_state() -> (u32, Vec<Monitor>, Vec<LogicalMonitor>, Properties) {
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

pub fn record(id: u64, pid: u32, x: f64) -> Value {
    json!({
        "id": id, "pid": pid, "appId": "org.example.Editor.desktop", "wmClass": "editor",
        "frame": {"x": x, "y": 100, "width": 600, "height": 400},
        "normal": true, "transient": false, "allowsMove": true, "allowsResize": true,
        "fullScreen": false, "maximized": false, "minimized": false,
        "onActiveWorkspace": true, "onAllWorkspaces": false, "parked": false,
    })
}

/// The adapter on the fake shell, with the events it reports.
pub struct Adapter {
    pub platform: GnomePlatform,
    pub events: Receiver<PlatformEvent>,
    pub stopper: Stopper,
    pub run: Option<std::thread::JoinHandle<()>>,
}

impl Adapter {
    pub fn start() -> Adapter {
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

    pub fn next(&self) -> PlatformEvent {
        self.events.recv_timeout(WAIT).expect("an event")
    }

    /// Everything reported before a marker the fake sends now: events are delivered in order,
    /// so when the marker arrives, nothing sent before it is still on its way.
    pub fn settle(&self, fake: &Fake) -> Vec<PlatformEvent> {
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
