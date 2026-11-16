//! Drawing an overlay scene, or the marked windows' outlines, into a premultiplied ARGB
//! image, for adapters that draw their own overlays (X11, and KDE Plasma's layer-shell
//! surfaces), and the outline shapes used when no compositor can show transparency.

#[cfg(test)]
mod fixtures;
pub mod glyphs;
mod outline;
mod render;

pub use outline::{mark_outline, outline};
pub use render::{pixel_size, render_mark_rows, render_rows, to_argb};
