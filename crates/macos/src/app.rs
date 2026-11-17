//! The main-thread side: NSApplication, the status item, and every observer that
//! feeds the engine.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSEvent, NSEventModifierFlags, NSEventType,
};
use objc2_core_foundation::CGPoint;
use snappy_platform::PlatformEvent;

use crate::delegate::Delegate;
use crate::error::MacError;
use crate::platform::MacPlatform;
use crate::windows::registry::Shared;
use crate::{
    apps, ax, caps_lock, desktop_guard, events, hotkeys, input, instance, main_thread, observers, overlay,
    permissions, tray,
};

/// Owns the main-thread machinery. Create it on the main thread before anything else.
pub struct App {
    mtm: MainThreadMarker,
    /// Keeps the app, menu, and menu items' delegate alive; AppKit holds it weakly.
    _delegate: Retained<Delegate>,
    windows: Shared<ax::AxElement>,
}

impl App {
    /// Sets up NSApplication with the accessory policy, so Snappy has no Dock icon, and
    /// starts delivering events to `events`. Refuses off the main thread.
    pub fn new(events: Sender<PlatformEvent>) -> Result<App, MacError> {
        let mtm = MainThreadMarker::new().ok_or(MacError::NotMainThread)?;
        let application = NSApplication::sharedApplication(mtm);
        application.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        let delegate = Delegate::new(mtm);
        application.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));

        events::connect(events);
        permissions::start();
        hotkeys::install_handler();
        tray::install(&delegate, mtm);

        let windows = Shared::default();
        observers::ax::connect(Shared::clone(&windows));
        for running in apps::all_running() {
            observers::ax::watch_app(&apps::facts(&running));
        }
        observers::system::start();
        instance::watch_termination();

        Ok(App {
            mtm,
            _delegate: delegate,
            windows,
        })
    }

    /// The engine-thread handle. Its calls that need a value from the main thread wait
    /// until `run` has started the run loop.
    pub fn platform(&self) -> MacPlatform {
        MacPlatform::new(Shared::clone(&self.windows))
    }

    /// Runs the NSApplication run loop until `request_termination`, then removes
    /// everything Snappy installed.
    pub fn run(self) {
        NSApplication::sharedApplication(self.mtm).run();
        teardown();
    }
}

/// Set while AppKit waits for the answer to `applicationShouldTerminate:`.
static TERMINATION_DEFERRED: AtomicBool = AtomicBool::new(false);

/// AppKit asked whether to quit, and waits for `request_termination` to answer.
pub(crate) fn defer_termination() {
    TERMINATION_DEFERRED.store(true, Ordering::SeqCst);
}

/// Asks the main thread to stop the run loop, or, when AppKit is waiting to quit, tells it
/// to go ahead. Callable from any thread.
pub fn request_termination() {
    main_thread::spawn(|mtm| {
        let application = NSApplication::sharedApplication(mtm);
        if TERMINATION_DEFERRED.swap(false, Ordering::SeqCst) {
            // Quitting this way ends the process without `run` returning, so its teardown
            // happens first.
            teardown();
            application.replyToApplicationShouldTerminate(true);
            return;
        }
        application.stop(None);
        // `stop` takes effect after the next event, so post one.
        let wake = NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
            NSEventType::ApplicationDefined,
            CGPoint::new(0.0, 0.0),
            NSEventModifierFlags::empty(),
            0.0,
            0,
            None,
            0,
            0,
            0,
        );
        if let Some(wake) = wake {
            application.postEvent_atStart(&wake, true);
        }
    });
}

fn teardown() {
    input::remove_all();
    hotkeys::unregister_all();
    caps_lock::stop();
    // The engine gives Caps Lock back before quitting; this covers an engine that panicked,
    // since the mapping outlives the process. The run loop is over, so nothing waits on it.
    caps_lock::unmap();
    permissions::set_polling(false);
    overlay::close_all();
    desktop_guard::clear();
    observers::system::stop();
    observers::ax::unwatch_all();
}
