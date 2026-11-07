//! Snappy's GNOME Shell extension: the copy built into the daemon, installing or updating
//! it in the user's extensions folder, and what its state means for live control.
//!
//! GNOME Shell on Wayland loads newly installed extensions only at the next login, so a fresh
//! install is turned on in GNOME's settings right away and starts working after logging out
//! and back in. Once installed, turning it on or off is left to the user.

use std::path::{Path, PathBuf};
use std::process::Command;

use snappy_platform::LiveControl;

use crate::bus::Bus;
use crate::messages::{
    EXTENSION_DISABLED, EXTENSION_NEEDS_LOGIN, EXTENSION_NOT_INSTALLED, EXTENSION_OTHER_VERSION,
    EXTENSION_UNSUPPORTED, EXTENSIONS_OFF, SHELL_UNREACHABLE, extension_error,
};
use crate::protocol::{EXTENSION_UUID, PROTOCOL_VERSION};

/// The extension's files, relative to its folder.
pub const FILES: &[(&str, &[u8])] = &[
    (
        "metadata.json",
        include_bytes!("../../../apps/gnome-extension/metadata.json"),
    ),
    (
        "extension.js",
        include_bytes!("../../../apps/gnome-extension/extension.js"),
    ),
    (
        "service.js",
        include_bytes!("../../../apps/gnome-extension/service.js"),
    ),
    (
        "compat.js",
        include_bytes!("../../../apps/gnome-extension/compat.js"),
    ),
    (
        "windows.js",
        include_bytes!("../../../apps/gnome-extension/windows.js"),
    ),
    (
        "input.js",
        include_bytes!("../../../apps/gnome-extension/input.js"),
    ),
    (
        "shortcuts.js",
        include_bytes!("../../../apps/gnome-extension/shortcuts.js"),
    ),
    (
        "overlay.js",
        include_bytes!("../../../apps/gnome-extension/overlay.js"),
    ),
    ("menu.js", include_bytes!("../../../apps/gnome-extension/menu.js")),
    (
        "parking.js",
        include_bytes!("../../../apps/gnome-extension/parking.js"),
    ),
    (
        "icons/snappy-symbolic.svg",
        include_bytes!("../../../apps/gnome-extension/icons/snappy-symbolic.svg"),
    ),
];

/// `GetExtensionInfo` states, which GNOME 47 renamed but kept the numbers of.
mod state {
    pub const ACTIVE: i64 = 1;
    pub const ERROR: i64 = 3;
    pub const OUT_OF_DATE: i64 = 4;
    pub const ACTIVATING: i64 = 8;
}

const EXTENSION_STARTING: &str = "Snappy's GNOME Shell extension is starting.";

/// `$XDG_DATA_HOME/gnome-shell/extensions/<uuid>`, defaulting to `~/.local/share`.
pub fn install_folder() -> Option<PathBuf> {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))?;
    Some(data.join("gnome-shell/extensions").join(EXTENSION_UUID))
}

/// What installing did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Installed {
    /// There was no copy before.
    Fresh,
    /// An older copy was replaced; GNOME Shell loads it at the next login.
    Updated,
    /// The installed copy is this one.
    Current,
}

/// Writes every file that is missing or differs, each through a temporary file in the same
/// folder, so a crash never leaves a half-written file for GNOME Shell to load.
pub fn install(folder: &Path) -> std::io::Result<Installed> {
    let fresh = !folder.join("metadata.json").exists();
    let mut changed = false;
    for (relative, contents) in FILES {
        let path = folder.join(relative);
        if std::fs::read(&path).is_ok_and(|current| current == *contents) {
            continue;
        }
        let parent = path.parent().unwrap_or(folder);
        std::fs::create_dir_all(parent)?;
        let temporary = parent.join(format!(".{}.snappy-new", file_name(relative)));
        std::fs::write(&temporary, contents)?;
        std::fs::rename(&temporary, &path)?;
        changed = true;
    }
    Ok(match (fresh, changed) {
        (true, _) => Installed::Fresh,
        (false, true) => Installed::Updated,
        (false, false) => Installed::Current,
    })
}

