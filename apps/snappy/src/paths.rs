//! Where Snappy keeps its files on each OS.

use std::ffi::OsString;
use std::path::PathBuf;

use snappy_engine::{Paths, file_names};
use snappy_platform::launch;

pub fn snappy_paths() -> Option<Paths> {
    if let Some(folder) = one_folder(std::env::var_os(launch::HOME_ENV), package_data_folder) {
        return Some(Paths::in_folder(&folder));
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

/// The one folder that holds every file, when there is one: `SNAPPY_HOME` when it's set, or
/// else the folder an MSIX package keeps its files in.
fn one_folder(home: Option<OsString>, package: impl FnOnce() -> Option<PathBuf>) -> Option<PathBuf> {
    home.filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .or_else(package)
}

#[cfg(windows)]
fn package_data_folder() -> Option<PathBuf> {
    snappy_windows::package_data_folder()
}

#[cfg(not(windows))]
fn package_data_folder() -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package() -> Option<PathBuf> {
        Some(PathBuf::from("package"))
    }

    #[test]
    fn snappy_home_wins_over_the_package_folder() {
        assert_eq!(
            one_folder(Some("home".into()), package),
            Some(PathBuf::from("home"))
        );
    }

    #[test]
    fn a_packaged_snappy_keeps_every_file_in_its_package_folder() {
        assert_eq!(one_folder(None, package), Some(PathBuf::from("package")));
        assert_eq!(
            one_folder(Some(OsString::new()), package),
            Some(PathBuf::from("package"))
        );
    }

    #[test]
    fn an_unpackaged_snappy_without_snappy_home_uses_the_usual_places() {
        assert_eq!(one_folder(None, || None), None);
    }
}
