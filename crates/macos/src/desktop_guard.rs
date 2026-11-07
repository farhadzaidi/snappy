//! Recognizing Snappy's own Space with public APIs only, so workspaces pause on any
//! other Space: one invisible, click-through 1×1 anchor per display, ordered once so it
//! joins the current Space. Main thread only.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSBackingStoreType, NSColor, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use snappy_platform::Display;

use crate::coords::flip_rect;
use crate::displays::primary_height;

thread_local! {
    static ANCHORS: RefCell<Vec<Retained<NSWindow>>> = const { RefCell::new(Vec::new()) };
}

/// Places fresh anchors on the current Space, one per display.
pub fn bind(displays: &[Display], mtm: MainThreadMarker) {
    clear();
    let height = primary_height(mtm);
    let anchors = displays
        .iter()
        .map(|display| {
            let area = flip_rect(display.work_area, height);
            anchor(CGPoint::new(area.x + 1.0, area.y + 1.0), mtm)
        })
        .collect();
    ANCHORS.with(|slot| *slot.borrow_mut() = anchors);
}

/// True while every anchor is on the active Space.
pub fn on_managed_desktop() -> bool {
    ANCHORS.with(|anchors| {
        let anchors = anchors.borrow();
        !anchors.is_empty() && anchors.iter().all(|anchor| anchor.isOnActiveSpace())
    })
}

pub fn clear() {
    ANCHORS.with(|anchors| {
        for anchor in anchors.borrow_mut().drain(..) {
            anchor.close();
        }
    });
}

fn anchor(origin: CGPoint, mtm: MainThreadMarker) -> Retained<NSWindow> {
    let frame = CGRect::new(origin, CGSize::new(1.0, 1.0));
    // SAFETY: standard designated initializer.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            frame,
            NSWindowStyleMask::Borderless,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: anchors are kept in `ANCHORS` until closed, never released on close.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setCollectionBehavior(
        NSWindowCollectionBehavior::Stationary | NSWindowCollectionBehavior::IgnoresCycle,
    );
    window.setIgnoresMouseEvents(true);
    window.setHasShadow(false);
    window.setOpaque(false);
    window.setBackgroundColor(Some(&NSColor::clearColor()));
    window.setAlphaValue(0.0);
    window.orderBack(None);
    window
}
