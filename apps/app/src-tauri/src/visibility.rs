//! When the window shows. It starts hidden and shows once the UI has drawn, rather than as
//! an empty frame that ignores clicks while the web view starts.

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::window::MAIN_WINDOW;

/// How long the window waits for the UI to draw before showing anyway, so a UI that fails to
/// start leaves a window to see and close rather than an app that never appears. The UI
/// normally draws about a second after the window is made, most of it the web view starting,
/// so this leaves a slow machine room to spare while still answering a broken start before
/// someone gives up on it.
const DRAW_DEADLINE: Duration = Duration::from_secs(3);

#[derive(Default)]
pub struct Visibility {
    shown: AtomicBool,
}

impl Visibility {
    /// Show the window by the deadline if the UI hasn't drawn by then.
    pub fn start_deadline(app: &AppHandle) {
        let app = app.clone();
        thread::spawn(move || {
            thread::sleep(DRAW_DEADLINE);
            app.state::<Visibility>().show(&app);
        });
    }

    /// The UI has drawn: show the window, unless the deadline already did.
    pub fn drawn(&self, app: &AppHandle) {
        self.show(app);
    }

    /// Bring the shown window forward. A window not shown yet comes forward when it shows.
    pub fn bring_forward(&self, app: &AppHandle) {
        if self.is_shown() {
            reveal(app);
        }
    }

    /// Show and focus the window the first time only, so whichever of drawing and the
    /// deadline comes second doesn't pull focus back to it later.
    fn show(&self, app: &AppHandle) {
        if self.first_showing() {
            reveal(app);
        }
    }

    /// True for the first call only.
    fn first_showing(&self) -> bool {
        !self.shown.swap(true, Ordering::AcqRel)
    }

    fn is_shown(&self) -> bool {
        self.shown.load(Ordering::Acquire)
    }
}

fn reveal(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return;
    };
    if let Err(error) = window.show().and_then(|()| window.set_focus()) {
        eprintln!("snappy-app: couldn't show the window: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::Visibility;

    #[test]
    fn the_window_shows_once_whether_the_ui_draws_or_the_deadline_passes_first() {
        let visibility = Visibility::default();
        assert!(!visibility.is_shown(), "the window starts hidden");
        assert!(visibility.first_showing());
        assert!(visibility.is_shown());
        assert!(!visibility.first_showing(), "the later of the two shows nothing");
    }
}
