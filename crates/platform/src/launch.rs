//! How the daemon is started: its command-line flags, and the environment it reads. Login
//! items, the app, and the test harnesses start it with these.

/// Start without opening the app's window, as at login.
pub const BACKGROUND: &str = "--background";
/// Stop the running daemon the way installing a new build does, leaving every window where
/// it is, and wait for it to exit.
pub const STOP_FOR_RESTART: &str = "--stop-for-restart";
/// Names a folder to hold every file instead of the standard locations.
pub const HOME_ENV: &str = "SNAPPY_HOME";
/// Set to 1 to turn the debug log on whatever the settings say.
pub const DEBUG_ENV: &str = "SNAPPY_DEBUG";
