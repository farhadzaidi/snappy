//! The engine thread: one Tokio `LocalSet` running the engine until quit.

use std::sync::mpsc::Receiver;
use std::thread::JoinHandle;

use snappy_engine::{Engine, Input, Paths};
use snappy_platform::{Platform, PlatformEvent, launch};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::app_process::AppProcess;

/// Start the engine on a thread of its own, fed by the adapter's `events`. `open_app`
/// opens the app's window at launch.
///
/// `stop_adapter` ends the adapter's loop once the engine is done, however it ends. After a
/// panic no engine is left to put anything back, so the daemon exits and its next start picks
/// up from the saved state, as after a crash, instead of living on with the adapter still
/// holding windows and keys.
pub fn spawn_engine<P: Platform + Send + 'static>(
    platform: P,
    paths: Paths,
    events: Receiver<PlatformEvent>,
    open_app: bool,
    stop_adapter: impl FnOnce() + Send + 'static,
) -> std::io::Result<JoinHandle<std::io::Result<()>>> {
    let (inputs, incoming) = tokio::sync::mpsc::unbounded_channel();
    forward_events(events, inputs.clone());
    std::thread::Builder::new()
        .name("snappy-engine".into())
        .spawn(move || {
            let _stop_adapter = OnDrop::new(stop_adapter);
            run_engine(platform, paths, inputs, incoming, open_app)
        })
}

/// Wait for the engine thread to end, turning a panic into an error.
pub fn join_engine(engine: JoinHandle<std::io::Result<()>>) -> std::io::Result<()> {
    engine
        .join()
        .map_err(|_| std::io::Error::other("engine thread panicked"))?
}

/// Runs a closure when dropped, which a panic unwinding the thread does too.
struct OnDrop<F: FnOnce()>(Option<F>);

impl<F: FnOnce()> OnDrop<F> {
    fn new(run: F) -> Self {
        Self(Some(run))
    }
}

impl<F: FnOnce()> Drop for OnDrop<F> {
    fn drop(&mut self) {
        if let Some(run) = self.0.take() {
            run();
        }
    }
}

/// Run the engine on the current thread until the user quits.
fn run_engine<P: Platform + 'static>(
    platform: P,
    paths: Paths,
    inputs: UnboundedSender<Input>,
    mut incoming: UnboundedReceiver<Input>,
    open_app: bool,
) -> std::io::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    let local = tokio::task::LocalSet::new();
    local.block_on(&runtime, async move {
        let mut app_process = AppProcess::new(inputs.clone());
        let engine = Engine::new(platform, paths);
        if std::env::var_os(launch::DEBUG_ENV).is_some_and(|value| value == "1") {
            engine.force_debug_log();
        }
        engine.start();
        if open_app {
            let _ = inputs.send(Input::Platform(PlatformEvent::OpenAppRequested));
        }

        let outbox_ready = engine.outbox_ready();
        loop {
            // Messages also come from work that finishes between inputs, such as a snap
            // that settles or fails, so a queued message wakes the loop too.
            let input = tokio::select! {
                input = incoming.recv() => match input {
                    Some(input) => Some(input),
                    None => break,
                },
                () = outbox_ready.notified() => None,
            };
            let Some(input) = input else {
                app_process.send(engine.take_outbox());
                continue;
            };
            let quit = matches!(
                input,
                Input::Platform(PlatformEvent::QuitRequested | PlatformEvent::RestartRequested)
            );
            if input == Input::Platform(PlatformEvent::OpenAppRequested)
                && let Err(error) = app_process.open()
            {
                eprintln!("snappy: couldn't open the app: {error}");
            }
            engine.handle(input);
            app_process.send(engine.take_outbox());
            if quit {
                engine.idle().await;
                break;
            }
        }
    });
    Ok(())
}

/// Bridge events from the platform's main thread into the engine's channel.
fn forward_events(events: Receiver<PlatformEvent>, inputs: UnboundedSender<Input>) {
    std::thread::spawn(move || {
        for event in events {
            if inputs.send(Input::Platform(event)).is_err() {
                return;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::OnDrop;

    fn counting() -> (Arc<AtomicUsize>, impl FnOnce() + Send + 'static) {
        let runs = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&runs);
        (runs, move || {
            counter.fetch_add(1, Ordering::SeqCst);
        })
    }

    #[test]
    fn the_adapter_stops_when_the_engine_thread_panics() {
        let (stops, stop) = counting();
        let engine = std::thread::spawn(move || {
            let _stop = OnDrop::new(stop);
            panic!("the engine failed");
        });
        assert!(engine.join().is_err());
        assert_eq!(stops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn the_adapter_stops_once_when_the_engine_returns() {
        let (stops, stop) = counting();
        let engine = std::thread::spawn(move || {
            let _stop = OnDrop::new(stop);
        });
        assert!(engine.join().is_ok());
        assert_eq!(stops.load(Ordering::SeqCst), 1);
    }
}
