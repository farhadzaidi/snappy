//! IOKit declarations for the HID event system's key mapping and the Caps Lock state, as
//! documented in `hidsystem/IOHIDEventSystemClient.h`, `IOHIDLib.h`, `IOHIDShared.h`, and
//! `IOKitLib.h`.
//!
//! Every `Create`/`Copy` function returns a +1 reference the caller releases.

use objc2_core_foundation::{CFDictionary, CFString, CFType};

use super::Boolean;

pub type IOHIDEventSystemClientRef = *const CFType;
pub type MachPort = u32;
pub type IoObject = MachPort;
pub type IoConnect = MachPort;
pub type KernReturn = i32;

pub const KERN_SUCCESS: KernReturn = 0;
/// `kIOMainPortDefault`: the default port, `MACH_PORT_NULL`.
pub const MAIN_PORT_DEFAULT: MachPort = 0;
/// `kIOHIDSystemClass`.
pub const HID_SYSTEM_CLASS: &std::ffi::CStr = c"IOHIDSystem";
/// `kIOHIDParamConnectType`.
pub const HID_PARAM_CONNECT_TYPE: u32 = 1;
/// `kIOHIDCapsLockState`.
pub const HID_CAPS_LOCK_STATE: i32 = 1;

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    pub fn IOHIDEventSystemClientCreateSimpleClient(allocator: *const CFType) -> IOHIDEventSystemClientRef;
    pub fn IOHIDEventSystemClientCopyProperty(
        client: IOHIDEventSystemClientRef,
        key: &CFString,
    ) -> *const CFType;
    pub fn IOHIDEventSystemClientSetProperty(
        client: IOHIDEventSystemClientRef,
        key: &CFString,
        property: &CFType,
    ) -> Boolean;

    pub fn IOServiceMatching(name: *const std::ffi::c_char) -> *const CFDictionary;
    /// Consumes one reference to `matching`.
    pub fn IOServiceGetMatchingService(main_port: MachPort, matching: *const CFDictionary) -> IoObject;
    pub fn IOServiceOpen(
        service: IoObject,
        owning_task: MachPort,
        kind: u32,
        connect: *mut IoConnect,
    ) -> KernReturn;
    pub fn IOServiceClose(connect: IoConnect) -> KernReturn;
    pub fn IOObjectRelease(object: IoObject) -> KernReturn;
    pub fn IOHIDGetModifierLockState(handle: IoConnect, selector: i32, state: *mut bool) -> KernReturn;
    pub fn IOHIDSetModifierLockState(handle: IoConnect, selector: i32, state: bool) -> KernReturn;
}

unsafe extern "C" {
    /// This task's port, which `mach_task_self()` reads.
    pub static mach_task_self_: MachPort;
}
