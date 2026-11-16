//! The operating system, as names, defaults, and reserved shortcuts depend on it.

use serde::{Deserialize, Serialize};

/// The operating system, for display names, defaults, and reserved shortcuts. Written
/// `"macos"`, `"windows"`, or `"linux"`, as the daemon tells the settings app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Os {
    MacOs,
    Windows,
    Linux,
}

impl Os {
    /// The system's name, as messages show it.
    pub fn system_name(self) -> &'static str {
        match self {
            Os::MacOs => "macOS",
            Os::Windows => "Windows",
            Os::Linux => "Linux",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platforms_are_written_by_the_names_the_settings_app_uses() {
        for (platform, name) in [
            (Os::MacOs, "\"macos\""),
            (Os::Windows, "\"windows\""),
            (Os::Linux, "\"linux\""),
        ] {
            assert_eq!(serde_json::to_string(&platform).unwrap(), name);
            assert_eq!(serde_json::from_str::<Os>(name).unwrap(), platform);
        }
    }
}
