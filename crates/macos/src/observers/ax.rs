//! Accessibility notifications: window created and focused window changed per app, and
//! moved, resized, deminiaturized, and destroyed per tracked window.
//!
//! Observers and their run loop sources are created on the main thread, so callbacks
//! arrive there. Adding a notification is a round trip to the target app, so app-level
//! registrations run on a background queue and window-level ones on the engine thread.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::c_void;
use std::ptr::{self, NonNull};
use std::sync::{Mutex, PoisonError};

use dispatch2::{DispatchQoS, DispatchQueue, GlobalQueueIdentifier};
use objc2_core_foundation::{CFRetained, CFRunLoop, CFString, CFType, kCFRunLoopCommonModes};
use snappy_platform::{PlatformEvent, WindowId};

use crate::ax::{AxElement, AxError, names};
use crate::events::emit;
use crate::ffi::Pid;
use crate::ffi::ax::{
    AXObserverAddNotification, AXObserverCreate, AXObserverGetRunLoopSource, AXObserverRef,
    AXUIElementGetPid, AXUIElementRef,
};
use crate::main_thread;
use crate::thresholds::AX_MESSAGING_TIMEOUT;
use crate::windows::eligibility::AppFacts;
use crate::windows::registry::{self, Shared};

const APP_NOTIFICATIONS: [&str; 2] = [names::WINDOW_CREATED, names::FOCUSED_WINDOW_CHANGED];
const WINDOW_NOTIFICATIONS: [&str; 4] = [
    names::ELEMENT_DESTROYED,
    names::MOVED,
    names::RESIZED,
    names::WINDOW_DEMINIATURIZED,
];

#[derive(Clone)]
struct Observer(CFRetained<CFType>);

// SAFETY: AXObservers are Core Foundation objects; adding notifications is thread-safe,
// and the run loop source is only touched on the main thread.
unsafe impl Send for Observer {}

impl Observer {
    fn create(pid: Pid) -> Option<Self> {
        let mut raw: AXObserverRef = ptr::null();
        // SAFETY: valid callback and out-pointer; the result is +1.
        AxError::check(unsafe { AXObserverCreate(pid, on_notification, &mut raw) }).ok()?;
        // SAFETY: the create function returned a +1 reference, owned from here on.
        NonNull::new(raw.cast_mut()).map(|raw| Self(unsafe { CFRetained::from_raw(raw) }))
    }

    fn as_ptr(&self) -> AXObserverRef {
        CFRetained::as_ptr(&self.0).as_ptr()
    }

    /// Delivers callbacks on the main run loop, or stops them. Main thread.
    fn schedule(&self, on: bool) {
        // SAFETY: the observer owns its source for as long as it lives.
        let source = unsafe { AXObserverGetRunLoopSource(self.as_ptr()).as_ref() };
        // SAFETY: reading an immutable framework constant.
        let mode = unsafe { kCFRunLoopCommonModes };
        let Some(main) = CFRunLoop::main() else { return };
        if on {
            main.add_source(source, mode);
        } else {
            main.remove_source(source, mode);
        }
    }

    fn add(&self, element: &AxElement, name: &'static str, refcon: *mut c_void) -> Result<(), AxError> {
        let name = CFString::from_static_str(name);
        // SAFETY: valid observer, element, and name. The refcon is an integer, never dereferenced.
        AxError::check(unsafe { AXObserverAddNotification(self.as_ptr(), element.as_ptr(), &name, refcon) })
    }
}

static OBSERVERS: Mutex<BTreeMap<Pid, Observer>> = Mutex::new(BTreeMap::new());

thread_local! {
    static WINDOWS: RefCell<Option<Shared<AxElement>>> = const { RefCell::new(None) };
}

fn observers() -> std::sync::MutexGuard<'static, BTreeMap<Pid, Observer>> {
    OBSERVERS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Gives destroyed-window callbacks the registry to prune. Main thread.
pub fn connect(windows: Shared<AxElement>) {
    WINDOWS.with(|slot| *slot.borrow_mut() = Some(windows));
}

