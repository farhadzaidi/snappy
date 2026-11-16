//! The engine thread: one Tokio `LocalSet` running the engine until quit.

use std::sync::mpsc::Receiver;
use std::thread::JoinHandle;

use snappy_engine::{Engine, Input, Paths};
use snappy_platform::{Platform, PlatformEvent};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::settings_app::SettingsApp;

/// Set to `1` to record the debug log for this run regardless of the setting.
const DEBUG_LOG_OVERRIDE: &str = "SNAPPY_DEBUG";

/// Start the engine on a thread of its own, fed by the adapter's `events`. `open_settings`
/// opens the settings window at launch.
///
/// `stop_adapter` ends the adapter's loop once the engine is done, however it ends. After a
/// panic no engine is left to put anything back, so the daemon exits and its next start picks
/// up from the saved state, as after a crash, instead of living on with the adapter still
/// holding windows and keys.
pub fn spawn_engine<P: Platform + Send + 'static>(
    platform: P,
    paths: Paths,
    events: Receiver<PlatformEvent>,
    open_settings: bool,
    stop_adapter: impl FnOnce() + Send + 'static,
) -> std::io::Result<JoinHandle<std::io::Result<()>>> {
    let (inputs, incoming) = tokio::sync::mpsc::unbounded_channel();
    forward_events(events, inputs.clone());
    std::thread::Builder::new()
        .name("snappy-engine".into())
        .spawn(move || {
            let _stop_adapter = OnDrop::new(stop_adapter);
            run_engine(platform, paths, inputs, incoming, open_settings)
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
    open_settings: bool,
) -> std::io::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    let local = tokio::task::LocalSet::new();
    local.block_on(&runtime, async move {
        let mut settings_app = SettingsApp::new(inputs.clone());
        let engine = Engine::new(platform, paths);
        if std::env::var_os(DEBUG_LOG_OVERRIDE).is_some_and(|value| value == "1") {
            engine.force_debug_log();
        }
        engine.start();
        if open_settings {
            let _ = inputs.send(Input::Platform(PlatformEvent::OpenSettingsRequested));
        }

        while let Some(input) = incoming.recv().await {
            let quit = matches!(
                input,
                Input::Platform(PlatformEvent::QuitRequested | PlatformEvent::RestartRequested)
            );
            if input == Input::Platform(PlatformEvent::OpenSettingsRequested)
                && let Err(error) = settings_app.open()
            {
                eprintln!("snappy: couldn't open settings: {error}");
            }
            engine.handle(input);
            settings_app.send(engine.take_outbox());
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
