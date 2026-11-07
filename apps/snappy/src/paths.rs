//! Where Snappy keeps its files on each OS.

use std::path::PathBuf;

use snappy_engine::Paths;

/// Names a folder to hold every file instead of the standard locations. Live tests use it
/// to run Snappy against scratch settings and state.
const HOME_OVERRIDE: &str = "SNAPPY_HOME";

pub fn snappy_paths() -> Option<Paths> {
    if let Some(home) = std::env::var_os(HOME_OVERRIDE).filter(|home| !home.is_empty()) {
        let home = PathBuf::from(home);
        return Some(Paths {
            settings: home.join("settings.json"),
            state_dir: home.join("state"),
            log_dir: home.join("logs"),
        });
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
        settings: settings_dir.join("settings.json"),
        state_dir,
        log_dir,
    })
}
