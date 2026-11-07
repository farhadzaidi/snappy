//! Accessibility (HIServices) declarations, as documented in `AXUIElement.h`,
//! `AXValue.h`, and `AXError.h`.
//!
//! Every `Create`/`Copy` function returns a +1 reference the caller releases. Element and
//! observer references are Core Foundation objects and may be used from any thread.

use std::ffi::c_void;

use objc2_core_foundation::{CFArray, CFDictionary, CFRunLoopSource, CFString, CFType, CFTypeID};

use super::{Boolean, Pid};

pub type AXError = i32;
pub type AXUIElementRef = *const CFType;
pub type AXObserverRef = *const CFType;
pub type AXValueRef = *const CFType;
pub type AXValueType = u32;

pub const AX_VALUE_CG_POINT: AXValueType = 1;
pub const AX_VALUE_CG_SIZE: AXValueType = 2;

/// Called on the run loop the observer's source was added to.
pub type AXObserverCallback = unsafe extern "C" fn(
    observer: AXObserverRef,
    element: AXUIElementRef,
    notification: *const CFString,
    refcon: *mut c_void,
);

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    pub fn AXIsProcessTrusted() -> Boolean;
    pub fn AXIsProcessTrustedWithOptions(options: *const CFDictionary) -> Boolean;

    pub fn AXUIElementGetTypeID() -> CFTypeID;
    pub fn AXUIElementCreateApplication(pid: Pid) -> AXUIElementRef;
    pub fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    pub fn AXUIElementCopyElementAtPosition(
        application: AXUIElementRef,
        x: f32,
        y: f32,
        element: *mut AXUIElementRef,
    ) -> AXError;
    pub fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: &CFString,
        value: *mut *const CFType,
    ) -> AXError;
    pub fn AXUIElementCopyMultipleAttributeValues(
        element: AXUIElementRef,
        attributes: &CFArray,
        options: u32,
        values: *mut *const CFArray,
    ) -> AXError;
    pub fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: &CFString,
        value: &CFType,
    ) -> AXError;
    pub fn AXUIElementIsAttributeSettable(
        element: AXUIElementRef,
        attribute: &CFString,
        settable: *mut Boolean,
    ) -> AXError;
    pub fn AXUIElementPerformAction(element: AXUIElementRef, action: &CFString) -> AXError;
    pub fn AXUIElementGetPid(element: AXUIElementRef, pid: *mut Pid) -> AXError;
    pub fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, seconds: f32) -> AXError;

    pub fn AXValueGetTypeID() -> CFTypeID;
    pub fn AXValueCreate(value_type: AXValueType, value: *const c_void) -> AXValueRef;
    pub fn AXValueGetType(value: AXValueRef) -> AXValueType;
    pub fn AXValueGetValue(value: AXValueRef, value_type: AXValueType, out: *mut c_void) -> Boolean;

    pub fn AXObserverCreate(pid: Pid, callback: AXObserverCallback, observer: *mut AXObserverRef) -> AXError;
    pub fn AXObserverAddNotification(
        observer: AXObserverRef,
        element: AXUIElementRef,
        notification: &CFString,
        refcon: *mut c_void,
    ) -> AXError;
    /// Returns a +0 reference owned by the observer.
    pub fn AXObserverGetRunLoopSource(observer: AXObserverRef) -> *const CFRunLoopSource;
}
