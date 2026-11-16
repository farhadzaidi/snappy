//! Zone overlays: one layered, click-through, topmost popup per display, covering
//! its work area. Scenes are drawn in software into a DIB and handed to the window with
//! `UpdateLayeredWindow`; DWM keeps the result, so the pixels are freed right after and an
//! idle overlay costs Snappy no memory beyond its window. A surface redraws only when its
//! scene changes. Marked windows' outlines go on a second popup per display of the same
//! kind, so they stay while the zones show and hide.

mod canvas;
mod dib;
mod overlays;
mod paint;
mod style;
mod text;

pub use canvas::{Canvas, Color, Mask};
pub use dib::Dib;
pub use overlays::Overlays;
