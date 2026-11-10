//! Running applications, read through `NSRunningApplication` and `NSWorkspace`. These
//! reads are thread-safe and used from both threads.

use objc2::ClassType;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2_app_kit::{NSApplicationActivationPolicy, NSRunningApplication, NSWorkspace};

use crate::eligibility::{AppFacts, app_identifier};
use crate::ffi::{NO_SUCH_PROCESS, Pid, kill};

pub fn own_pid() -> Pid {
    std::process::id() as Pid
}

/// The app's process ID. Sent directly so the bindings' `libc` feature isn't needed.
pub fn pid(app: &NSRunningApplication) -> Pid {
    // SAFETY: `processIdentifier` returns a `pid_t`, which is an `i32` on macOS.
    unsafe { msg_send![app, processIdentifier] }
}

/// The app with `pid`, terminated or not.
fn lookup(pid: Pid) -> Option<Retained<NSRunningApplication>> {
    // SAFETY: the class method takes a `pid_t` and returns an autoreleased app or nil.
    unsafe { msg_send![NSRunningApplication::class(), runningApplicationWithProcessIdentifier: pid] }
}

pub fn facts(app: &NSRunningApplication) -> AppFacts {
    let pid = pid(app);
    let bundle_id = app.bundleIdentifier().map(|id| id.to_string());
    let executable = app
        .executableURL()
        .and_then(|url| url.lastPathComponent())
        .map(|name| name.to_string());
    AppFacts {
        pid,
        identifier: app_identifier(bundle_id.as_deref(), executable.as_deref(), pid),
        bundle_id,
        regular: app.activationPolicy() == NSApplicationActivationPolicy::Regular,
        hidden: app.isHidden(),
        is_self: pid == own_pid(),
    }
}

/// The live app for `pid`, or `None` once it has terminated.
pub fn running(pid: Pid) -> Option<Retained<NSRunningApplication>> {
    lookup(pid).filter(|app| !app.isTerminated())
}

pub fn all_running() -> Vec<Retained<NSRunningApplication>> {
    NSWorkspace::sharedWorkspace().runningApplications().to_vec()
}

pub fn frontmost() -> Option<Retained<NSRunningApplication>> {
    NSWorkspace::sharedWorkspace().frontmostApplication()
}

/// Seconds since 1970 at which the process started, from the kernel. Unlike
/// `NSRunningApplication.launchDate`, it exists for apps started from a terminal too.
pub fn launch_time(pid: i32) -> Option<f64> {
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let size = i32::try_from(std::mem::size_of::<libc::proc_bsdinfo>()).ok()?;
    // SAFETY: the buffer is a zeroed `proc_bsdinfo` of exactly the size passed.
    let written =
        unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 0, info.as_mut_ptr().cast(), size) };
    if written != size {
        return None;
    }
    // SAFETY: the kernel filled the whole structure.
    let info = unsafe { info.assume_init() };
    Some(info.pbi_start_tvsec as f64 + info.pbi_start_tvusec as f64 / 1_000_000.0)
}

/// False once the process is gone, even if AppKit hasn't noticed yet.
pub fn is_running(pid: Pid) -> bool {
    if running(pid).is_none() {
        return false;
    }
    // SAFETY: signal 0 only checks that the process exists.
    let alive = unsafe { kill(pid, 0) } == 0;
    alive || std::io::Error::last_os_error().raw_os_error() != Some(NO_SUCH_PROCESS)
}
