//! What `snappy --uninstall` tells the person running it. What each adapter removes is
//! worded in the adapter's own messages.

use std::path::Path;

pub const NO_HOME: &str = "Snappy couldn't find your home folder.";
pub const QUIT: &str = "Quit Snappy.";
pub const NOT_RUNNING: &str = "Snappy wasn't running.";
pub const STILL_RUNNING: &str = "Snappy didn't quit. Quit it from its menu, then run this again.";
pub const SETTLING: &str = "Snappy didn't quit last time. Starting it once to put things back.";
pub const NOT_SETTLED: &str = "Snappy couldn't start to put things back.";
pub const STILL_HIDDEN: &str = "Some windows Snappy hid couldn't be brought back.";
pub const DONE: &str = "Snappy's changes and files are gone. Remove Snappy itself the way you installed it.";
pub const NOT_DONE: &str = "Some of Snappy's changes or files are still there.";

pub fn deleted(path: &Path) -> String {
    format!("Deleted {}.", path.display())
}

pub fn not_deleted(path: &Path, error: &std::io::Error) -> String {
    format!("Couldn't delete {}: {error}.", path.display())
}

pub fn kept_settings(path: &Path) -> String {
    format!("Kept your settings in {}.", path.display())
}
