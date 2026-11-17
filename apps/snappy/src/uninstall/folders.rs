//! Deleting Snappy's files and folders, keeping the settings file when asked.

use std::fs;
use std::io;
use std::path::Path;
use std::time::Duration;

/// How many times a folder is tried again, and how long apart. The app's web view can hold
/// its files for a moment after the app exits.
const RETRIES: u32 = 6;
const RETRY_PAUSE: Duration = Duration::from_millis(500);

/// Deletes `target`, a file or a folder with everything in it, except `keep` and the folders
/// that lead to it. True when anything was deleted.
pub fn remove(target: &Path, keep: Option<&Path>) -> io::Result<bool> {
    let metadata = match fs::symlink_metadata(target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if keep == Some(target) {
        return Ok(false);
    }
    if !metadata.is_dir() {
        fs::remove_file(target)?;
        return Ok(true);
    }
    if let Some(keep) = keep.filter(|keep| keep.starts_with(target)) {
        let mut removed = false;
        for entry in fs::read_dir(target)? {
            removed |= remove(&entry?.path(), Some(keep))?;
        }
        return Ok(removed);
    }
    remove_folder(target)?;
    Ok(true)
}

fn remove_folder(folder: &Path) -> io::Result<()> {
    let mut tries = 0;
    loop {
        match fs::remove_dir_all(folder) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(_) if tries < RETRIES => {
                tries += 1;
                std::thread::sleep(RETRY_PAUSE);
            }
            other => return other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!("snappy-uninstall-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(folder.join("state/logs")).unwrap();
        fs::write(folder.join("settings.json"), "{}").unwrap();
        fs::write(folder.join("state/workspace-journal.json"), "{}").unwrap();
        fs::write(folder.join("state/logs/debug.jsonl"), "").unwrap();
        folder
    }

    #[test]
    fn a_folder_goes_with_everything_in_it_and_a_missing_one_is_nothing_to_do() {
        let folder = scratch("all");
        assert!(remove(&folder, None).unwrap());
        assert!(!folder.exists());
        assert!(!remove(&folder, None).unwrap());
    }

    #[test]
    fn keeping_the_settings_keeps_only_the_settings_file() {
        let folder = scratch("keep");
        let settings = folder.join("settings.json");
        assert!(remove(&folder, Some(&settings)).unwrap());
        assert!(settings.exists());
        assert_eq!(fs::read_dir(&folder).unwrap().count(), 1);
        assert!(!remove(&settings, Some(&settings)).unwrap());
        assert!(!remove(&folder, Some(&settings)).unwrap(), "nothing else is left");
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_folder_apart_from_the_settings_goes_whole() {
        let folder = scratch("apart");
        let settings = folder.join("settings.json");
        assert!(remove(&folder.join("state"), Some(&settings)).unwrap());
        assert!(!folder.join("state").exists());
        assert!(settings.exists());
        fs::remove_dir_all(&folder).unwrap();
    }
}
