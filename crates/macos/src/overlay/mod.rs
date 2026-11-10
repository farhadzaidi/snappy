//! Zone overlays, one surface per display, and the marked windows, on a surface of their
//! own per display. Main thread only.
//!
//! Zone surfaces are created at `prepare` and kept, so the first drag never waits for one.
//! At most one is visible at a time.

mod marks;
mod style;
mod surface;

use std::cell::RefCell;
use std::collections::HashMap;

use objc2::MainThreadMarker;
use snappy_platform::{OverlayMarks, OverlayScene};

use crate::displays::primary_height;
use marks::MarkSurface;
use surface::Surface;

/// What the bookkeeping needs from one display's overlay window.
trait Panel {
    fn show(&self);
    fn hide(&self);
    fn is_visible(&self) -> bool;
}

impl Panel for Surface {
    fn show(&self) {
        Surface::show(self);
    }

    fn hide(&self) {
        Surface::hide(self);
    }

    fn is_visible(&self) -> bool {
        Surface::is_visible(self)
    }
}

struct Overlays<S> {
    surfaces: HashMap<String, S>,
    shown: Option<String>,
}

impl<S> Default for Overlays<S> {
    fn default() -> Self {
        Self {
            surfaces: HashMap::new(),
            shown: None,
        }
    }
}

impl<S: Panel> Overlays<S> {
    /// Shows `display`'s overlay, hiding the one shown before.
    fn show_only(&mut self, display: &str) {
        let previous = self.shown.replace(display.to_owned());
        let previous = previous.filter(|previous| previous != display);
        if let Some(surface) = previous.and_then(|previous| self.surfaces.get(&previous)) {
            surface.hide();
        }
        if let Some(surface) = self.surfaces.get(display) {
            surface.show();
        }
    }

    fn hide_all(&mut self) {
        self.shown = None;
        for surface in self.surfaces.values().filter(|surface| surface.is_visible()) {
            surface.hide();
        }
    }
}

thread_local! {
    static OVERLAYS: RefCell<Overlays<Surface>> = RefCell::new(Overlays::default());
    static MARKS: RefCell<HashMap<String, MarkSurface>> = RefCell::new(HashMap::new());
}

/// Outlines `marks` on `display`, making its marks surface the first time there are any.
pub fn mark(display: &str, marks: &OverlayMarks, mtm: MainThreadMarker) {
    MARKS.with(|surfaces| {
        let mut surfaces = surfaces.borrow_mut();
        if marks.frames.is_empty() && !surfaces.contains_key(display) {
            return;
        }
        let height = primary_height(mtm);
        surfaces
            .entry(display.to_owned())
            .or_insert_with(|| MarkSurface::new(mtm))
            .update(marks, height);
    });
}

pub fn prepare(display: &str, scene: &OverlayScene, mtm: MainThreadMarker) {
    OVERLAYS.with(|overlays| {
        let mut overlays = overlays.borrow_mut();
        let height = primary_height(mtm);
        overlays
            .surfaces
            .entry(display.to_owned())
            .or_insert_with(|| Surface::new(mtm))
            .update(scene, height);
    });
}

/// Shows `display`'s overlay, hiding the one shown before.
pub fn show(display: &str, scene: &OverlayScene, mtm: MainThreadMarker) {
    prepare(display, scene, mtm);
    OVERLAYS.with(|overlays| overlays.borrow_mut().show_only(display));
}

pub fn hide() {
    OVERLAYS.with(|overlays| overlays.borrow_mut().hide_all());
}

/// Closes the surfaces of displays not in `displays`.
pub fn retain(displays: &[String]) {
    OVERLAYS.with(|overlays| {
        let mut overlays = overlays.borrow_mut();
        let gone: Vec<String> = overlays
            .surfaces
            .keys()
            .filter(|id| !displays.contains(id))
            .cloned()
            .collect();
        for id in gone {
            if let Some(surface) = overlays.surfaces.remove(&id) {
                surface.close();
            }
            if overlays.shown.as_ref() == Some(&id) {
                overlays.shown = None;
            }
        }
    });
    MARKS.with(|surfaces| {
        let mut surfaces = surfaces.borrow_mut();
        let gone: Vec<String> = surfaces
            .keys()
            .filter(|id| !displays.contains(id))
            .cloned()
            .collect();
        for id in gone {
            if let Some(surface) = surfaces.remove(&id) {
                surface.close();
            }
        }
    });
}

/// Closes every surface, at shutdown.
pub fn close_all() {
    retain(&[]);
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[derive(Default)]
    struct FakePanel {
        visible: Cell<bool>,
    }

    impl Panel for FakePanel {
        fn show(&self) {
            self.visible.set(true);
        }

        fn hide(&self) {
            self.visible.set(false);
        }

        fn is_visible(&self) -> bool {
            self.visible.get()
        }
    }

    fn visible(overlays: &Overlays<FakePanel>) -> Vec<&str> {
        let mut shown: Vec<&str> = overlays
            .surfaces
            .iter()
            .filter(|(_, panel)| panel.is_visible())
            .map(|(id, _)| id.as_str())
            .collect();
        shown.sort_unstable();
        shown
    }

    #[test]
    fn only_one_displays_overlay_is_visible_at_a_time() {
        let mut overlays = Overlays::<FakePanel>::default();
        for display in ["main", "side", "top"] {
            overlays.surfaces.insert(display.into(), FakePanel::default());
        }

        overlays.show_only("main");
        assert_eq!(visible(&overlays), ["main"]);
        overlays.show_only("side");
        assert_eq!(visible(&overlays), ["side"]);
        overlays.show_only("side");
        assert_eq!(visible(&overlays), ["side"]);

        overlays.surfaces["top"].show();
        overlays.hide_all();
        assert!(visible(&overlays).is_empty());
    }
}
