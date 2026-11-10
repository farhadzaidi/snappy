//! The main-thread side: NSApplication, the status item, and every observer that
//! feeds the engine.

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
use crate::platform::MacPlatform;
use crate::registry::Shared;
use crate::{
    apps, ax, desktop_guard, events, hotkeys, main_thread, monitors, observers, overlay, permissions,
    signals, tray,
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
    /// starts delivering events to `events`.
    ///
    /// # Panics
    /// When called off the main thread.
    pub fn new(events: Sender<PlatformEvent>) -> App {
        let Some(mtm) = MainThreadMarker::new() else {
            panic!("snappy_macos::App must be created on the main thread");
        };
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
        signals::watch_termination();

        App {
            mtm,
            _delegate: delegate,
            windows,
        }
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

/// Asks the main thread to stop the run loop. Callable from any thread.
pub fn request_termination() {
    main_thread::spawn(|mtm| {
        let application = NSApplication::sharedApplication(mtm);
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
    monitors::remove_all();
    hotkeys::unregister_all();
    permissions::set_polling(false);
    overlay::close_all();
    desktop_guard::clear();
    observers::system::stop();
    observers::ax::unwatch_all();
}
