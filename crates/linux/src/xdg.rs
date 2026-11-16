//! The user's XDG base folders. A variable that is unset, empty, or relative is ignored, as
//! the XDG Base Directory specification says, and the default under the home folder is used.

use std::ffi::OsString;
use std::path::PathBuf;

/// `$XDG_CONFIG_HOME`, else `~/.config`.
pub fn config_home() -> Option<PathBuf> {
    base(std::env::var_os("XDG_CONFIG_HOME"), ".config")
}

/// `$XDG_DATA_HOME`, else `~/.local/share`.
pub fn data_home() -> Option<PathBuf> {
    base(std::env::var_os("XDG_DATA_HOME"), ".local/share")
}

fn base(variable: Option<OsString>, default_in_home: &str) -> Option<PathBuf> {
    variable
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(default_in_home)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_absolute_variable_is_used() {
        let set = |value: &str| base(Some(value.into()), ".config");
        assert_eq!(set("/custom/config"), Some(PathBuf::from("/custom/config")));
        let home = std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config"));
        assert_eq!(set("relative/config"), home);
        assert_eq!(set(""), home);
        assert_eq!(base(None, ".config"), home);
    }
}
