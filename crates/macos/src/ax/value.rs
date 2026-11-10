//! Points and sizes boxed in `AXValue`s.

use std::ffi::c_void;
use std::ptr::NonNull;

use objc2_core_foundation::{CFGetTypeID, CFRetained, CFType, CGPoint, CGSize};
use snappy_core::geometry::{Point, Rect, Size};

use crate::ffi::ax::{
    AX_VALUE_CG_POINT, AX_VALUE_CG_SIZE, AXValueCreate, AXValueGetType, AXValueGetTypeID, AXValueGetValue,
    AXValueType,
};

pub fn point(point: Point) -> Option<CFRetained<CFType>> {
    let value = CGPoint::new(point.x, point.y);
    create(AX_VALUE_CG_POINT, &value)
}

pub fn size(size: Size) -> Option<CFRetained<CFType>> {
    let value = CGSize::new(size.width, size.height);
    create(AX_VALUE_CG_SIZE, &value)
}

pub fn read_point(value: &CFType) -> Option<Point> {
    read::<CGPoint>(value, AX_VALUE_CG_POINT).map(|p| Point::new(p.x, p.y))
}

pub fn read_size(value: &CFType) -> Option<Size> {
    read::<CGSize>(value, AX_VALUE_CG_SIZE).map(|s| Size::new(s.width, s.height))
}

pub fn read_frame(position: &CFType, size: &CFType) -> Option<Rect> {
    Some(Rect::from_origin_size(read_point(position)?, read_size(size)?))
}

fn create<T>(value_type: AXValueType, value: &T) -> Option<CFRetained<CFType>> {
    // SAFETY: `value` points at the C struct `value_type` names; the result is +1.
    let raw = unsafe { AXValueCreate(value_type, (value as *const T).cast::<c_void>()) };
    NonNull::new(raw.cast_mut()).map(|raw| unsafe { CFRetained::from_raw(raw) })
}

fn read<T: Default>(value: &CFType, value_type: AXValueType) -> Option<T> {
    // SAFETY: the type ID check guarantees `value` is an AXValue.
    let is_ax_value = CFGetTypeID(Some(value)) == unsafe { AXValueGetTypeID() };
    if !is_ax_value || unsafe { AXValueGetType(value) } != value_type {
        return None;
    }
    let mut out = T::default();
    // SAFETY: `out` is the C struct `value_type` names.
    let ok = unsafe { AXValueGetValue(value, value_type, (&mut out as *mut T).cast::<c_void>()) };
    (ok != 0).then_some(out)
}
