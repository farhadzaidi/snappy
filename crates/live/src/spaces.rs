//! Adding a native desktop, going between desktops, and removing the added ones, so a
//! scenario can leave a window on a desktop that isn't showing. Only the macOS VM is driven
//! this way: Spaces are what make a window drop out of an app's Accessibility list there.

/// Add a desktop and go to it.
#[cfg(target_os = "macos")]
pub fn open_new() -> Result<(), String> {
    crate::desktop::mission_control::open_new_desktop()
}

/// Go back to the first desktop.
#[cfg(target_os = "macos")]
pub fn go_to_first() -> Result<(), String> {
    crate::desktop::mission_control::go_to_first_desktop()
}

/// Remove every desktop but the first, moving their windows there, and go there.
#[cfg(target_os = "macos")]
pub fn remove_added() -> Result<(), String> {
    crate::desktop::mission_control::remove_added_desktops()
}

#[cfg(not(target_os = "macos"))]
const NONE: &str = "this sandbox's desktops aren't driven by the live suite";

#[cfg(not(target_os = "macos"))]
pub fn open_new() -> Result<(), String> {
    Err(NONE.to_owned())
}

#[cfg(not(target_os = "macos"))]
pub fn go_to_first() -> Result<(), String> {
    Err(NONE.to_owned())
}

#[cfg(not(target_os = "macos"))]
pub fn remove_added() -> Result<(), String> {
    Err(NONE.to_owned())
}