fn file_name(relative: &str) -> &str {
    relative.rsplit('/').next().unwrap_or(relative)
}

/// Adds the extension to GNOME's enabled extensions and takes it off the disabled ones. The
/// running GNOME Shell doesn't know a freshly installed extension, so it can't be asked.
pub fn enable_in_settings() -> Result<(), String> {
    let enabled = read_list("enabled-extensions")?;
    if !enabled.iter().any(|uuid| uuid == EXTENSION_UUID) {
        let mut enabled = enabled;
        enabled.push(EXTENSION_UUID.to_owned());
        write_list("enabled-extensions", &enabled)?;
    }
    // GNOME 46 and newer also keep a list of extensions the user turned off.
    if let Ok(disabled) = read_list("disabled-extensions")
        && disabled.iter().any(|uuid| uuid == EXTENSION_UUID)
    {
        let kept: Vec<String> = disabled
            .into_iter()
            .filter(|uuid| uuid != EXTENSION_UUID)
            .collect();
        write_list("disabled-extensions", &kept)?;
    }
    Ok(())
}

const SHELL_SCHEMA: &str = "org.gnome.shell";

fn read_list(key: &str) -> Result<Vec<String>, String> {
    let output = Command::new("gsettings")
        .args(["get", SHELL_SCHEMA, key])
        .output()
        .map_err(|error| format!("gsettings didn't run: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    Ok(parse_list(&String::from_utf8_lossy(&output.stdout)))
}

fn write_list(key: &str, items: &[String]) -> Result<(), String> {
    let status = Command::new("gsettings")
        .args(["set", SHELL_SCHEMA, key, &format_list(items)])
        .status()
        .map_err(|error| format!("gsettings didn't run: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("gsettings exited with {status}"))
}

/// A GVariant string array as `gsettings` prints it: `['a', 'b']` or `@as []`.
fn parse_list(text: &str) -> Vec<String> {
    let text = text.trim();
    let text = text.strip_prefix("@as").unwrap_or(text).trim();
    let inner = text
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or("");
    inner
        .split(',')
        .map(|item| {
            item.trim()
                .trim_matches(|quote| quote == '\'' || quote == '"')
                .to_owned()
        })
        .filter(|item| !item.is_empty())
        .collect()
}

/// Extension IDs hold no quotes or backslashes, so plain single quotes are enough.
fn format_list(items: &[String]) -> String {
    let quoted: Vec<String> = items
        .iter()
        .filter(|item| !item.contains(['\'', '\\']))
        .map(|item| format!("'{item}'"))
        .collect();
    format!("[{}]", quoted.join(", "))
}

/// Whether the extension answers in this protocol version, and if not, why.
pub fn live_control(bus: &Bus, installed: bool) -> LiveControl {
    match bus.extension_version() {
        Some(PROTOCOL_VERSION) => return LiveControl::Available,
        Some(_) => return LiveControl::Unavailable(EXTENSION_OTHER_VERSION.to_owned()),
        None => {}
    }
    let Some(info) = bus.extension_info() else {
        return LiveControl::Unavailable(SHELL_UNREACHABLE.to_owned());
    };
    // GNOME Shell reports the state as a double holding a whole number.
    let state = info
        .get("state")
        .and_then(|value| f64::try_from(value).ok())
        .filter(|state| state.is_finite())
        .map(|state| state.round() as i64);
    let error = info
        .get("error")
        .and_then(|value| String::try_from(value.try_clone().ok()?).ok())
        .unwrap_or_default();
    LiveControl::Unavailable(reason(state, &error, installed, bus.user_extensions_enabled()))
}

/// Why the extension isn't answering, from its state as GNOME Shell reports it.
fn reason(state: Option<i64>, error: &str, installed: bool, extensions_on: Option<bool>) -> String {
    let message = match state {
        None if installed => EXTENSION_NEEDS_LOGIN,
        None => EXTENSION_NOT_INSTALLED,
        // Running but not answering in this version: an older or newer copy is loaded.
        Some(state::ACTIVE) => EXTENSION_OTHER_VERSION,
        Some(state::ERROR) => return extension_error(error),
        Some(state::OUT_OF_DATE) => EXTENSION_UNSUPPORTED,
        Some(state::ACTIVATING) => EXTENSION_STARTING,
        // Inactive, initialized, or deactivating: turned off, by the user or for every extension.
        Some(_) if extensions_on == Some(false) => EXTENSIONS_OFF,
        Some(_) => EXTENSION_DISABLED,
    };
    message.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_extension_file_is_built_in() {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/gnome-extension");
        let mut on_disk = Vec::new();
        let mut pending = vec![folder.clone()];
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    pending.push(path);
                } else {
                    on_disk.push(path.strip_prefix(&folder).unwrap().to_string_lossy().into_owned());
                }
            }
        }
        on_disk.sort();
        let mut built_in: Vec<String> = FILES.iter().map(|(path, _)| (*path).to_owned()).collect();
        built_in.sort();
        assert_eq!(on_disk, built_in);
    }

    #[test]
    fn the_metadata_names_this_extension_and_the_supported_shells() {
        let metadata: serde_json::Value = serde_json::from_slice(FILES[0].1).unwrap();
        assert_eq!(metadata["uuid"], EXTENSION_UUID);
        let versions: Vec<&str> = metadata["shell-version"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect();
        assert_eq!(versions, ["46", "47", "48", "49"]);
    }

    #[test]
    fn installing_writes_once_then_replaces_only_what_changed() {
        let folder = std::env::temp_dir().join(format!("snappy-extension-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        assert_eq!(install(&folder).unwrap(), Installed::Fresh);
        assert_eq!(std::fs::read(folder.join("service.js")).unwrap(), FILES[2].1);
        assert!(folder.join("icons/snappy-symbolic.svg").exists());
        assert_eq!(install(&folder).unwrap(), Installed::Current);

        std::fs::write(folder.join("menu.js"), "// an older copy").unwrap();
        assert_eq!(install(&folder).unwrap(), Installed::Updated);
        assert_eq!(std::fs::read(folder.join("menu.js")).unwrap(), FILES[8].1);
        let leftovers = std::fs::read_dir(&folder)
            .unwrap()
            .filter(|entry| {
                entry
                    .as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with("snappy-new")
            })
            .count();
        assert_eq!(leftovers, 0);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn gsettings_lists_round_trip() {
        assert_eq!(parse_list("@as []\n"), Vec::<String>::new());
        assert_eq!(
            parse_list("['ding@rastersoft.com', 'ubuntu-dock@ubuntu.com']\n"),
            vec!["ding@rastersoft.com", "ubuntu-dock@ubuntu.com"]
        );
        let items = vec!["a@b".to_owned(), EXTENSION_UUID.to_owned()];
        assert_eq!(format_list(&items), "['a@b', 'snappy@farhadzaidi.github.io']");
        assert_eq!(parse_list(&format_list(&items)), items);
        assert_eq!(format_list(&[]), "[]");
    }

    #[test]
    fn each_extension_state_says_what_to_do() {
        assert_eq!(reason(None, "", true, Some(true)), EXTENSION_NEEDS_LOGIN);
        assert_eq!(reason(None, "", false, Some(true)), EXTENSION_NOT_INSTALLED);
        assert_eq!(reason(Some(1), "", true, Some(true)), EXTENSION_OTHER_VERSION);
        assert_eq!(reason(Some(2), "", true, Some(true)), EXTENSION_DISABLED);
        assert_eq!(reason(Some(6), "", true, Some(false)), EXTENSIONS_OFF);
        assert_eq!(reason(Some(4), "", true, Some(true)), EXTENSION_UNSUPPORTED);
        assert_eq!(reason(Some(8), "", true, Some(true)), EXTENSION_STARTING);
        assert_eq!(
            reason(Some(3), "TypeError: x is undefined.", true, Some(true)),
            "Snappy's GNOME Shell extension couldn't start: TypeError: x is undefined. Log out and back \
             in to try again."
        );
    }
}
