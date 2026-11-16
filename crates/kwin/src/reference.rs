//! Read by the live-test harness, so its expectations use Snappy's own view of keys and
//! displays.

use snappy_core::keys::Combo;

/// The Qt key code a shortcut is registered with.
pub fn key_code(combo: &Combo) -> Option<i32> {
    crate::keys::key_code(combo)
}

/// The displays Snappy makes of KWin's outputs, primary first.
pub fn displays(monitors: &[crate::protocol::Monitor]) -> Vec<snappy_platform::Display> {
    crate::displays::displays(monitors)
}

/// Loads the plain JavaScript file at `path` into KWin as the script `name` and starts it, as
/// the tests do with throwaway probes that report back over D-Bus.
pub fn run_plain_script(
    bus: &zbus::blocking::Connection,
    path: &std::path::Path,
    name: &str,
) -> Result<(), String> {
    use crate::bus::{KWIN_NAME, SCRIPT_INTERFACE, SCRIPTING_INTERFACE, SCRIPTING_PATH, script_path};
    let failed = |error: zbus::Error| format!("KWin's scripting didn't answer: {error}");
    let reply = bus
        .call_method(
            Some(KWIN_NAME),
            SCRIPTING_PATH,
            Some(SCRIPTING_INTERFACE),
            "loadScript",
            &(path.to_string_lossy().into_owned(), name),
        )
        .map_err(failed)?;
    let id: i32 = reply.body().deserialize().map_err(|error| error.to_string())?;
    bus.call_method(
        Some(KWIN_NAME),
        script_path(id).as_str(),
        Some(SCRIPT_INTERFACE),
        "run",
        &(),
    )
    .map(drop)
    .map_err(failed)
}

/// Unloads the script `name`, if KWin has it.
pub fn unload_script(bus: &zbus::blocking::Connection, name: &str) {
    use crate::bus::{KWIN_NAME, SCRIPTING_INTERFACE, SCRIPTING_PATH};
    let _ = bus.call_method(
        Some(KWIN_NAME),
        SCRIPTING_PATH,
        Some(SCRIPTING_INTERFACE),
        "unloadScript",
        &(name,),
    );
}
