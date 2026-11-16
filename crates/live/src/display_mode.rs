//! Changing the size of the sandbox's display, the way connecting a display, or waking with
//! one, changes the displays Snappy works with. Only the macOS VM's display offers another
//! usable size; elsewhere there is none to change to.

use snappy_core::geometry::Size;

/// The main display's size, in points.
#[cfg(target_os = "macos")]
pub fn size() -> Option<Size> {
    crate::desktop::macos::display_size()
}

#[cfg(not(target_os = "macos"))]
pub fn size() -> Option<Size> {
    None
}

/// The largest size the main display can show the desktop at below its current one.
pub fn smaller() -> Option<Size> {
    let current = size()?;
    usable_sizes()
        .into_iter()
        .filter(|size| size.width < current.width && size.height < current.height)
        .max_by(|a, b| (a.width * a.height).total_cmp(&(b.width * b.height)))
}

/// Show the desktop on the main display at `size`, one of its usable sizes.
#[cfg(target_os = "macos")]
pub fn set(size: Size) -> Result<(), String> {
    crate::desktop::macos::set_display_size(size)
}

#[cfg(not(target_os = "macos"))]
pub fn set(_size: Size) -> Result<(), String> {
    Err("this sandbox's display can't change size".to_owned())
}

#[cfg(target_os = "macos")]
fn usable_sizes() -> Vec<Size> {
    crate::desktop::macos::usable_display_sizes()
}

#[cfg(not(target_os = "macos"))]
fn usable_sizes() -> Vec<Size> {
    Vec::new()
}
