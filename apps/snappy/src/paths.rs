//! Where Snappy keeps its files on each OS.

use std::path::PathBuf;

use snappy_engine::{Paths, file_names};
use snappy_platform::launch;

pub fn snappy_paths() -> Option<Paths> {
    if let Some(home) = std::env::var_os(launch::HOME_ENV).filter(|home| !home.is_empty()) {
        return Some(Paths::in_folder(&PathBuf::from(home)));
    }
    let project = directories::ProjectDirs::from("", "", "Snappy")?;
    let home = directories::BaseDirs::new()?.home_dir().to_path_buf();

    let settings_dir = project.config_dir().to_path_buf();
    let state_dir = if cfg!(target_os = "macos") {
        settings_dir.clone()
    } else {
        project
            .state_dir()
            .unwrap_or_else(|| project.data_local_dir())
            .to_path_buf()
    };
    let log_dir: PathBuf = if cfg!(target_os = "macos") {
        home.join("Library/Logs/Snappy")
    } else if cfg!(target_os = "windows") {
        project.data_local_dir().join("Logs")
    } else {
        state_dir.join("logs")
    };

    Some(Paths {
        settings: settings_dir.join(file_names::SETTINGS),
        state_dir,
        log_dir,
    })
}
