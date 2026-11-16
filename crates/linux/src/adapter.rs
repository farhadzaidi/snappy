//! What the daemon asks of every Linux adapter's `App`, so it starts whichever one the session
//! needs the same way.

use std::sync::mpsc::Sender;

use snappy_platform::{Platform, PlatformEvent};

/// An adapter's event loop and the handles the engine thread gets from it.
pub trait Adapter: Sized {
    type Platform: Platform + Send + 'static;
    type Error: std::error::Error + Send + Sync + 'static;

    /// Connects to the session and starts listening; what happens goes to `events`.
    fn start(events: Sender<PlatformEvent>) -> Result<Self, Self::Error>;

    /// The engine thread's handle.
    fn platform(&self) -> Self::Platform;

    /// Asks a running `run` to return. Callable from any thread.
    fn stop_handle(&self) -> impl FnOnce() + Send + 'static;

    /// Handles events until stopped. Fails when the connection to the session goes away.
    fn run(self) -> Result<(), Self::Error>;
}
