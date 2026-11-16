//! Layout editing: every operation the editor canvas performs on a layout's zones.
//!
//! All operations take the current zones and return new ones; none mutate in place.
//! Results never contain overlaps or zones below the minimum size, except that drafts
//! loaded with overlaps stay as they are until the user repairs them. Operations that add
//! zones (cutting and placing copies) never go past the most zones a layout may have.

mod creating;
mod cutting;
mod geometry;
mod gestures;
mod guides;
mod handles;
mod merging;
mod moving;
#[cfg(test)]
mod properties;
mod repair;
mod seams;
#[cfg(test)]
pub(crate) mod test_support;

pub use creating::{PlaceRefusal, add_rect, draw, place_copies};
pub use cutting::{CutScope, cut, cut_guides, cut_zones, split};
pub use geometry::{Axis, Side, is_separated};
pub(crate) use geometry::{EPSILON, Span, is_free, is_sound};
pub use gestures::{
    CutPreview, Drawn, Edit, Field, Guides, cut_preview, drag_divider, drag_move, drag_resize, draw_zone,
    set_field,
};
pub use guides::{GUIDE_FRACTIONS, guides, snap, snapped_move, zone_guides};
pub use handles::{ResizeHandle, handle_at, handle_center, resize_rect};
pub use merging::{MergeRefusal, Merged, merge};
pub use moving::move_zone;
pub use repair::{ALIGN_TOLERANCE, align_edges};
pub use seams::{Divider, dividers, move_edge, resize_zone, seam_members};
