//! The engine thread: one Tokio `LocalSet` running the engine until quit.

use snappy_engine::{Engine, Input, Paths};
use snappy_platform::{Platform, PlatformEvent};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::settings_app::SettingsApp;

/// Set to `1` to record the debug log for this run regardless of the setting.
const DEBUG_LOG_OVERRIDE: &str = "SNAPPY_DEBUG";

/// Run the engine on the current thread until the user quits. `open_settings` opens the
/// settings window at launch.
pub fn run_engine<P: Platform + 'static>(
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
            let quit = input == Input::Platform(PlatformEvent::QuitRequested);
            if input == Input::Platform(PlatformEvent::OpenSettingsRequested)
                && let Err(error) = settings_app.open()
            {
                eprintln!("snappy: could not open settings: {error}");
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
pub fn forward_events(events: std::sync::mpsc::Receiver<PlatformEvent>, inputs: UnboundedSender<Input>) {
    std::thread::spawn(move || {
        for event in events {
            if inputs.send(Input::Platform(event)).is_err() {
                return;
            }
        }
    });
}
