//! Delivery of platform events to the engine. Main-thread callbacks only forward events;
//! they never call window APIs.

use std::cell::RefCell;
use std::sync::mpsc::Sender;

use snappy_platform::PlatformEvent;

thread_local! {
    static SINK: RefCell<Option<Sender<PlatformEvent>>> = const { RefCell::new(None) };
}

/// Installs the engine's channel on the main thread.
pub fn connect(sender: Sender<PlatformEvent>) {
    SINK.with(|sink| *sink.borrow_mut() = Some(sender));
}

/// Sends an event to the engine. Events after the engine has gone are dropped.
pub fn emit(event: PlatformEvent) {
    SINK.with(|sink| {
        if let Some(sender) = sink.borrow().as_ref() {
            let _ = sender.send(event);
        }
    });
}
