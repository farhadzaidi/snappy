use std::path::{Path, PathBuf};

use super::file_names;

/// Where Snappy keeps its files.
#[derive(Debug, Clone)]
pub struct Paths {
    pub settings: PathBuf,
    pub state_dir: PathBuf,
    pub log_dir: PathBuf,
}

impl Paths {
    /// Every file in one folder, as when `SNAPPY_HOME` names it.
    pub fn in_folder(folder: &Path) -> Paths {
        Paths {
            settings: folder.join(file_names::SETTINGS),
            state_dir: folder.join(file_names::STATE_FOLDER),
            log_dir: folder.join(file_names::LOGS_FOLDER),
        }
    }

    pub fn journal(&self) -> PathBuf {
        self.state_dir.join(file_names::JOURNAL)
    }

    pub fn membership(&self) -> PathBuf {
        self.state_dir.join(file_names::MEMBERSHIP)
    }

    /// What undoes Snappy's change to Caps Lock, while the change is in place.
    pub fn caps_lock(&self) -> PathBuf {
        self.state_dir.join(file_names::CAPS_LOCK)
    }

    pub fn log(&self, name: &str) -> PathBuf {
        self.log_dir.join(name)
    }
}
