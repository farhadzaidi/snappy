//! The daemon's command line: what it's asked to do. Anything it doesn't know is refused, so
//! a mistyped or newer flag never starts Snappy instead.

use std::ffi::OsString;

use snappy_platform::launch;

/// What the command line asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    /// Run as the daemon; `background` starts without opening the app's window.
    Run { background: bool },
    /// Stop the running daemon for a restart.
    #[cfg(windows)]
    StopForRestart,
    /// Quit the running daemon and remove everything Snappy added, keeping the settings
    /// file when `keep_settings` says so.
    Uninstall { keep_settings: bool },
}

/// Why the command line can't be followed. The message is for whoever typed it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArgumentError {
    #[error("{0} isn't one of Snappy's options. Run snappy on its own, or with --background or --uninstall.")]
    Unknown(String),
    #[error("--keep-settings goes with --uninstall.")]
    KeepSettingsAlone,
    #[error("--uninstall goes on its own, or with --keep-settings.")]
    UninstallWithOthers,
}

/// Reads the arguments after the program's name.
pub fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Request, ArgumentError> {
    let (mut background, mut uninstall, mut keep_settings) = (false, false, false);
    #[cfg(windows)]
    let mut stop_for_restart = false;
    for argument in arguments {
        let Some(text) = argument.to_str() else {
            return Err(ArgumentError::Unknown(argument.to_string_lossy().into_owned()));
        };
        match text {
            launch::BACKGROUND => background = true,
            launch::UNINSTALL => uninstall = true,
            launch::KEEP_SETTINGS => keep_settings = true,
            #[cfg(windows)]
            launch::STOP_FOR_RESTART => stop_for_restart = true,
            _ if text.starts_with(launch::PROCESS_SERIAL_NUMBER_PREFIX) => {}
            _ => return Err(ArgumentError::Unknown(text.to_owned())),
        }
    }
    #[cfg(windows)]
    if stop_for_restart {
        return Ok(Request::StopForRestart);
    }
    match (uninstall, keep_settings, background) {
        (true, _, true) => Err(ArgumentError::UninstallWithOthers),
        (true, keep_settings, false) => Ok(Request::Uninstall { keep_settings }),
        (false, true, _) => Err(ArgumentError::KeepSettingsAlone),
        (false, false, background) => Ok(Request::Run { background }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(arguments: &[&str]) -> Result<Request, ArgumentError> {
        parse(arguments.iter().map(OsString::from))
    }

    #[test]
    fn no_arguments_runs_and_opens_the_window_and_background_runs_quietly() {
        assert_eq!(parsed(&[]), Ok(Request::Run { background: false }));
        assert_eq!(parsed(&["--background"]), Ok(Request::Run { background: true }));
    }

    #[test]
    fn uninstall_keeps_the_settings_only_when_asked() {
        assert_eq!(
            parsed(&["--uninstall"]),
            Ok(Request::Uninstall { keep_settings: false })
        );
        assert_eq!(
            parsed(&["--keep-settings", "--uninstall"]),
            Ok(Request::Uninstall { keep_settings: true })
        );
    }

    #[test]
    fn unknown_arguments_are_refused_rather_than_starting_snappy() {
        assert_eq!(
            parsed(&["--version"]),
            Err(ArgumentError::Unknown("--version".to_owned()))
        );
        assert_eq!(
            parsed(&["uninstall"]),
            Err(ArgumentError::Unknown("uninstall".to_owned()))
        );
        assert_eq!(
            parsed(&["--keep-settings"]),
            Err(ArgumentError::KeepSettingsAlone)
        );
        assert_eq!(
            parsed(&["--uninstall", "--background"]),
            Err(ArgumentError::UninstallWithOthers)
        );
    }

    #[test]
    fn the_process_serial_number_older_macos_adds_is_ignored() {
        assert_eq!(parsed(&["-psn_0_12345"]), Ok(Request::Run { background: false }));
    }

    #[cfg(not(windows))]
    #[test]
    fn stopping_for_a_restart_is_a_flag_only_windows_has() {
        assert_eq!(
            parsed(&["--stop-for-restart"]),
            Err(ArgumentError::Unknown("--stop-for-restart".to_owned()))
        );
    }

    #[cfg(windows)]
    #[test]
    fn stopping_for_a_restart_is_a_request_of_its_own() {
        assert_eq!(parsed(&["--stop-for-restart"]), Ok(Request::StopForRestart));
    }
}
