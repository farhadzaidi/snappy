//! Running work on the main thread from the engine thread through the GCD main queue.
//!
//! The main thread never waits on the engine, so a synchronous dispatch can't deadlock.
//! Work submitted before `App::run` starts waits until the run loop drains the queue.

use std::time::Duration;

use dispatch2::{DispatchQueue, DispatchTime};
use objc2::MainThreadMarker;

/// Runs `work` on the main thread and returns its result.
pub fn sync<R: Send>(work: impl FnOnce(MainThreadMarker) -> R + Send) -> R {
    if let Some(mtm) = MainThreadMarker::new() {
        return work(mtm);
    }
    let mut result = None;
    DispatchQueue::main().exec_sync(|| result = Some(work(on_main())));
    result.unwrap_or_else(|| unreachable!("the main queue runs work before exec_sync returns"))
}

/// Queues `work` on the main thread without waiting.
pub fn spawn(work: impl FnOnce(MainThreadMarker) + Send + 'static) {
    DispatchQueue::main().exec_async(move || work(on_main()));
}

/// Queues `work` on the main thread after `delay`.
pub fn after(delay: Duration, work: impl FnOnce(MainThreadMarker) + Send + 'static) {
    let when = DispatchTime::NOW.time(delay.as_nanos().try_into().unwrap_or(i64::MAX));
    // `after` reports no error for a computed time; there is nothing to recover.
    let _ = DispatchQueue::main().after(when, move || work(on_main()));
}

/// A marker for code the main queue is running.
fn on_main() -> MainThreadMarker {
    // SAFETY: the GCD main queue only runs work on the main thread.
    unsafe { MainThreadMarker::new_unchecked() }
}
