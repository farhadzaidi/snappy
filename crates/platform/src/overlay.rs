//! What the overlay draws on each display, and the trait that draws it.

use snappy_core::geometry::Rect;

/// What one display's overlay shows.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlayScene {
    pub work_area: Rect,
    pub zones: Vec<OverlayZone>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OverlayZone {
    pub frame: Rect,
    pub number: usize,
    pub highlighted: bool,
    /// Text under the number, naming what the drop does when it isn't the default.
    pub caption: Option<Caption>,
    /// Outline as the swap source.
    pub swap_source: bool,
}

/// What a drop does, shown under the hovered zone's number while the swap key reverses
/// "When the zone is taken".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Caption {
    Swap,
    Stack,
}

impl Caption {
    pub fn text(self) -> &'static str {
        match self {
            Caption::Swap => "Swap",
            Caption::Stack => "Stack",
        }
    }
}

/// Windows outlined on one display apart from its zones, such as the selected windows.
/// Each is outlined inside its frame, whether or not zones show.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OverlayMarks {
    pub work_area: Rect,
    pub windows: Vec<MarkedWindow>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkedWindow {
    /// Parts outside the work area aren't drawn.
    pub frame: Rect,
    pub style: MarkStyle,
}

/// How a marked window is outlined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkStyle {
    /// In the selection: the swap source's mint border.
    Selected,
    /// Just focused by a focus shortcut: a border in a color of its own, shown briefly.
    Focused,
}

pub trait Overlay {
    fn prepare(&mut self, display: &str, scene: &OverlayScene);
    fn show(&mut self, display: &str, scene: &OverlayScene);
    /// Hide the zones. Marks stay.
    fn hide(&mut self);
    /// Release surfaces of displays not in `displays`, marks included.
    fn retain(&mut self, displays: &[String]);
    /// Outline `marks` on `display` until the next call for it; no frames removes them.
    /// Like the zones, marks take no clicks and never take focus.
    fn mark(&mut self, display: &str, marks: &OverlayMarks);
}
