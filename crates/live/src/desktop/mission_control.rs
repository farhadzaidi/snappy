//! macOS: adding a desktop, going from one desktop to another, and removing the desktops
//! added, by pressing Mission Control's own buttons through Accessibility, as a user clicks
//! them. The VM keeps Mission Control's Control-arrow shortcuts off, since Snappy's
//! defaults use those chords.

use std::process::Command;
use std::ptr::{self, NonNull};

use objc2_core_foundation::{CFArray, CFRetained, CFString, CFType};

use crate::input::pause;

type AXUIElementRef = *const CFType;
type AXError = i32;

const AX_SUCCESS: AXError = 0;
const CHILDREN: &str = "AXChildren";
const IDENTIFIER: &str = "AXIdentifier";
const PRESS: &str = "AXPress";
const REMOVE_DESKTOP: &str = "AXRemoveDesktop";
/// Mission Control's identifiers for its list of desktops and the button that adds one.
const DESKTOP_LIST: &str = "mc.spaces.list";
const ADD_DESKTOP: &str = "mc.spaces.add";
/// Mission Control's buttons sit this deep below the Dock's application element.
const SEARCH_DEPTH: usize = 6;
/// Mission Control opens, and finishes going to a desktop, within this.
const ANIMATION_MS: u64 = 2500;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: &CFString,
        value: *mut *const CFType,
    ) -> AXError;
    fn AXUIElementPerformAction(element: AXUIElementRef, action: &CFString) -> AXError;
}

/// Opens Mission Control, adds a desktop, and goes to it.
pub fn open_new_desktop() -> Result<(), String> {
    let mission_control = open()?;
    mission_control
        .find(ADD_DESKTOP)
        .ok_or("Mission Control has no button to add a desktop")?
        .perform(PRESS)?;
    pause(ANIMATION_MS);
    let desktops = desktops(&mission_control)?;
    let last = desktops.last().ok_or("Mission Control lists no desktops")?;
    go(last)
}

/// Goes to the first desktop.
pub fn go_to_first_desktop() -> Result<(), String> {
    let mission_control = open()?;
    let desktops = desktops(&mission_control)?;
    go(desktops.first().ok_or("Mission Control lists no desktops")?)
}

/// Removes every desktop but the first, whose windows move to the first, and goes there.
pub fn remove_added_desktops() -> Result<(), String> {
    let mission_control = open()?;
    let added = desktops(&mission_control)?.len().saturating_sub(1);
    for _ in 0..added {
        if let [_, .., last] = desktops(&mission_control)?.as_slice() {
            last.perform(REMOVE_DESKTOP)?;
            pause(ANIMATION_MS);
        }
    }
    let desktops = desktops(&mission_control)?;
    if desktops.len() > 1 {
        return Err(format!("{} desktops are left", desktops.len()));
    }
    go(desktops.first().ok_or("Mission Control lists no desktops")?)
}

/// Opens Mission Control; the Dock draws it.
fn open() -> Result<Element, String> {
    Command::new("/usr/bin/open")
        .args(["-a", "Mission Control"])
        .status()
        .map_err(|error| format!("couldn't open Mission Control: {error}"))?;
    pause(ANIMATION_MS);
    Element::dock()
}

fn desktops(mission_control: &Element) -> Result<Vec<Element>, String> {
    mission_control
        .find(DESKTOP_LIST)
        .map(|list| list.children())
        .ok_or_else(|| "Mission Control shows no desktops".to_owned())
}

/// Pressing a desktop goes to it and closes Mission Control.
fn go(desktop: &Element) -> Result<(), String> {
    desktop.perform(PRESS)?;
    pause(ANIMATION_MS);
    Ok(())
}

/// The Dock's process ID, which draws Mission Control.
fn dock_pid() -> Result<i32, String> {
    let output = Command::new("/usr/bin/pgrep")
        .args(["-x", "Dock"])
        .output()
        .map_err(|error| format!("couldn't look for the Dock: {error}"))?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .and_then(|line| line.trim().parse().ok())
        .ok_or_else(|| "the Dock isn't running".to_owned())
}

/// An Accessibility element of the Dock's.
struct Element(CFRetained<CFType>);

impl Element {
    fn dock() -> Result<Element, String> {
        let pid = dock_pid()?;
        // SAFETY: the create function returns a +1 reference or null.
        let raw = unsafe { AXUIElementCreateApplication(pid) };
        NonNull::new(raw.cast_mut())
            // SAFETY: an owned reference, released when the element drops.
            .map(|raw| Element(unsafe { CFRetained::from_raw(raw) }))
            .ok_or_else(|| "couldn't reach the Dock through Accessibility".to_owned())
    }

    fn as_ptr(&self) -> AXUIElementRef {
        CFRetained::as_ptr(&self.0).as_ptr()
    }

    fn attribute(&self, name: &'static str) -> Option<CFRetained<CFType>> {
        let name = CFString::from_static_str(name);
        let mut value: *const CFType = ptr::null();
        // SAFETY: a live element, a name, and an out-pointer; the result is +1.
        let error = unsafe { AXUIElementCopyAttributeValue(self.as_ptr(), &name, &mut value) };
        if error != AX_SUCCESS {
            return None;
        }
        // SAFETY: on success the copy function stored a +1 reference, owned from here on.
        NonNull::new(value.cast_mut()).map(|value| unsafe { CFRetained::from_raw(value) })
    }

    fn children(&self) -> Vec<Element> {
        let Some(array) = self
            .attribute(CHILDREN)
            .and_then(|value| value.downcast::<CFArray>().ok())
        else {
            return Vec::new();
        };
        // SAFETY: `AXChildren` is an array of elements, which are CF objects.
        let array = unsafe { CFRetained::cast_unchecked::<CFArray<CFType>>(array) };
        array.to_vec().into_iter().map(Element).collect()
    }

    fn identifier(&self) -> Option<String> {
        let value = self.attribute(IDENTIFIER)?.downcast::<CFString>().ok()?;
        Some(value.to_string())
    }

    fn perform(&self, action: &'static str) -> Result<(), String> {
        let name = CFString::from_static_str(action);
        // SAFETY: a live element and an action name.
        let error = unsafe { AXUIElementPerformAction(self.as_ptr(), &name) };
        if error == AX_SUCCESS {
            Ok(())
        } else {
            Err(format!("Mission Control refused {action}: error {error}"))
        }
    }

    /// The element below this one with `identifier`, searching depth first.
    fn find(&self, identifier: &str) -> Option<Element> {
        self.find_within(identifier, SEARCH_DEPTH)
    }

    fn find_within(&self, identifier: &str, depth: usize) -> Option<Element> {
        self.children().into_iter().find_map(|child| {
            if child.identifier().as_deref() == Some(identifier) {
                Some(child)
            } else if depth > 1 {
                child.find_within(identifier, depth - 1)
            } else {
                None
            }
        })
    }
}
