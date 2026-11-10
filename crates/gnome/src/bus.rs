//! The session bus: requests to the extension, and what GNOME Shell and Mutter report about
//! extensions and displays.

use std::collections::HashMap;
use std::sync::mpsc::{self, Sender};
use std::time::Duration;

use serde::de::DeserializeOwned;
use zbus::blocking::Connection;
use zbus::zvariant::OwnedValue;

use crate::protocol::{INTERFACE, OBJECT_PATH, Request, SHELL_BUS_NAME};

/// How long a request may take before the engine gives up on it. GNOME Shell answers from
/// its main loop, normally within a frame.
const CALL_TIMEOUT: Duration = Duration::from_secs(2);

const EXTENSIONS_PATH: &str = "/org/gnome/Shell";
const EXTENSIONS_INTERFACE: &str = "org.gnome.Shell.Extensions";
const PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";
pub const DISPLAY_CONFIG_NAME: &str = "org.gnome.Mutter.DisplayConfig";
pub const DISPLAY_CONFIG_PATH: &str = "/org/gnome/Mutter/DisplayConfig";

/// Why a request didn't succeed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallError {
    /// The bus or GNOME Shell didn't answer, or the extension isn't there.
    Unreachable(String),
    /// The extension answered with an error, such as `gone` for a closed window.
    Refused(String),
    /// A reply that doesn't have the shape this version expects.
    Malformed(String),
}

impl CallError {
    /// Logs what went wrong for the daemon's output, once, where it happened.
    fn logged(self) -> CallError {
        if let CallError::Malformed(detail) = &self {
            eprintln!("snappy: unexpected reply from GNOME Shell: {detail}");
        }
        self
    }
}

impl CallError {
    pub fn is_refusal(&self, reason: &str) -> bool {
        matches!(self, CallError::Refused(refused) if refused == reason)
    }
}

/// What `GetExtensionInfo` says about an extension; empty when Shell doesn't know it.
pub type ExtensionInfo = HashMap<String, OwnedValue>;

/// Work for the bus thread.
type Job = Box<dyn FnOnce(&Connection) + Send>;

/// The session bus connection the adapter shares between its threads. Calls run on a bus
/// thread of their own: the engine thread drives an async runtime, inside which zbus can't
/// block, so it waits for the bus thread's answer instead.
#[derive(Clone)]
pub struct Bus {
    conn: Connection,
    jobs: Sender<Job>,
}

impl Bus {
    pub fn session() -> Result<Bus, crate::error::GnomeError> {
        let conn = zbus::blocking::connection::Builder::session()?
            .method_timeout(CALL_TIMEOUT)
            .build()?;
        let (jobs, queue) = mpsc::channel::<Job>();
        let worker = conn.clone();
        std::thread::Builder::new()
            .name("snappy-gnome-bus".into())
            .spawn(move || {
                for job in queue {
                    job(&worker);
                }
            })?;
        Ok(Bus { conn, jobs })
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Runs `job` on the bus thread and waits for its result; `None` if the thread is gone.
    fn run<T: Send + 'static>(&self, job: impl FnOnce(&Connection) -> T + Send + 'static) -> Option<T> {
        let (answer, answered) = mpsc::channel();
        let job: Job = Box::new(move |conn| {
            let _ = answer.send(job(conn));
        });
        self.jobs.send(job).ok()?;
        answered.recv().ok()
    }

    fn call_method<B>(
        &self,
        destination: &'static str,
        path: &'static str,
        interface: &'static str,
        method: &'static str,
        body: B,
    ) -> Result<zbus::Message, CallError>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType + Send + 'static,
    {
        self.run(move |conn| {
            conn.call_method(Some(destination), path, Some(interface), method, &body)
                .map_err(|error| CallError::Unreachable(error.to_string()))
        })
        .unwrap_or_else(|| Err(CallError::Unreachable("the bus thread stopped".to_owned())))
    }

    /// Sends a request to the extension and reads the `ok` value of its reply.
    pub fn call<T: DeserializeOwned>(&self, request: &Request) -> Result<T, CallError> {
        let json = serde_json::to_string(request)
            .map_err(|error| CallError::Malformed(error.to_string()).logged())?;
        let reply = self.call_method(SHELL_BUS_NAME, OBJECT_PATH, INTERFACE, "Call", (json,))?;
        let (text,): (String,) = reply
            .body()
            .deserialize()
            .map_err(|error| CallError::Malformed(error.to_string()).logged())?;
        parse_reply(&text).map_err(CallError::logged)
    }

