//! App lifecycle, Space, sleep, display, and Accessibility-trust notifications. Main
//! thread only; observers registered with no queue are called on the posting thread,
//! which for these notifications is the main thread.

use std::cell::RefCell;
use std::ptr::NonNull;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{
    NSApplicationDidChangeScreenParametersNotification, NSRunningApplication, NSWorkspace,
    NSWorkspaceActiveSpaceDidChangeNotification, NSWorkspaceApplicationKey,
    NSWorkspaceDidActivateApplicationNotification, NSWorkspaceDidLaunchApplicationNotification,
    NSWorkspaceDidTerminateApplicationNotification, NSWorkspaceWillSleepNotification,
};
use objc2_foundation::{
    NSDistributedNotificationCenter, NSNotification, NSNotificationCenter, NSNotificationName,
    NSObjectProtocol, ns_string,
};
use snappy_platform::PlatformEvent;

use super::ax;
use crate::apps;
use crate::events::emit;
use crate::permissions;

type Token = Retained<ProtocolObject<dyn NSObjectProtocol>>;

thread_local! {
    static TOKENS: RefCell<Vec<(Retained<NSNotificationCenter>, Token)>> = const { RefCell::new(Vec::new()) };
}

pub fn start() {
    let workspace = NSWorkspace::sharedWorkspace().notificationCenter();
    // SAFETY: reading immutable framework constants.
    let (activated, launched, terminated, space, sleep, screens) = unsafe {
        (
            NSWorkspaceDidActivateApplicationNotification,
            NSWorkspaceDidLaunchApplicationNotification,
            NSWorkspaceDidTerminateApplicationNotification,
            NSWorkspaceActiveSpaceDidChangeNotification,
            NSWorkspaceWillSleepNotification,
            NSApplicationDidChangeScreenParametersNotification,
        )
    };
    observe(&workspace, activated, app_activated);
    observe(&workspace, launched, app_launched);
    observe(&workspace, terminated, app_terminated);
    observe(&workspace, space, |_| emit(PlatformEvent::NativeDesktopChanged));
    observe(&workspace, sleep, |_| emit(PlatformEvent::WillSleep));
    observe(&NSNotificationCenter::defaultCenter(), screens, |_| {
        emit(PlatformEvent::DisplaysChanged)
    });
    let distributed: Retained<NSNotificationCenter> =
        Retained::into_super(NSDistributedNotificationCenter::defaultCenter());
    observe(&distributed, ns_string!("com.apple.accessibility.api"), |_| {
        permissions::recheck_soon()
    });
}

pub fn stop() {
    TOKENS.with(|tokens| {
        for (center, token) in tokens.borrow_mut().drain(..) {
            // SAFETY: the token came from this center.
            unsafe { center.removeObserver((*token).as_ref()) };
        }
    });
}

fn observe(center: &Retained<NSNotificationCenter>, name: &NSNotificationName, handler: fn(&NSNotification)) {
    let block = RcBlock::new(move |notification: NonNull<NSNotification>| {
        // SAFETY: the center passes a valid notification for the call's duration.
        handler(unsafe { notification.as_ref() });
    });
    // SAFETY: the block is 'static and only forwards events.
    let token = unsafe { center.addObserverForName_object_queue_usingBlock(Some(name), None, None, &block) };
    TOKENS.with(|tokens| tokens.borrow_mut().push((center.clone(), token)));
}

fn app_of(notification: &NSNotification) -> Option<Retained<NSRunningApplication>> {
    // SAFETY: reading an immutable framework constant.
    let key = unsafe { NSWorkspaceApplicationKey };
    notification
        .userInfo()?
        .objectForKey(key)?
        .downcast::<NSRunningApplication>()
        .ok()
}

fn pid_of(app: &NSRunningApplication) -> Option<u32> {
    u32::try_from(apps::pid(app)).ok()
}

/// Reported only when the activated app is still frontmost.
fn app_activated(notification: &NSNotification) {
    let Some(app) = app_of(notification) else { return };
    // Some apps become regular only after launch, too late to be watched then.
    ax::watch_app(&apps::facts(&app));
    let frontmost = apps::frontmost().map(|front| apps::pid(&front));
    if frontmost != Some(apps::pid(&app)) {
        return;
    }
    if let Some(pid) = pid_of(&app) {
        emit(PlatformEvent::FocusChanged {
            pid,
            by_activation: true,
        });
    }
}

fn app_launched(notification: &NSNotification) {
    let Some(app) = app_of(notification) else { return };
    ax::watch_app(&apps::facts(&app));
    if let Some(pid) = pid_of(&app) {
        emit(PlatformEvent::AppLaunched { pid });
    }
}

fn app_terminated(notification: &NSNotification) {
    let Some(app) = app_of(notification) else { return };
    let pid = apps::pid(&app);
    ax::unwatch_app(pid);
    ax::forget_process(pid);
    if let Some(pid) = pid_of(&app) {
        emit(PlatformEvent::AppTerminated { pid });
    }
}
