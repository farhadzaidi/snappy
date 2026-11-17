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

/// The app's bundle identifier, which names the folders its web view and the system keep
/// for it.
pub const APP_ID: &str = "io.github.farhadzaidi.Snappy.App";

/// The daemon's bundle identifier on macOS.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const DAEMON_ID: &str = "io.github.farhadzaidi.Snappy";

/// What macOS and the app's web view keep for Snappy in `~/Library`, besides Snappy's own
/// folders. Homebrew's cask trashes the same with `--zap`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn macos_library_entries() -> Vec<String> {
    vec![
        format!("Caches/{DAEMON_ID}"),
        format!("Caches/{APP_ID}"),
        format!("HTTPStorages/{APP_ID}"),
        format!("Preferences/{APP_ID}.plist"),
        format!("Preferences/{DAEMON_ID}.plist"),
        format!("Saved Application State/{APP_ID}.savedState"),
        format!("WebKit/{APP_ID}"),
    ]
}

/// What `snappy --uninstall` deletes, files and whole folders: Snappy's own folders, or where
/// one folder holds every file, only Snappy's entries in it; then what the app's web view and
/// the system keep for Snappy.
pub fn uninstall_targets(paths: &Paths) -> Vec<PathBuf> {
    let mut targets = if one_folder(std::env::var_os(launch::HOME_ENV), package_data_folder).is_some() {
        vec![
            paths.settings.clone(),
            paths.state_dir.clone(),
            paths.log_dir.clone(),
        ]
    } else {
        own_folders(paths)
    };
    targets.extend(app_data());
    targets
}

/// The folders in the usual places that hold nothing but Snappy's files. On Windows each
/// sits in a `Snappy` folder of its own, which goes too.
fn own_folders(paths: &Paths) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = Vec::new();
    let settings_dir = paths.settings.parent().unwrap_or(&paths.settings);
    for folder in [settings_dir, &paths.state_dir, &paths.log_dir] {
        let folder = if cfg!(windows) {
            folder.parent().unwrap_or(folder)
        } else {
            folder
        };
        if !folders.iter().any(|kept| folder.starts_with(kept)) {
            folders.push(folder.to_path_buf());
        }
    }
    folders
}

#[cfg(target_os = "macos")]
fn app_data() -> Vec<PathBuf> {
    let Some(base) = directories::BaseDirs::new() else {
        return Vec::new();
    };
    let library = base.home_dir().join("Library");
    macos_library_entries()
        .iter()
        .map(|entry| library.join(entry))
        .collect()
}

/// The web view's data, which Tauri keeps in a folder named after the app in the user's
/// local data folder, and on Linux WebKit's cache.
#[cfg(not(target_os = "macos"))]
fn app_data() -> Vec<PathBuf> {
    let Some(base) = directories::BaseDirs::new() else {
        return Vec::new();
    };
    let mut folders = vec![base.data_local_dir().join(APP_ID)];
    if cfg!(target_os = "linux") {
        folders.push(base.cache_dir().join(APP_ID));
    }
    folders
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

    #[test]
    fn uninstalling_deletes_each_own_folder_once() {
        let state = PathBuf::from("/home/me/.local/state/snappy");
        let paths = Paths {
            settings: PathBuf::from("/home/me/.config/snappy/settings.json"),
            log_dir: state.join("logs"),
            state_dir: state.clone(),
        };
        let folders = own_folders(&paths);
        if cfg!(windows) {
            assert_eq!(
                folders,
                [
                    PathBuf::from("/home/me/.config"),
                    PathBuf::from("/home/me/.local/state")
                ]
            );
        } else {
            assert_eq!(folders, [PathBuf::from("/home/me/.config/snappy"), state]);
        }
    }

    fn repository_file(path: &str) -> String {
        std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(path),
        )
        .unwrap()
    }

    #[test]
    fn the_app_and_daemon_ids_are_their_bundles_own() {
        assert!(
            repository_file("packaging/macos/Snappy.plist")
                .contains(&format!("<string>{DAEMON_ID}</string>"))
        );
        assert!(
            repository_file("apps/app/src-tauri/tauri.conf.json")
                .contains(&format!("\"identifier\": \"{APP_ID}\""))
        );
        assert!(
            repository_file("packaging/macos/SnappyApp.plist")
                .contains(&format!("<string>{APP_ID}</string>"))
        );
    }

    #[test]
    fn uninstalling_on_macos_deletes_what_homebrew_zaps() {
        let cask = repository_file("packaging/homebrew/snappy.rb.template");
        let zap = cask.split("zap trash:").nth(1).unwrap();
        let mut zapped: Vec<&str> = zap
            .lines()
            .filter_map(|line| line.trim().strip_prefix("\"~/Library/")?.strip_suffix("\","))
            .collect();
        zapped.retain(|entry| !["Application Support/Snappy", "Logs/Snappy"].contains(entry));
        zapped.sort_unstable();
        let mut entries = macos_library_entries();
        entries.sort_unstable();
        assert_eq!(zapped, entries);
    }
}
