//! The marks layer: outlines on the selected windows and on the window a focus shortcut
//! just focused, drawn apart from the zones.

use std::collections::BTreeMap;

use snappy_platform::{MarkStyle, MarkedWindow, OverlayMarks, Platform, WindowId};

use crate::core::Core;

/// The marks each display was last sent; displays without any are absent.
#[derive(Default)]
pub(crate) struct Marks {
    sent: BTreeMap<String, OverlayMarks>,
}

impl<P: Platform> Core<P> {
    /// Outline the selected windows, and the window a focus shortcut just focused, on each
    /// display they show on, where they are now. Only displays whose outlines changed are
    /// sent.
    pub fn refresh_marks(&mut self) {
        let mut marked = Vec::new();
        for (window, style) in self.outlined() {
            if let (true, Ok(frame)) = (self.is_on_screen(window), self.platform.frame(window)) {
                marked.push(MarkedWindow { frame, style });
            }
        }
        for display in self.displays.clone() {
            let marks = OverlayMarks {
                work_area: display.work_area,
                windows: marked
                    .iter()
                    .copied()
                    .filter(|mark| mark.frame.intersection_area(display.work_area) > 0.0)
                    .collect(),
            };
            let unchanged = match self.marks.sent.get(&display.id) {
                Some(sent) => *sent == marks,
                None => marks.windows.is_empty(),
            };
            if unchanged {
                continue;
            }
            self.platform.mark(&display.id, &marks);
            if marks.windows.is_empty() {
                self.marks.sent.remove(&display.id);
            } else {
                self.marks.sent.insert(display.id.clone(), marks);
            }
        }
        let displays = self.display_ids();
        self.marks.sent.retain(|display, _| displays.contains(display));
    }

    /// The windows to outline and how. A selected tab is outlined on its group's shown
    /// tab, the one on screen, and the flashed window only in the focus style.
    fn outlined(&self) -> Vec<(WindowId, MarkStyle)> {
        let flash = self.focus_flash.window;
        let mut wanted: Vec<(WindowId, MarkStyle)> = Vec::new();
        for window in &self.selection.windows {
            let shown = self.workspaces.tab_root(*window);
            if Some(shown) != flash && !wanted.iter().any(|(marked, _)| *marked == shown) {
                wanted.push((shown, MarkStyle::Selected));
            }
        }
        wanted.extend(flash.map(|window| (window, MarkStyle::Focused)));
        wanted
    }

    /// Outlines follow a marked window that moved.
    pub fn marked_window_moved(&mut self, window: WindowId) {
        if self.outlined().iter().any(|(marked, _)| *marked == window) {
            self.refresh_marks();
        }
    }
}
