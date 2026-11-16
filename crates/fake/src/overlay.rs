//! The fake overlay, which records what Snappy shows.

use snappy_platform::{Overlay, OverlayMarks, OverlayScene};

use crate::platform::FakePlatform;

/// One thing Snappy did to the overlay.
#[derive(Debug, Clone, PartialEq)]
pub enum OverlayCall {
    Prepare(String),
    Show(String, OverlayScene),
    Hide,
}

impl Overlay for FakePlatform {
    fn prepare(&mut self, display: &str, _scene: &OverlayScene) {
        self.overlay.push(OverlayCall::Prepare(display.to_owned()));
    }

    fn show(&mut self, display: &str, scene: &OverlayScene) {
        self.overlay
            .push(OverlayCall::Show(display.to_owned(), scene.clone()));
    }

    fn hide(&mut self) {
        if self.overlay.last() != Some(&OverlayCall::Hide) {
            self.overlay.push(OverlayCall::Hide);
        }
    }

    fn retain(&mut self, displays: &[String]) {
        self.marks.retain(|display, _| displays.contains(display));
    }

    fn mark(&mut self, display: &str, marks: &OverlayMarks) {
        self.mark_calls += 1;
        if marks.windows.is_empty() {
            self.marks.remove(display);
        } else {
            self.marks.insert(display.to_owned(), marks.windows.clone());
        }
    }
}
