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

    /// What undoes Snappy's change to Caps Lock, while the change is in place.
    pub fn caps_lock(&self) -> PathBuf {
        self.state_dir.join("caps-lock.json")
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

/// The shape of a state file. Each file says which version of its shape it holds, so a later
/// Snappy can tell an older file from a damaged one; a change to the shape bumps `VERSION`.
pub trait StateFile: Serialize + DeserializeOwned {
    const VERSION: u32;

    /// Read a file from before state files had a version.
    fn unversioned(value: serde_json::Value) -> Option<Self>;
}

/// What reading a state file found.
#[derive(Debug, PartialEq)]
pub enum ReadState<T> {
    Missing,
    Read(T),
    /// The file couldn't be read, or holds a version Snappy doesn't know. It was moved
    /// aside, to this path when that worked, so it neither blocks Snappy nor is lost.
    MovedAside(Option<PathBuf>),
}

impl<T> ReadState<T> {
    pub fn read(self) -> Option<T> {
        match self {
            ReadState::Read(value) => Some(value),
            ReadState::Missing | ReadState::MovedAside(_) => None,
        }
    }
}

/// Read a state file, moving it aside when it can't be read.
pub fn read_state<T: StateFile>(path: &Path) -> ReadState<T> {
    let bytes = match fs::metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return ReadState::Missing,
        Ok(metadata) if metadata.len() < STATE_READ_LIMIT => fs::read(path).ok(),
        _ => None,
    };
    match bytes.and_then(|bytes| parse_state(&bytes)) {
        Some(value) => ReadState::Read(value),
        None => ReadState::MovedAside(move_aside(path)),
    }
}

fn parse_state<T: StateFile>(bytes: &[u8]) -> Option<T> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    match value.get("version").map(serde_json::Value::as_u64) {
        None => T::unversioned(value),
        Some(version) if version == Some(u64::from(T::VERSION)) => serde_json::from_value(value).ok(),
        Some(_) => None,
    }
}

/// `name.json` becomes `name.unreadable.json`, replacing an earlier one.
fn move_aside(path: &Path) -> Option<PathBuf> {
    let stem = path.file_stem()?.to_str()?;
    let aside = path.with_file_name(format!("{stem}.unreadable.json"));
    fs::rename(path, &aside).ok().map(|()| aside)
}

/// Write a state file readable only by the user, with its version.
pub fn write_state<T: StateFile>(path: &Path, value: &T) -> std::io::Result<()> {
    let mut json = serde_json::to_value(value).map_err(std::io::Error::other)?;
    let Some(fields) = json.as_object_mut() else {
        return Err(std::io::Error::other("a state file holds an object"));
    };
    fields.insert("version".to_owned(), T::VERSION.into());
    write_json(path, &json)
}

/// Write a diagnostic log readable only by the user. Logs are read back only by Snappy's
/// own appends and by people, so they carry no version.
pub fn write_log<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    write_json(path, value)
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
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
    let _ = write_log(path, &entries);
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
        let kept: Vec<i32> = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(kept.first(), Some(&5));
        assert_eq!(kept.len(), 20);
    }

    #[derive(Debug, PartialEq, Serialize, serde::Deserialize)]
    struct Numbers {
        numbers: Vec<i32>,
    }

    impl StateFile for Numbers {
        const VERSION: u32 = 2;

        fn unversioned(value: serde_json::Value) -> Option<Self> {
            serde_json::from_value(value).ok().map(|numbers| Self { numbers })
        }
    }

    fn numbers() -> Numbers {
        Numbers {
            numbers: vec![1, 2, 3],
        }
    }

    #[cfg(unix)]
    #[test]
    fn state_files_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("private");
        let path = dir.join("state.json");
        write_state(&path, &numbers()).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn state_files_say_their_version_and_read_back() {
        let dir = temp_dir("versioned");
        let path = dir.join("state.json");
        write_state(&path, &numbers()).unwrap();
        let written: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(written["version"], 2);
        assert_eq!(read_state(&path), ReadState::Read(numbers()));
        assert_eq!(read_state::<Numbers>(&dir.join("none.json")), ReadState::Missing);
    }

    #[test]
    fn a_state_file_from_before_versions_still_reads() {
        let dir = temp_dir("unversioned");
        let path = dir.join("state.json");
        fs::write(&path, b"[1, 2, 3]").unwrap();
        assert_eq!(read_state(&path), ReadState::Read(numbers()));
    }

    #[test]
    fn an_unreadable_state_file_or_one_of_another_version_is_moved_aside() {
        let dir = temp_dir("aside");
        let path = dir.join("state.json");
        let aside = dir.join("state.unreadable.json");
        for contents in [&b"{ broken"[..], br#"{ "version": 3, "numbers": [1] }"#] {
            fs::write(&path, contents).unwrap();
            assert_eq!(
                read_state::<Numbers>(&path),
                ReadState::MovedAside(Some(aside.clone()))
            );
            assert!(!path.exists());
            assert_eq!(fs::read(&aside).unwrap(), contents, "kept, not lost");
        }
    }
}