    /// A request whose reply carries nothing.
    pub fn send(&self, request: &Request) -> Result<(), CallError> {
        self.call::<serde_json::Value>(request).map(drop)
    }

    fn property(
        &self,
        destination: &'static str,
        path: &'static str,
        interface: &'static str,
        name: &'static str,
    ) -> Option<OwnedValue> {
        let reply = self
            .call_method(destination, path, PROPERTIES_INTERFACE, "Get", (interface, name))
            .ok()?;
        reply.body().deserialize::<OwnedValue>().ok()
    }

    /// The protocol version the loaded extension speaks, when one is loaded.
    pub fn extension_version(&self) -> Option<u32> {
        u32::try_from(self.property(SHELL_BUS_NAME, OBJECT_PATH, INTERFACE, "Version")?).ok()
    }

    pub fn extension_info(&self, uuid: &str) -> Option<ExtensionInfo> {
        let uuid = uuid.to_owned();
        let reply = self
            .call_method(
                SHELL_BUS_NAME,
                EXTENSIONS_PATH,
                EXTENSIONS_INTERFACE,
                "GetExtensionInfo",
                (uuid,),
            )
            .ok()?;
        reply.body().deserialize::<ExtensionInfo>().ok()
    }

    /// Whether GNOME lets user extensions run at all.
    pub fn user_extensions_enabled(&self) -> Option<bool> {
        bool::try_from(self.property(
            SHELL_BUS_NAME,
            EXTENSIONS_PATH,
            EXTENSIONS_INTERFACE,
            "UserExtensionsEnabled",
        )?)
        .ok()
    }

    /// Ends the desktop session after GNOME's own confirmation, as its menu's Log Out does.
    pub fn log_out(&self) -> Result<(), CallError> {
        self.call_method(
            "org.gnome.SessionManager",
            "/org/gnome/SessionManager",
            "org.gnome.SessionManager",
            "Logout",
            (0u32,),
        )
        .map(drop)
    }

    /// Mutter's `GetCurrentState`, or `None` when it can't be read.
    pub fn display_state(&self) -> Option<crate::displays::CurrentState> {
        let reply = self
            .call_method(
                DISPLAY_CONFIG_NAME,
                DISPLAY_CONFIG_PATH,
                DISPLAY_CONFIG_NAME,
                "GetCurrentState",
                (),
            )
            .ok()?;
        reply.body().deserialize().ok()
    }
}

/// `{"ok": value}` or `{"error": message}`.
fn parse_reply<T: DeserializeOwned>(text: &str) -> Result<T, CallError> {
    let mut reply: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(text).map_err(|error| CallError::Malformed(error.to_string()))?;
    if let Some(error) = reply.remove("error") {
        return Err(CallError::Refused(
            error.as_str().map_or_else(|| error.to_string(), str::to_owned),
        ));
    }
    let value = reply.remove("ok").unwrap_or(serde_json::Value::Null);
    serde_json::from_value(value).map_err(|error| CallError::Malformed(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_carry_a_value_or_an_error() {
        assert_eq!(parse_reply::<u32>(r#"{"ok":5}"#), Ok(5));
        assert_eq!(parse_reply::<Option<u32>>(r#"{"ok":null}"#), Ok(None));
        assert_eq!(parse_reply::<()>(r#"{"ok":null}"#), Ok(()));
        assert_eq!(
            parse_reply::<u32>(r#"{"error":"gone"}"#),
            Err(CallError::Refused("gone".into()))
        );
        assert!(matches!(
            parse_reply::<u32>(r#"{"ok":"text"}"#),
            Err(CallError::Malformed(_))
        ));
        assert!(matches!(
            parse_reply::<u32>("not json"),
            Err(CallError::Malformed(_))
        ));
    }

    #[test]
    fn refusals_are_recognized_by_reason() {
        assert!(CallError::Refused("gone".into()).is_refusal("gone"));
        assert!(!CallError::Unreachable("gone".into()).is_refusal("gone"));
    }
}
