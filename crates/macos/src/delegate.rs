//! Snappy's Objective-C delegate: the app's launch, reopen, and quit handlers, the menu bar menu's
//! delegate, and its items' action target. Every method only forwards.

use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSApplicationDelegate, NSApplicationTerminateReply, NSMenu, NSMenuDelegate, NSMenuItem,
};
#[cfg(feature = "mac-app-store")]
use objc2_foundation::NSNotification;
use snappy_platform::PlatformEvent;

use crate::events::emit;
#[cfg(feature = "mac-app-store")]
use crate::login_launch;
use crate::{app, tray};

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and `Delegate` has no `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SnappyDelegate"]
    pub struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl NSApplicationDelegate for Delegate {
        /// Opening Snappy.app while it runs opens the app's window.
        #[unsafe(method(applicationShouldHandleReopen:hasVisibleWindows:))]
        fn should_handle_reopen(&self, _app: &NSApplication, _visible: bool) -> bool {
            emit(PlatformEvent::OpenAppRequested);
            false
        }

        /// A quit from outside, as an Apple Event sends at logout or from a script, quits as
        /// the menu's Quit does: the engine brings back hidden windows first, and
        /// `request_termination` gives AppKit its answer once it's done.
        #[unsafe(method(applicationShouldTerminate:))]
        fn should_terminate(&self, _app: &NSApplication) -> NSApplicationTerminateReply {
            app::defer_termination();
            emit(PlatformEvent::QuitRequested);
            NSApplicationTerminateReply::TerminateLater
        }

        /// The Mac App Store build has no launch agent to start it with `--background`, so
        /// opening it is reported here, unless macOS opened it at login.
        #[cfg(feature = "mac-app-store")]
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            if !login_launch::launched_at_login() {
                emit(PlatformEvent::OpenAppRequested);
            }
        }
    }

    unsafe impl NSMenuDelegate for Delegate {
        /// The menu is rebuilt every time it opens.
        #[unsafe(method(menuWillOpen:))]
        fn menu_will_open(&self, menu: &NSMenu) {
            tray::rebuild(menu, self, self.mtm());
        }
    }

    impl Delegate {
        #[unsafe(method(chooseMenuItem:))]
        fn choose_menu_item(&self, item: &NSMenuItem) {
            tray::choose(item.tag());
        }
    }
);

impl Delegate {
    pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
        // SAFETY: `init` on a freshly allocated NSObject subclass with no ivars.
        unsafe { msg_send![Self::alloc(mtm), init] }
    }
}
