//! Open at login through an XDG autostart entry.

use std::path::{Path, PathBuf};

const FILE_NAME: &str = "snappy.desktop";

/// `$XDG_CONFIG_HOME/autostart/snappy.desktop`, defaulting to `~/.config`.
pub fn entry_path() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(config.join("autostart").join(FILE_NAME))
}

/// Enabled when the entry exists and isn't hidden.
pub fn is_enabled(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|text| !text.lines().any(|line| line.trim() == "Hidden=true"))
}

pub fn set_enabled(path: &Path, executable: &Path, on: bool) -> Result<(), String> {
    let result = if on {
        write_entry(path, executable)
    } else {
        match std::fs::remove_file(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    };
    result.map_err(|error| format!("Snappy could not change the login item: {error}"))
}

fn write_entry(path: &Path, executable: &Path) -> std::io::Result<()> {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(path, desktop_entry(executable))
}

/// The autostart entry running `snappy --background`.
pub fn desktop_entry(executable: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Snappy\n\
         Comment=Arrange windows into zones\n\
         Exec={} --background\n\
         Terminal=false\n\
         NoDisplay=true\n\
         X-GNOME-Autostart-enabled=true\n",
        exec_argument(&executable.to_string_lossy())
    )
}

/// Quotes a path for `Exec`: reserved characters are backslash-escaped inside double
/// quotes, then the value's own escaping doubles every backslash. `%` starts field codes,
/// so it doubles too.
fn exec_argument(path: &str) -> String {
    let mut quoted = String::from("\"");
    for character in path.chars() {
        match character {
            '"' | '`' | '$' => {
                quoted.push_str("\\\\");
                quoted.push(character);
            }
            '\\' => quoted.push_str("\\\\\\\\"),
            '%' => quoted.push_str("%%"),
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_runs_snappy_in_the_background() {
        let entry = desktop_entry(Path::new("/usr/bin/snappy"));
        assert!(entry.starts_with("[Desktop Entry]\nType=Application\n"));
        assert!(entry.contains("\nExec=\"/usr/bin/snappy\" --background\n"));
        assert!(!entry.contains("Hidden=true"));
    }

    #[test]
    fn exec_paths_are_quoted_and_escaped() {
        assert_eq!(exec_argument("/opt/My Apps/snappy"), "\"/opt/My Apps/snappy\"");
        assert_eq!(exec_argument("/a\"b$c`d"), "\"/a\\\\\"b\\\\$c\\\\`d\"");
        assert_eq!(exec_argument("/a\\b"), "\"/a\\\\\\\\b\"");
        assert_eq!(exec_argument("/100%/snappy"), "\"/100%%/snappy\"");
    }

    #[test]
    fn enabling_and_disabling_writes_and_removes_the_entry() {
        let folder = std::env::temp_dir().join(format!("snappy-autostart-{}", std::process::id()));
        let path = folder.join("autostart").join(FILE_NAME);
        assert!(!is_enabled(&path));

        set_enabled(&path, Path::new("/usr/bin/snappy"), true).unwrap();
        assert!(is_enabled(&path));
        set_enabled(&path, Path::new("/usr/bin/snappy"), false).unwrap();
        assert!(!is_enabled(&path));
        set_enabled(&path, Path::new("/usr/bin/snappy"), false).unwrap();

        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "[Desktop Entry]\nHidden=true\n").unwrap();
        assert!(!is_enabled(&path));
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