/// Starts observing a supported app. Main thread.
pub fn watch_app(app: &AppFacts) {
    if !app.is_supported() || observers().contains_key(&app.pid) {
        return;
    }
    let Some(observer) = Observer::create(app.pid) else {
        return;
    };
    observer.schedule(true);
    observers().insert(app.pid, observer.clone());

    let pid = app.pid;
    let queue = DispatchQueue::global_queue(GlobalQueueIdentifier::QualityOfService(
        DispatchQoS::UserInitiated,
    ));
    queue.exec_async(move || {
        // An app still starting up may not answer yet. Forgetting it lets its next
        // activation try again; a notification the app doesn't offer never will be.
        let watched = AxElement::application(pid, AX_MESSAGING_TIMEOUT).is_some_and(|application| {
            APP_NOTIFICATIONS
                .into_iter()
                .filter_map(|name| observer.add(&application, name, ptr::null_mut()).err())
                .all(AxError::is_unsupported_notification)
        });
        if !watched {
            main_thread::spawn(move |_| unwatch_app(pid));
        }
    });
}

/// Stops observing an app. Main thread.
pub fn unwatch_app(pid: Pid) {
    if let Some(observer) = observers().remove(&pid) {
        observer.schedule(false);
    }
}

/// Stops observing every app, at shutdown. Main thread.
pub fn unwatch_all() {
    for observer in std::mem::take(&mut *observers()).into_values() {
        observer.schedule(false);
    }
}

/// Reports changes to a newly tracked window. Engine thread.
pub fn watch_window(pid: Pid, element: &AxElement, id: WindowId) {
    let Some(observer) = observers().get(&pid).cloned() else {
        return;
    };
    // Window IDs start at 1, so a null refcon always means an app-level notification.
    let refcon = id.0 as usize as *mut c_void;
    for name in WINDOW_NOTIFICATIONS {
        // A window that refuses one still reports the others.
        let _ = observer.add(element, name, refcon);
    }
}

unsafe extern "C" fn on_notification(
    _observer: AXObserverRef,
    element: AXUIElementRef,
    notification: *const CFString,
    refcon: *mut c_void,
) {
    // SAFETY: Accessibility passes a valid notification name for the call's duration.
    let Some(name) = (unsafe { notification.as_ref() }) else {
        return;
    };
    let name = name.to_string();
    let event = match refcon as usize as u64 {
        0 => app_event(&name, element),
        id => window_event(&name, WindowId(id)),
    };
    if let Some(event) = event {
        emit(event);
    }
}

fn app_event(name: &str, element: AXUIElementRef) -> Option<PlatformEvent> {
    let mut pid: Pid = 0;
    // SAFETY: a local lookup on a valid element; no message is sent to the app.
    AxError::check(unsafe { AXUIElementGetPid(element, &mut pid) }).ok()?;
    let pid = u32::try_from(pid).ok()?;
    match name {
        names::WINDOW_CREATED => Some(PlatformEvent::WindowCreated { pid }),
        names::FOCUSED_WINDOW_CHANGED => Some(PlatformEvent::FocusChanged {
            pid,
            by_activation: false,
        }),
        _ => None,
    }
}

fn window_event(name: &str, id: WindowId) -> Option<PlatformEvent> {
    match name {
        names::ELEMENT_DESTROYED => {
            WINDOWS.with(|windows| {
                if let Some(windows) = windows.borrow().as_ref() {
                    registry::lock(windows).remove(id);
                }
            });
            Some(PlatformEvent::WindowDestroyed(id))
        }
        names::MOVED | names::RESIZED | names::WINDOW_DEMINIATURIZED => Some(PlatformEvent::WindowMoved(id)),
        _ => None,
    }
}

/// Forgets every window of a terminated app. Main thread.
pub fn forget_process(pid: Pid) {
    WINDOWS.with(|windows| {
        if let Some(windows) = windows.borrow().as_ref() {
            registry::lock(windows).remove_process(pid);
        }
    });
}
