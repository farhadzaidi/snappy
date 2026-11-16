//! Changing the size of the sandbox's display, the way connecting a display, or waking with
//! one, changes the displays Snappy works with. Only the macOS VM's display offers another
//! usable size; elsewhere there is none to change to.

use snappy_core::geometry::Size;

/// The main display's size, in points.
pub fn size() -> Option<Size> {
    platform::size()
}

/// The largest size the main display can show the desktop at below its current one.
pub fn smaller() -> Option<Size> {
    let current = size()?;
    platform::usable_sizes()
        .into_iter()
        .filter(|size| size.width < current.width && size.height < current.height)
        .max_by(|a, b| (a.width * a.height).total_cmp(&(b.width * b.height)))
}

/// Show the desktop on the main display at `size`, one of its usable sizes.
pub fn set(size: Size) -> Result<(), String> {
    platform::set(size)
}

#[cfg(target_os = "macos")]
mod platform {
    use objc2_core_foundation::{CFArray, CFRetained};
    use objc2_core_graphics::{
        CGDisplayCopyAllDisplayModes, CGDisplayCopyDisplayMode, CGDisplayMode, CGDisplaySetDisplayMode,
        CGError, CGMainDisplayID,
    };
    use snappy_core::geometry::Size;

    pub fn size() -> Option<Size> {
        CGDisplayCopyDisplayMode(CGMainDisplayID()).map(|mode| size_of(&mode))
    }

    pub fn usable_sizes() -> Vec<Size> {
        usable_modes().iter().map(|mode| size_of(mode)).collect()
    }

    pub fn set(size: Size) -> Result<(), String> {
        let mode = usable_modes()
            .into_iter()
            .find(|mode| size_of(mode) == size)
            .ok_or_else(|| format!("the display has no {}×{} mode", size.width, size.height))?;
        // SAFETY: no options are passed.
        let error = unsafe { CGDisplaySetDisplayMode(CGMainDisplayID(), Some(&mode), None) };
        if error == CGError::Success {
            Ok(())
        } else {
            Err(format!(
                "the display refused {}×{}: error {}",
                size.width, size.height, error.0
            ))
        }
    }

    fn usable_modes() -> Vec<CFRetained<CGDisplayMode>> {
        // SAFETY: no options are passed.
        let Some(modes) = (unsafe { CGDisplayCopyAllDisplayModes(CGMainDisplayID(), None) }) else {
            return Vec::new();
        };
        // SAFETY: the array CoreGraphics returns holds display modes.
        let modes = unsafe { CFRetained::cast_unchecked::<CFArray<CGDisplayMode>>(modes) };
        modes
            .iter()
            .filter(|mode| CGDisplayMode::is_usable_for_desktop_gui(Some(mode)))
            .collect()
    }

    /// Display modes measure the desktop in points.
    fn size_of(mode: &CGDisplayMode) -> Size {
        Size::new(
            CGDisplayMode::width(Some(mode)) as f64,
            CGDisplayMode::height(Some(mode)) as f64,
        )
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use snappy_core::geometry::Size;

    pub fn size() -> Option<Size> {
        None
    }

    pub fn usable_sizes() -> Vec<Size> {
        Vec::new()
    }

    pub fn set(_size: Size) -> Result<(), String> {
        Err("this sandbox's display can't change size".to_owned())
    }
}
