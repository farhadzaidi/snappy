//! Zone overlays, one surface per display, and the marked windows, on a surface of their
//! own per display. Main thread only.

mod marks;
mod overlays;
mod style;
mod surface;

pub use overlays::{close_all, hide, mark, prepare, retain, show};
