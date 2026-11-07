//! Files the engine owns: settings, workspace state, and diagnostics.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use snappy_core::keys::Platform;
use snappy_core::settings::{Settings, SettingsError};

/// State files are never read past this size.
const STATE_READ_LIMIT: u64 = 1_000_000;
/// Diagnostic logs are read back only below this size.
const LOG_READ_LIMIT: u64 = 64 * 1024;

/// Where Snappy keeps its files.
#[derive(Debug, Clone)]
pub struct Paths {
    pub settings: PathBuf,
    pub state_dir: PathBuf,
    pub log_dir: PathBuf,
}

impl Paths {
    pub fn journal(&self) -> PathBuf {
        self.state_dir.join("workspace-journal.json")
    }

    pub fn membership(&self) -> PathBuf {
        self.state_dir.join("workspace-membership.json")
    }

    pub fn log(&self, name: &str) -> PathBuf {
        self.log_dir.join(name)
    }
}

/// The settings file. After a failed load it refuses every save, so an unreadable file
/// is never replaced.
#[derive(Debug)]
pub struct SettingsStore {
    path: PathBuf,
    platform: Platform,
    protected: bool,
}

impl SettingsStore {
    pub fn new(path: PathBuf, platform: Platform) -> Self {
        Self {
            path,
            platform,
            protected: false,
        }
    }

    /// True when no settings file has been written yet: Snappy's first launch.
    pub fn is_new(&self) -> bool {
        !self.path.exists()
    }

    /// Load the settings, or defaults when there is no file yet.
    pub fn load(&mut self) -> Result<Settings, SettingsError> {
        let result = match fs::read(&self.path) {
            Ok(bytes) => Settings::from_json(&bytes, self.platform),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Settings::defaults(self.platform).validate(self.platform)
            }
            Err(_) => Err(SettingsError::Invalid),
        };
        self.protected = result.is_err();
        result
    }

    pub fn save(&self, settings: &Settings) -> Result<(), SettingsError> {
        if self.protected {
            return Err(SettingsError::Protected);
        }
        let bytes = settings.to_json()?;
        write_atomic(&self.path, &bytes, false).map_err(|_| SettingsError::Invalid)
    }

    /// Read another settings file, for import.
    pub fn read_file(path: &Path, platform: Platform) -> Result<Settings, SettingsError> {
        let bytes = fs::read(path).map_err(|_| SettingsError::Invalid)?;
        Settings::from_json(&bytes, platform)
    }

    /// Write the settings to another file, for export.
    pub fn write_file(path: &Path, settings: &Settings) -> Result<(), SettingsError> {
        write_atomic(path, &settings.to_json()?, false).map_err(|_| SettingsError::Invalid)
    }
}

/// Read a JSON state file; missing, oversized, or unreadable files read as `None`.
pub fn read_state<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let size = fs::metadata(path).ok()?.len();
    if size >= STATE_READ_LIMIT {
        return None;
    }
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

/// Write a JSON state file readable only by the user.
pub fn write_state<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(std::io::Error::other)?;
    write_atomic(path, &bytes, true)
}

pub fn remove_state(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// Append `entry` to a bounded JSON log, keeping the newest `keep` entries. Failures are
/// ignored: diagnostics never affect window control.
pub fn append_log<T: Serialize + DeserializeOwned>(path: &Path, entry: T, keep: usize) {
    let small_enough = fs::metadata(path)
        .map(|m| m.len() <= LOG_READ_LIMIT)
        .unwrap_or(true);
    let mut entries: Vec<T> = if small_enough {
        fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    entries.push(entry);
    let excess = entries.len().saturating_sub(keep);
    entries.drain(..excess);
    let _ = write_state(path, &entries);
}

/// Write via a temporary file in the same folder and rename it into place.
fn write_atomic(path: &Path, bytes: &[u8], private: bool) -> std::io::Result<()> {
    let folder = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(folder)?;
    let temporary = folder.join(format!(
        ".{}.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("snappy")
    ));
    {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    if private {
        restrict_to_owner(&temporary)?;
    }
    fs::rename(&temporary, path)
}

#[cfg(unix)]
fn restrict_to_owner(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn restrict_to_owner(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("snappy-store-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_missing_file_means_defaults() {
        let dir = temp_dir("missing");
        let mut store = SettingsStore::new(dir.join("settings.json"), Platform::MacOs);
        let settings = store.load().unwrap();
        assert_eq!(
            settings,
            Settings::defaults(Platform::MacOs)
                .validate(Platform::MacOs)
                .unwrap()
        );
        store.save(&settings).unwrap();
        assert!(dir.join("settings.json").exists());
    }

    #[test]
    fn unreadable_files_are_never_replaced() {
        let dir = temp_dir("corrupt");
        let path = dir.join("settings.json");
        fs::write(&path, b"{ broken").unwrap();
        let mut store = SettingsStore::new(path.clone(), Platform::MacOs);
        assert!(store.load().is_err());
        let defaults = Settings::defaults(Platform::MacOs);
        assert_eq!(store.save(&defaults), Err(SettingsError::Protected));
        assert_eq!(fs::read(&path).unwrap(), b"{ broken");
    }

    #[test]
    fn a_save_replaces_the_whole_file_and_leaves_nothing_behind() {
        let dir = temp_dir("whole");
        let path = dir.join("settings.json");
        fs::write(&path, vec![b'x'; 200_000]).unwrap();
        let store = SettingsStore::new(path.clone(), Platform::MacOs);
        let settings = Settings::defaults(Platform::MacOs);
        store.save(&settings).unwrap();
        assert_eq!(fs::read(&path).unwrap(), settings.to_json().unwrap());
        let names: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["settings.json"]);
    }

    #[test]
    fn a_failed_save_leaves_the_existing_file_intact() {
        let dir = temp_dir("failed");
        let path = dir.join("settings.json");
        fs::write(&path, b"previous").unwrap();
        fs::create_dir(dir.join(".settings.json.tmp")).unwrap();
        let store = SettingsStore::new(path.clone(), Platform::MacOs);
        assert!(store.save(&Settings::defaults(Platform::MacOs)).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"previous");
    }

    #[test]
    fn logs_keep_only_the_newest_entries() {
        let dir = temp_dir("log");
        let path = dir.join("drops.json");
        for value in 0..25 {
            append_log(&path, value, 20);
        }
        let kept: Vec<i32> = read_state(&path).unwrap();
        assert_eq!(kept.first(), Some(&5));
        assert_eq!(kept.len(), 20);
    }

    #[cfg(unix)]
    #[test]
    fn state_files_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("private");
        let path = dir.join("state.json");
        write_state(&path, &vec![1, 2, 3]).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }
}
