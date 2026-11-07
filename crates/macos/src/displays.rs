//! Connected displays from `NSScreen`, in top-left coordinates. Main thread only.

use std::ptr::NonNull;

use objc2::MainThreadMarker;
use objc2_app_kit::NSScreen;
use objc2_core_foundation::{CFRetained, CFUUID, CGRect};
use objc2_foundation::{NSNumber, ns_string};
use snappy_core::geometry::Rect;
use snappy_platform::Display;

use crate::coords::flip_rect;
use crate::ffi::CGDisplayCreateUUIDFromDisplayID;

pub fn to_rect(rect: CGRect) -> Rect {
    Rect::new(rect.origin.x, rect.origin.y, rect.size.width, rect.size.height)
}

pub fn to_cg_rect(rect: Rect) -> CGRect {
    CGRect::new(
        objc2_core_foundation::CGPoint::new(rect.x, rect.y),
        objc2_core_foundation::CGSize::new(rect.width, rect.height),
    )
}

/// The primary display's height, which anchors every coordinate conversion.
pub fn primary_height(mtm: MainThreadMarker) -> f64 {
    NSScreen::screens(mtm)
        .firstObject()
        .map(|screen| screen.frame().size.height)
        .unwrap_or(0.0)
}

/// Every display in system order, primary first.
pub fn read(mtm: MainThreadMarker) -> Vec<Display> {
    let height = primary_height(mtm);
    NSScreen::screens(mtm)
        .iter()
        .map(|screen| {
            let (frame, work_area) =
                geometry(to_rect(screen.frame()), to_rect(screen.visibleFrame()), height);
            Display {
                id: display_id(&screen),
                name: screen.localizedName().to_string(),
                frame,
                work_area,
                scale: screen.backingScaleFactor(),
            }
        })
        .collect()
}

/// A display's frame and work area in top-left coordinates, from Cocoa's frame and
/// visible frame. The visible frame leaves out the menu bar and the Dock.
pub fn geometry(frame: Rect, visible_frame: Rect, primary_height: f64) -> (Rect, Rect) {
    (
        flip_rect(frame, primary_height),
        flip_rect(visible_frame, primary_height),
    )
}

/// The display's UUID, stable across reconnects, or `display-<number>`.
pub fn display_id(screen: &NSScreen) -> String {
    let number = screen_number(screen);
    uuid_string(number).unwrap_or_else(|| format!("display-{number}"))
}

fn screen_number(screen: &NSScreen) -> u32 {
    screen
        .deviceDescription()
        .objectForKey(ns_string!("NSScreenNumber"))
        .and_then(|value| value.downcast::<NSNumber>().ok())
        .map(|number| number.unsignedIntValue())
        .unwrap_or(0)
}

fn uuid_string(display: u32) -> Option<String> {
    // SAFETY: returns null or a +1 CFUUID.
    let raw = unsafe { CGDisplayCreateUUIDFromDisplayID(display) };
    let uuid = NonNull::new(raw.cast_mut())?;
    // SAFETY: the function above returns a CFUUID.
    let uuid: CFRetained<CFUUID> = unsafe { CFRetained::from_raw(uuid.cast()) };
    CFUUID::new_string(None, Some(&uuid)).map(|string| string.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIMARY: Rect = Rect::new(0.0, 0.0, 1440.0, 900.0);

    #[test]
    fn the_work_area_excludes_the_menu_bar_and_a_bottom_dock() {
        let visible = Rect::new(0.0, 70.0, 1440.0, 805.0);
        let (frame, work_area) = geometry(PRIMARY, visible, 900.0);
        assert_eq!(frame, PRIMARY);
        assert_eq!(work_area, Rect::new(0.0, 25.0, 1440.0, 805.0));
        assert_eq!(work_area.min_y() - frame.min_y(), 25.0, "menu bar");
        assert_eq!(frame.max_y() - work_area.max_y(), 70.0, "Dock");
    }

    #[test]
    fn the_work_area_excludes_a_side_dock() {
        let visible = Rect::new(64.0, 0.0, 1376.0, 875.0);
        let (_, work_area) = geometry(PRIMARY, visible, 900.0);
        assert_eq!(work_area, Rect::new(64.0, 25.0, 1376.0, 875.0));
    }

    #[test]
    fn a_secondary_display_keeps_its_own_menu_bar_out() {
        let cocoa_frame = Rect::new(1440.0, 0.0, 1920.0, 1080.0);
        let visible = Rect::new(1440.0, 0.0, 1920.0, 1055.0);
        let (frame, work_area) = geometry(cocoa_frame, visible, 900.0);
        assert_eq!(frame, Rect::new(1440.0, -180.0, 1920.0, 1080.0));
        assert_eq!(work_area, Rect::new(1440.0, -155.0, 1920.0, 1055.0));
    }
}
