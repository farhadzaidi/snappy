//! The window server's list of on-screen windows. Only owner, layer, and bounds are
//! read: window names would need Screen Recording access, and Snappy never reads titles.

use objc2_core_foundation::{CFArray, CFDictionary, CFNumber, CFRetained, CFString, CFType, CGRect};
use objc2_core_graphics::{
    CGRectMakeWithDictionaryRepresentation, CGWindowListCopyWindowInfo, CGWindowListOption, kCGNullWindowID,
    kCGWindowBounds, kCGWindowLayer, kCGWindowOwnerPID,
};

use crate::displays::to_rect;
use crate::occupancy::ServerWindow;

type Row = CFDictionary<CFString, CFType>;

/// On-screen windows front to back, or `None` when the list can't be read.
pub fn on_screen_windows() -> Option<Vec<ServerWindow>> {
    let options = CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements;
    let rows = CGWindowListCopyWindowInfo(options, kCGNullWindowID)?;
    // SAFETY: the window list is an array of string-keyed dictionaries.
    let rows = unsafe { CFRetained::cast_unchecked::<CFArray<Row>>(rows) };
    Some(rows.iter().filter_map(|row| parse(&row)).collect())
}

fn parse(row: &Row) -> Option<ServerWindow> {
    // SAFETY: reading immutable framework constants.
    let (pid_key, layer_key, bounds_key) = unsafe { (kCGWindowOwnerPID, kCGWindowLayer, kCGWindowBounds) };
    let number = |key: &CFString| row.get(key)?.downcast::<CFNumber>().ok();
    let pid = number(pid_key)?.as_i32()?;
    let layer = number(layer_key)?.as_i64()?;
    let bounds = row.get(bounds_key)?.downcast::<CFDictionary>().ok()?;
    let mut frame = CGRect::default();
    // SAFETY: valid dictionary and out-pointer.
    let parsed = unsafe { CGRectMakeWithDictionaryRepresentation(Some(&bounds), &mut frame) };
    parsed.then(|| ServerWindow {
        pid,
        layer,
        frame: to_rect(frame),
    })
}
