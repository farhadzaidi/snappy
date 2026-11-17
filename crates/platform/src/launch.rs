//! How the daemon is started: its command-line flags, and the environment it reads. Login
//! items, the app, and the test harnesses start it with these.

/// Start without opening the app's window, as at login.
pub const BACKGROUND: &str = "--background";
/// Stop the running daemon the way installing a new build does, leaving every window where
/// it is, and wait for it to exit.
pub const STOP_FOR_RESTART: &str = "--stop-for-restart";
/// Quit the running daemon, bringing back every window it hid, then remove everything
/// Snappy added to the system and its files, leaving the program for whatever installed it.
pub const UNINSTALL: &str = "--uninstall";
/// With `UNINSTALL`, keep the settings file.
pub const KEEP_SETTINGS: &str = "--keep-settings";
/// What older macOS adds to the arguments of an app it launches. It isn't a flag of
/// Snappy's, and is ignored.
pub const PROCESS_SERIAL_NUMBER_PREFIX: &str = "-psn_";
/// Names a folder to hold every file instead of the standard locations.
pub const HOME_ENV: &str = "SNAPPY_HOME";
/// Set to 1 to turn the debug log on whatever the settings say.
pub const DEBUG_ENV: &str = "SNAPPY_DEBUG";
