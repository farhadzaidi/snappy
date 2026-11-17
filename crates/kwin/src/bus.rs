//! The session bus: the daemon's own name, which the script calls with events and to collect
//! requests; the requests waiting for it; and calls to KWin and KDE's shortcut service.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use snappy_linux::dbus::BusThread;
pub use snappy_linux::dbus::CallError;
use zbus::blocking::{Connection, MessageIterator};
use zbus::fdo::RequestNameFlags;
use zbus::message::Type;
use zbus::{MatchRule, Message};

use crate::error::KWinError;
use crate::protocol::{
    ANSWER_METHOD, Answer, BUS_NAME, DOORBELL, EVENT_METHOD, Event, INTERFACE, Numbered, OBJECT_PATH,
    Request, TAKE_METHOD,
};
use snappy_linux::journal;
use snappy_linux::names::{DBUS_NAME, DBUS_PATH};

/// How long a request may take before the engine gives up on it. KWin answers from its main
/// loop, normally within a frame.
const CALL_TIMEOUT: Duration = Duration::from_secs(2);

pub const KWIN_NAME: &str = "org.kde.KWin";
pub const SCRIPTING_PATH: &str = "/Scripting";
pub const SCRIPTING_INTERFACE: &str = "org.kde.kwin.Scripting";
pub const SCRIPT_INTERFACE: &str = "org.kde.kwin.Script";
pub const GLOBAL_ACCEL_NAME: &str = "org.kde.kglobalaccel";
const GLOBAL_ACCEL_PATH: &str = "/kglobalaccel";
const GLOBAL_ACCEL_INTERFACE: &str = "org.kde.KGlobalAccel";
pub const COMPONENT_INTERFACE: &str = "org.kde.kglobalaccel.Component";
/// KWin's own shortcut component, where the script's doorbell lives.
const KWIN_COMPONENT: &str = "kwin";
const KWIN_COMPONENT_PATH: &str = "/component/kwin";
/// KConfig's change notifications: a signal on the file's name as a path, with the changed
/// keys by group (`a{saay}`), which KConfigWatcher turns into a reload.
pub const KXKBRC_PATH: &str = "/kxkbrc";
pub const CONFIG_NOTIFY_INTERFACE: &str = "org.kde.kconfig.notify";
pub const CONFIG_CHANGED: &str = "ConfigChanged";
pub const SCREEN_SAVER_INTERFACE: &str = "org.freedesktop.ScreenSaver";

/// Where KWin offers the script it numbered `id`.
pub fn script_path(id: i32) -> String {
    format!("{SCRIPTING_PATH}/Script{id}")
}

/// Who answers requests, as the daemon's output names it.
const REPLIER: &str = "KWin";

/// What the serving thread hands the event loop.
pub enum Served {
    Event(Event),
    /// A call that didn't come from KWin, which is refused, or an event that didn't parse,
    /// which is logged; either is dropped.
    Ignored,
}

/// The requests waiting for the script, and the answers waiting for whoever asked.
#[derive(Default)]
struct Exchange {
    state: Mutex<ExchangeState>,
    answered: Condvar,
}

#[derive(Default)]
struct ExchangeState {
    next_seq: u64,
    waiting: VecDeque<String>,
    /// The requests someone still waits on; a late answer to one that timed out is dropped.
    pending: HashSet<u64>,
    answers: HashMap<u64, Answer>,
    /// The script was rung and hasn't yet been told there is nothing more, so requests
    /// queued meanwhile reach it without another ring.
    collecting: bool,
}

impl Exchange {
    fn state(&self) -> MutexGuard<'_, ExchangeState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Hands over every waiting request; an empty list ends the script's collecting.
    fn take(&self) -> String {
        let mut state = self.state();
        if state.waiting.is_empty() {
            state.collecting = false;
            return "[]".to_owned();
        }
        let requests: Vec<String> = state.waiting.drain(..).collect();
        format!("[{}]", requests.join(","))
    }

