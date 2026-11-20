//! When two windows overlap, as sending to the back counts it.

use crate::geometry::Rect;

/// Windows that overlap by less than this across or down only touch, as neighbors in
/// adjacent zones may once their frames are rounded, so neither covers the other.
pub(super) const MIN_OVERLAP: f64 = 8.0;

/// Whether two frames overlap by at least `MIN_OVERLAP` across and down.
pub(super) fn overlaps(a: Rect, b: Rect) -> bool {
    a.intersection(b)
        .is_some_and(|shared| shared.width >= MIN_OVERLAP && shared.height >= MIN_OVERLAP)
}
