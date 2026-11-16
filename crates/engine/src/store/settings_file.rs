//! The settings file, which a failed load protects from being overwritten.

use std::fs;
use std::path::{Path, PathBuf};

use snappy_core::keys::Os;
use snappy_core::settings::{Settings, SettingsError};

use super::atomic_write::write_atomic;

/// The settings file. After a failed load it refuses every save, so an unreadable file
/// is never replaced.
#[derive(Debug)]
pub struct SettingsStore {
    path: PathBuf,
    platform: Os,
    protected: bool,
}

impl SettingsStore {
    pub fn new(path: PathBuf, platform: Os) -> Self {
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
            Err(_) => Err(SettingsError::Unreadable),
        };
        self.protected = result.is_err();
        result
    }

    pub fn save(&self, settings: &Settings) -> Result<(), SettingsError> {
        if self.protected {
            return Err(SettingsError::Protected);
        }
        let bytes = settings.to_json()?;
        write_atomic(&self.path, &bytes, false).map_err(|_| SettingsError::Unwritable)
    }

    /// Read another settings file, for import.
    pub fn read_file(path: &Path, platform: Os) -> Result<Settings, SettingsError> {
        let bytes = fs::read(path).map_err(|_| SettingsError::Unreadable)?;
        Settings::from_json(&bytes, platform)
    }

    /// Write the settings to another file, for export.
    pub fn write_file(path: &Path, settings: &Settings) -> Result<(), SettingsError> {
        write_atomic(path, &settings.to_json()?, false).map_err(|_| SettingsError::Unwritable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_dirs::temp_dir;

    #[test]
    fn a_missing_file_means_defaults() {
        let dir = temp_dir("missing");
        let mut store = SettingsStore::new(dir.join("settings.json"), Os::MacOs);
        let settings = store.load().unwrap();
        assert_eq!(
            settings,
            Settings::defaults(Os::MacOs).validate(Os::MacOs).unwrap()
        );
        store.save(&settings).unwrap();
        assert!(dir.join("settings.json").exists());
    }

    #[test]
    fn unreadable_files_are_never_replaced() {
        let dir = temp_dir("corrupt");
        let path = dir.join("settings.json");
        fs::write(&path, b"{ broken").unwrap();
        let mut store = SettingsStore::new(path.clone(), Os::MacOs);
        assert!(store.load().is_err());
        let defaults = Settings::defaults(Os::MacOs);
        assert_eq!(store.save(&defaults), Err(SettingsError::Protected));
        assert_eq!(fs::read(&path).unwrap(), b"{ broken");
    }

    #[test]
    fn a_save_replaces_the_whole_file_and_leaves_nothing_behind() {
        let dir = temp_dir("whole");
        let path = dir.join("settings.json");
        fs::write(&path, vec![b'x'; 200_000]).unwrap();
        let store = SettingsStore::new(path.clone(), Os::MacOs);
        let settings = Settings::defaults(Os::MacOs);
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
        let store = SettingsStore::new(path.clone(), Os::MacOs);
        assert!(store.save(&Settings::defaults(Os::MacOs)).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"previous");
    }
}