    fn answer(&self, answers: Vec<Answer>) {
        let mut state = self.state();
        for answer in answers {
            if state.pending.contains(&answer.seq) {
                state.answers.insert(answer.seq, answer);
            }
        }
        self.answered.notify_all();
    }
}

/// The session bus connection the adapter shares between its threads. Calls to other
/// services run on the bus thread.
#[derive(Clone)]
pub struct Bus {
    conn: Connection,
    thread: BusThread,
    exchange: Arc<Exchange>,
}

impl Bus {
    /// Connects and takes the daemon's name, so the script can reach it.
    pub fn session() -> Result<Bus, KWinError> {
        let conn = zbus::blocking::connection::Builder::session()?
            .method_timeout(CALL_TIMEOUT)
            .build()?;
        Bus::on(conn)
    }

    pub fn on(conn: Connection) -> Result<Bus, KWinError> {
        let reply = conn.request_name_with_flags(BUS_NAME, RequestNameFlags::DoNotQueue.into())?;
        if reply != zbus::fdo::RequestNameReply::PrimaryOwner {
            return Err(KWinError::NameTaken);
        }
        let thread = BusThread::spawn(&conn, "snappy-kwin-bus")?;
        Ok(Bus {
            conn,
            thread,
            exchange: Arc::default(),
        })
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Gives the daemon's name back, as quitting does.
    pub fn release_name(&self) {
        let _ = self.conn.release_name(BUS_NAME);
    }

    /// Calls a method of another service and reads its reply's body.
    pub fn call_method<B, R>(
        &self,
        destination: &'static str,
        path: String,
        interface: &'static str,
        method: &'static str,
        body: B,
    ) -> Result<R, CallError>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType + Send + 'static,
        R: for<'d> zbus::zvariant::DynamicDeserialize<'d> + Send + 'static,
    {
        self.thread.call(move |conn| {
            let reply = conn
                .call_method(Some(destination), path.as_str(), Some(interface), method, &body)
                .map_err(|error| CallError::Unreachable(error.to_string()))?;
            reply
                .body()
                .deserialize::<R>()
                .map_err(|error| CallError::Malformed(error.to_string()).logged(REPLIER))
        })
    }

    /// Sends a request to the script and reads the `ok` value of its answer.
    pub fn call<T: DeserializeOwned>(&self, request: &Request) -> Result<T, CallError> {
        let (seq, ring) = {
            let mut state = self.exchange.state();
            let seq = state.next_seq;
            state.next_seq += 1;
            let json = serde_json::to_string(&Numbered { seq, request })
                .map_err(|error| CallError::Malformed(error.to_string()).logged(REPLIER))?;
            state.waiting.push_back(json);
            state.pending.insert(seq);
            (seq, !std::mem::replace(&mut state.collecting, true))
        };
        if ring {
            self.ring();
        }
        let answer = self.wait_for(seq)?;
        if let Some(error) = answer.error {
            return Err(CallError::Refused(error));
        }
        serde_json::from_value(answer.ok)
            .map_err(|error| CallError::Malformed(error.to_string()).logged(REPLIER))
    }

    /// A request whose answer carries nothing.
    pub fn send(&self, request: &Request) -> Result<(), CallError> {
        self.call::<serde_json::Value>(request).map(drop)
    }

    fn wait_for(&self, seq: u64) -> Result<Answer, CallError> {
        let deadline = Instant::now() + CALL_TIMEOUT;
        let mut state = self.exchange.state();
        loop {
            if let Some(answer) = state.answers.remove(&seq) {
                state.pending.remove(&seq);
                return Ok(answer);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                // The script isn't there or lost the ring: forget the request, and ring again
                // for the next one.
                let tag = format!("{{\"seq\":{seq},");
                state.waiting.retain(|json| !json.starts_with(&tag));
                state.pending.remove(&seq);
                state.collecting = false;
                return Err(CallError::Unreachable("KWin's script didn't answer".to_owned()));
            }
            state = self
                .exchange
                .answered
                .wait_timeout(state, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    /// Triggers the script's doorbell through KDE's shortcut service, without waiting.
    fn ring(&self) {
        self.thread.post(|conn| {
            let rung = conn.call_method(
                Some(GLOBAL_ACCEL_NAME),
                KWIN_COMPONENT_PATH,
                Some(COMPONENT_INTERFACE),
                "invokeShortcut",
                &(DOORBELL,),
            );
            if let Err(error) = rung {
                journal::warn(format_args!("couldn't reach KWin's script: {error}"));
            }
        });
    }

    /// Answers the script's calls until the bus closes, passing each event to `deliver` in
    /// the order the script sent them. Only KWin may call.
    pub fn serve(&self, deliver: impl Fn(Served) -> bool + Send + 'static) -> Result<(), KWinError> {
        let rule = MatchRule::builder()
            .msg_type(Type::MethodCall)
            .path(OBJECT_PATH)?
            .build();
        let calls = MessageIterator::for_match_rule(rule, &self.conn, None)?;
        let bus = self.clone();
        std::thread::Builder::new()
            .name("snappy-kwin-calls".into())
            .spawn(move || {
                let mut kwin: Option<String> = None;
                for message in calls {
                    let Ok(message) = message else { break };
                    if !bus.sent_by_kwin(&message, &mut kwin) {
                        bus.refuse(&message, "org.freedesktop.DBus.Error.AccessDenied");
                        deliver(Served::Ignored);
                        continue;
                    }
                    let served = bus.handle(&message);
                    if let Some(served) = served
                        && !deliver(served)
                    {
                        return;
                    }
                }
            })?;
        Ok(())
    }

    /// Whether KWin sent the call; KWin's unique name is looked up again when it changes.
    fn sent_by_kwin(&self, message: &Message, kwin: &mut Option<String>) -> bool {
        let header = message.header();
        let Some(sender) = header.sender().map(ToString::to_string) else {
            return false;
        };
        if kwin.as_deref() != Some(sender.as_str()) {
            *kwin = self.owner(KWIN_NAME);
        }
        kwin.as_deref() == Some(sender.as_str())
    }

    fn owner(&self, name: &str) -> Option<String> {
        let reply = self
            .conn
            .call_method(
                Some(DBUS_NAME),
                DBUS_PATH,
                Some(DBUS_NAME),
                "GetNameOwner",
                &(name,),
            )
            .ok()?;
        reply.body().deserialize::<String>().ok()
    }

    fn refuse(&self, message: &Message, error: &str) {
        let header = message.header();
        let _ = self
            .conn
            .reply_error(&header, error, &"Only KWin may call Snappy here.");
    }

    fn handle(&self, message: &Message) -> Option<Served> {
        let header = message.header();
        if header.interface().map(|interface| interface.as_str()) != Some(INTERFACE) {
            let _ = self
                .conn
                .reply_error(&header, "org.freedesktop.DBus.Error.UnknownInterface", &"");
            return None;
        }
        let member = header
            .member()
            .map(|member| member.as_str().to_owned())
            .unwrap_or_default();
        match member.as_str() {
            EVENT_METHOD => {
                let _ = self.conn.reply(&header, &());
                let event = message
                    .body()
                    .deserialize::<String>()
                    .ok()
                    .and_then(|json| serde_json::from_str::<Event>(&json).ok());
                Some(event.map_or_else(
                    || {
                        journal::warn("KWin's script sent an event Snappy can't read");
                        Served::Ignored
                    },
                    Served::Event,
                ))
            }
            TAKE_METHOD => {
                let _ = self.conn.reply(&header, &(self.exchange.take(),));
                None
            }
            ANSWER_METHOD => {
                match message
                    .body()
                    .deserialize::<String>()
                    .ok()
                    .and_then(|json| serde_json::from_str::<Vec<Answer>>(&json).ok())
                {
                    Some(answers) => self.exchange.answer(answers),
                    None => journal::warn("KWin's script sent answers Snappy can't read"),
                }
                let _ = self.conn.reply(&header, &(self.exchange.take(),));
                None
            }
            _ => {
                let _ = self
                    .conn
                    .reply_error(&header, "org.freedesktop.DBus.Error.UnknownMethod", &member);
                None
            }
        }
    }

    pub fn kwin_running(&self) -> bool {
        self.call_method::<_, bool>(
            DBUS_NAME,
            DBUS_PATH.to_owned(),
            DBUS_NAME,
            "NameHasOwner",
            (KWIN_NAME,),
        )
        .unwrap_or(false)
    }

    pub fn is_script_loaded(&self, name: &'static str) -> Result<bool, CallError> {
        self.call_method(
            KWIN_NAME,
            SCRIPTING_PATH.to_owned(),
            SCRIPTING_INTERFACE,
            "isScriptLoaded",
            (name,),
        )
    }

    pub fn unload_script(&self, name: &'static str) -> Result<bool, CallError> {
        self.call_method(
            KWIN_NAME,
            SCRIPTING_PATH.to_owned(),
            SCRIPTING_INTERFACE,
            "unloadScript",
            (name,),
        )
    }

    /// Loads a QML script and starts it; KWin numbers the scripts it loads.
    pub fn run_script(&self, main: String, name: &'static str) -> Result<(), CallError> {
        let id: i32 = self.call_method(
            KWIN_NAME,
            SCRIPTING_PATH.to_owned(),
            SCRIPTING_INTERFACE,
            "loadDeclarativeScript",
            (main, name),
        )?;
        if id < 0 {
            return Err(CallError::Refused("KWin didn't load the script".to_owned()));
        }
        self.call_method::<_, ()>(KWIN_NAME, script_path(id), SCRIPT_INTERFACE, "run", ())
    }

    /// Registers a shortcut action with KDE's shortcut service: component, action, and their
    /// names as System Settings shows them.
    pub fn register_action(&self, action: [String; 4]) -> Result<(), CallError> {
        self.call_method(
            GLOBAL_ACCEL_NAME,
            GLOBAL_ACCEL_PATH.to_owned(),
            GLOBAL_ACCEL_INTERFACE,
            "doRegister",
            (action.to_vec(),),
        )
    }

    /// Sets an action's keys and returns the keys it got: none when another program holds
    /// them. `flags` are KGlobalAccel's.
    pub fn set_action_keys(
        &self,
        action: [String; 4],
        keys: Vec<i32>,
        flags: u32,
    ) -> Result<Vec<i32>, CallError> {
        let given: Vec<(Vec<i32>,)> = self.call_method(
            GLOBAL_ACCEL_NAME,
            GLOBAL_ACCEL_PATH.to_owned(),
            GLOBAL_ACCEL_INTERFACE,
            "setShortcutKeys",
            (action.to_vec(), vec![(keys,)], flags),
        )?;
        Ok(given
            .into_iter()
            .flat_map(|(keys,)| keys)
            .filter(|key| *key != 0)
            .collect())
    }

    pub fn unregister_action(&self, component: &'static str, action: String) -> Result<bool, CallError> {
        self.call_method(
            GLOBAL_ACCEL_NAME,
            GLOBAL_ACCEL_PATH.to_owned(),
            GLOBAL_ACCEL_INTERFACE,
            "unregister",
            (component, action),
        )
    }

    /// KConfig's change notification for `kxkbrc`'s `[Layout]` group, naming `keys`, as
    /// System Settings sends it when it saves the keyboard settings.
    pub fn notify_config_changed(&self, keys: &'static [&'static str]) -> Result<(), CallError> {
        self.thread.call(move |conn| notify_config_changed(conn, keys))
    }

    /// Every action holding exactly this key sequence, as `(component, action)`.
    pub fn key_holders(&self, keys: Vec<i32>) -> Result<Vec<(String, String)>, CallError> {
        /// KGlobalShortcutInfo: action, its name, component, its name, context, its name,
        /// keys, and default keys.
        type Info = (String, String, String, String, String, String, Vec<i32>, Vec<i32>);
        /// KGlobalAccel's search for the same sequence, rather than ones it shadows.
        const EQUAL: i32 = 0;
        let found: Vec<Info> = self.call_method(
            GLOBAL_ACCEL_NAME,
            GLOBAL_ACCEL_PATH.to_owned(),
            GLOBAL_ACCEL_INTERFACE,
            "globalShortcutsByKey",
            ((keys,), (EQUAL,)),
        )?;
        Ok(found
            .into_iter()
            .map(|(action, _, component, ..)| (component, action))
            .collect())
    }
}

/// KConfig's change notification for `kxkbrc`'s `[Layout]` group on `conn`, naming `keys`.
pub fn notify_config_changed(conn: &Connection, keys: &[&str]) -> Result<(), CallError> {
    let changed: HashMap<String, Vec<Vec<u8>>> = HashMap::from([(
        crate::caps_lock::CHANGED_GROUP.to_owned(),
        keys.iter().map(|key| key.as_bytes().to_vec()).collect(),
    )]);
    conn.emit_signal(
        None::<&str>,
        KXKBRC_PATH,
        CONFIG_NOTIFY_INTERFACE,
        CONFIG_CHANGED,
        &(changed,),
    )
    .map_err(|error| CallError::Unreachable(error.to_string()))
}

/// Has KDE's shortcut service forget what it keeps of Snappy's once the daemon is gone: the
/// shortcuts of Snappy's component, which its settings keep after the daemon lets go of them,
/// and the doorbell the script registers among KWin's. True when the service had any of them.
pub fn forget_shortcuts(conn: &Connection) -> Result<bool, CallError> {
    let unreachable = |error: zbus::Error| CallError::Unreachable(error.to_string());
    let component = conn.call_method(
        Some(GLOBAL_ACCEL_NAME),
        crate::hotkeys::COMPONENT_PATH,
        Some(COMPONENT_INTERFACE),
        "cleanUp",
        &(),
    );
    let cleaned = match component {
        Ok(reply) => reply.body().deserialize::<bool>().unwrap_or(false),
        // The service has no component of Snappy's.
        Err(zbus::Error::MethodError(..)) => false,
        Err(error) => return Err(unreachable(error)),
    };
    let doorbell = conn
        .call_method(
            Some(GLOBAL_ACCEL_NAME),
            GLOBAL_ACCEL_PATH,
            Some(GLOBAL_ACCEL_INTERFACE),
            "unregister",
            &(KWIN_COMPONENT, DOORBELL),
        )
        .map_err(unreachable)?
        .body()
        .deserialize::<bool>()
        .unwrap_or(false);
    Ok(cleaned || doorbell)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(seq: u64) -> Answer {
        Answer {
            seq,
            ok: serde_json::Value::Null,
            error: None,
        }
    }

    #[test]
    fn taking_hands_over_waiting_requests_in_order_and_an_empty_take_ends_collecting() {
        let exchange = Exchange::default();
        {
            let mut state = exchange.state();
            state.waiting.extend([
                r#"{"seq":0,"op":"windows"}"#.to_owned(),
                r#"{"seq":1,"op":"pointer"}"#.to_owned(),
            ]);
            state.collecting = true;
        }
        assert_eq!(
            exchange.take(),
            r#"[{"seq":0,"op":"windows"},{"seq":1,"op":"pointer"}]"#
        );
        assert!(exchange.state().collecting);
        assert_eq!(exchange.take(), "[]");
        assert!(!exchange.state().collecting);
    }

    #[test]
    fn answers_wait_for_whoever_asked_and_late_ones_are_dropped() {
        let exchange = Exchange::default();
        exchange.state().pending.extend([2, 4]);
        exchange.answer(vec![answer(4), answer(2), answer(3)]);
        let state = exchange.state();
        assert!(state.answers.contains_key(&2) && state.answers.contains_key(&4));
        assert!(!state.answers.contains_key(&3), "no one waits for 3 any more");
    }
}
