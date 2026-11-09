//! Snappy's KWin script: written to the session's runtime folder at every start and loaded
//! into KWin over D-Bus, so nothing is installed and nothing outlives the session. KWin keeps
//! compiled QML by file path, so each version of the script gets a folder of its own.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::bus::{Bus, CallError};
use crate::protocol::SCRIPT_NAME;

/// The script's files, relative to its folder.
const FILES: &[(&str, &str)] = &[
    (
        "main.qml",
        include_str!("../../../apps/kwin-script/contents/ui/main.qml"),
    ),
    (
        "service.js",
        include_str!("../../../apps/kwin-script/contents/ui/service.js"),
    ),
    (
        "windows.js",
        include_str!("../../../apps/kwin-script/contents/ui/windows.js"),
    ),
    (
        "parking.js",
        include_str!("../../../apps/kwin-script/contents/ui/parking.js"),
    ),
];
const MAIN: &str = "main.qml";
const FOLDER_PREFIX: &str = "kwin-script-";

/// Where this version of the script lives: `$XDG_RUNTIME_DIR/snappy/kwin-script-<hash>`.
fn folder() -> Option<PathBuf> {
    let mut hasher = DefaultHasher::new();
    FILES.hash(&mut hasher);
    let runtime = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?);
    Some(
        runtime
            .join("snappy")
            .join(format!("{FOLDER_PREFIX}{:016x}", hasher.finish())),
    )
}

/// Writes the script's files, each through a temporary file, and removes older versions.
/// Returns the path KWin loads.
pub fn write() -> io::Result<PathBuf> {
    let folder = folder().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no XDG_RUNTIME_DIR"))?;
    std::fs::create_dir_all(&folder)?;
    for (name, contents) in FILES {
        let path = folder.join(name);
        if std::fs::read_to_string(&path).is_ok_and(|current| current == *contents) {
            continue;
        }
        let temporary = folder.join(format!(".{name}.tmp"));
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&temporary, &path)?;
    }
    if let Some(parent) = folder.parent() {
        remove_older(parent, &folder);
    }
    Ok(folder.join(MAIN))
}

fn remove_older(parent: &Path, current: &Path) {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let ours = entry.file_name().to_string_lossy().starts_with(FOLDER_PREFIX);
        if ours && path != current {
            let _ = std::fs::remove_dir_all(&path);
        }
    }
}

/// Loads the script into KWin, replacing a copy left behind by a daemon that didn't quit
/// cleanly or by an earlier version.
pub fn start(bus: &Bus) -> Result<(), CallError> {
    let main =
        write().map_err(|error| CallError::Unreachable(format!("couldn't write the script: {error}")))?;
    if bus.is_script_loaded(SCRIPT_NAME)? {
        bus.unload_script(SCRIPT_NAME)?;
    }
    bus.run_script(main.to_string_lossy().into_owned(), SCRIPT_NAME)
}

/// Unloads the script, which takes its overlays and doorbell with it.
pub fn stop(bus: &Bus) {
    let _ = bus.unload_script(SCRIPT_NAME);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_the_script_imports_is_shipped() {
        for (_, text) in FILES {
            for line in text.lines() {
                let line = line.trim_start();
                if !(line.starts_with("import \"") || line.starts_with(".import \"")) {
                    continue;
                }
                let file = line.split('"').nth(1).unwrap();
                assert!(
                    FILES.iter().any(|(name, _)| *name == file),
                    "{file} isn't shipped"
                );
            }
        }
    }
}
