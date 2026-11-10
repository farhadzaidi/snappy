//! Snappy's Objective-C delegate: the app's reopen handler, the menu bar menu's
//! delegate, and its items' action target. Every method only forwards.

use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSApplication, NSApplicationDelegate, NSMenu, NSMenuDelegate, NSMenuItem};
use snappy_platform::PlatformEvent;

use crate::events::emit;
use crate::tray;

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and `Delegate` has no `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SnappyDelegate"]
    pub struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl NSApplicationDelegate for Delegate {
        /// Opening Snappy.app while it runs opens the settings window.
        #[unsafe(method(applicationShouldHandleReopen:hasVisibleWindows:))]
        fn should_handle_reopen(&self, _app: &NSApplication, _visible: bool) -> bool {
            emit(PlatformEvent::OpenSettingsRequested);
            false
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
