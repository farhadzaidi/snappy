//! The names Snappy goes by on Linux, and the message bus's own.

/// Snappy's application ID: its icon's name, and the root of its D-Bus names.
pub const APP_ID: &str = "io.github.farhadzaidi.Snappy";

/// The message bus itself, which reports names coming and going.
pub const DBUS_NAME: &str = "org.freedesktop.DBus";
pub const DBUS_PATH: &str = "/org/freedesktop/DBus";
pub const NAME_OWNER_CHANGED: &str = "NameOwnerChanged";

/// The object path D-Bus convention gives `name` under the application ID, such as
/// `/io/github/farhadzaidi/Snappy/KWin` for `KWin`.
pub fn object_path(name: &str) -> String {
    format!("/{}/{name}", APP_ID.replace('.', "/"))
}
