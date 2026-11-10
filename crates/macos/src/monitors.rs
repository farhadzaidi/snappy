//! Pointer and keyboard observation through NSEvent monitors.
//! Global monitors see other apps' events, local ones Snappy's own. Main thread only.

use std::cell::RefCell;
use std::ptr::NonNull;

use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{NSEvent, NSEventMask, NSEventType};
use snappy_core::geometry::Point;
use snappy_platform::PlatformEvent;

use crate::coords::flip_point;
use crate::displays::primary_height;
use crate::events::emit;
use crate::keycodes::mods_from_event_flags;

#[derive(Default)]
struct Monitors {
    press: Vec<Retained<AnyObject>>,
    drag: Vec<Retained<AnyObject>>,
}

thread_local! {
    static MONITORS: RefCell<Monitors> = RefCell::new(Monitors::default());
}

/// Primary-button presses and releases.
pub fn set_press_monitoring(on: bool, mtm: MainThreadMarker) {
    let mask = NSEventMask::LeftMouseDown | NSEventMask::LeftMouseUp;
    MONITORS.with(|monitors| toggle(&mut monitors.borrow_mut().press, on, mask, mtm));
}

/// Drags and modifier changes.
pub fn set_drag_monitoring(on: bool, mtm: MainThreadMarker) {
    let mask = NSEventMask::LeftMouseDragged | NSEventMask::FlagsChanged;
    MONITORS.with(|monitors| toggle(&mut monitors.borrow_mut().drag, on, mask, mtm));
}

fn toggle(installed: &mut Vec<Retained<AnyObject>>, on: bool, mask: NSEventMask, mtm: MainThreadMarker) {
    match (on, installed.is_empty()) {
        (true, true) => *installed = install(mask, mtm),
        (false, false) => remove(installed),
        _ => {}
    }
}

fn install(mask: NSEventMask, mtm: MainThreadMarker) -> Vec<Retained<AnyObject>> {
    let global = RcBlock::new(move |event: NonNull<NSEvent>| {
        // SAFETY: AppKit passes a valid event for the duration of the call.
        forward(unsafe { event.as_ref() }, mtm);
    });
    let local = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: as above.
        forward(unsafe { event.as_ref() }, mtm);
        event.as_ptr()
    });
    let global = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(mask, &global);
    // SAFETY: the handler returns the event unchanged, so delivery continues.
    let local = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &local) };
    global.into_iter().chain(local).collect()
}

fn remove(installed: &mut Vec<Retained<AnyObject>>) {
    for monitor in installed.drain(..) {
        // SAFETY: each object came from an add-monitor call and is removed once.
        unsafe { NSEvent::removeMonitor(&monitor) };
    }
}

/// Removes every monitor, at shutdown.
pub fn remove_all() {
    MONITORS.with(|monitors| {
        let mut monitors = monitors.borrow_mut();
        remove(&mut monitors.press);
        remove(&mut monitors.drag);
    });
}

fn forward(event: &NSEvent, mtm: MainThreadMarker) {
    let at = pointer(mtm);
    let mods = mods_from_event_flags(event.modifierFlags().0);
    let reported = match event.r#type() {
        NSEventType::LeftMouseDown => Some(PlatformEvent::PointerDown {
            at,
            mods,
            window: None,
        }),
        NSEventType::LeftMouseUp => Some(PlatformEvent::PointerUp { at, mods }),
        NSEventType::LeftMouseDragged => Some(PlatformEvent::PointerDragged { at, mods }),
        NSEventType::FlagsChanged => Some(PlatformEvent::ModifiersChanged { at, mods }),
        _ => None,
    };
    if let Some(reported) = reported {
        emit(reported);
    }
}

/// The pointer in top-left global coordinates.
pub fn pointer(mtm: MainThreadMarker) -> Point {
    let location = NSEvent::mouseLocation();
    flip_point(Point::new(location.x, location.y), primary_height(mtm))
}
