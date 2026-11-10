//! A retained `AXUIElement` with typed attribute access.

use std::hash::{Hash, Hasher};
use std::ptr::{self, NonNull};

use objc2_core_foundation::{
    CFArray, CFBoolean, CFGetTypeID, CFHash, CFRetained, CFString, CFType, ConcreteType,
};
use snappy_core::geometry::{Point, Rect, Size};

use super::error::AxError;
use super::names;
use super::value;
use crate::ffi::Pid;
use crate::ffi::ax::{
    AXUIElementCopyAttributeValue, AXUIElementCopyElementAtPosition, AXUIElementCopyMultipleAttributeValues,
    AXUIElementCreateApplication, AXUIElementCreateSystemWide, AXUIElementGetPid, AXUIElementGetTypeID,
    AXUIElementIsAttributeSettable, AXUIElementPerformAction, AXUIElementRef, AXUIElementSetAttributeValue,
    AXUIElementSetMessagingTimeout,
};

#[derive(Clone)]
pub struct AxElement(CFRetained<CFType>);

// SAFETY: AXUIElements are immutable Core Foundation objects, and the Accessibility API
// may be called from any thread.
unsafe impl Send for AxElement {}
unsafe impl Sync for AxElement {}

impl PartialEq for AxElement {
    fn eq(&self, other: &Self) -> bool {
        *self.0 == *other.0
    }
}

impl Eq for AxElement {}

impl Hash for AxElement {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl std::fmt::Debug for AxElement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AxElement({:#x})", self.token())
    }
}

impl AxElement {
    /// Takes ownership of a +1 reference; `None` for null.
    ///
    /// # Safety
    /// `raw` must be null or an owned `AXUIElementRef`.
    pub unsafe fn from_owned(raw: AXUIElementRef) -> Option<Self> {
        NonNull::new(raw.cast_mut()).map(|raw| Self(unsafe { CFRetained::from_raw(raw) }))
    }

    /// Wraps an attribute value if it is an element.
    pub fn from_value(value: CFRetained<CFType>) -> Option<Self> {
        // SAFETY: plain type ID lookup.
        let is_element = CFGetTypeID(Some(&value)) == unsafe { AXUIElementGetTypeID() };
        is_element.then_some(Self(value))
    }

    pub fn application(pid: Pid, timeout: f32) -> Option<Self> {
        // SAFETY: the create function returns a +1 reference.
        let element = unsafe { Self::from_owned(AXUIElementCreateApplication(pid)) }?;
        element.set_timeout(timeout);
        Some(element)
    }

    pub fn system_wide(timeout: f32) -> Option<Self> {
        // SAFETY: the create function returns a +1 reference.
        let element = unsafe { Self::from_owned(AXUIElementCreateSystemWide()) }?;
        element.set_timeout(timeout);
        Some(element)
    }

    pub fn as_ptr(&self) -> AXUIElementRef {
        CFRetained::as_ptr(&self.0).as_ptr()
    }

    /// The element as an attribute value, for `AXFocusedWindow`.
    pub fn as_cf(&self) -> &CFType {
        &self.0
    }

    /// `CFHash` of the element, stable while the window exists.
    pub fn token(&self) -> usize {
        CFHash(Some(&self.0))
    }

    pub fn set_timeout(&self, seconds: f32) {
        // SAFETY: valid element.
        unsafe { AXUIElementSetMessagingTimeout(self.as_ptr(), seconds) };
    }

    pub fn pid(&self) -> Option<Pid> {
        let mut pid = 0;
        // SAFETY: valid element and out-pointer.
        AxError::check(unsafe { AXUIElementGetPid(self.as_ptr(), &mut pid) }).ok()?;
        Some(pid)
    }

    /// The element at a top-left global point, on the system-wide element.
    pub fn element_at(&self, point: Point) -> Result<AxElement, AxError> {
        let mut hit: AXUIElementRef = ptr::null();
        // SAFETY: valid element and out-pointer; the result is +1.
        let code = unsafe {
            AXUIElementCopyElementAtPosition(self.as_ptr(), point.x as f32, point.y as f32, &mut hit)
        };
        AxError::check(code)?;
        unsafe { Self::from_owned(hit) }.ok_or(AxError::NO_VALUE)
    }

    pub fn attribute(&self, name: &'static str) -> Result<CFRetained<CFType>, AxError> {
        let name = CFString::from_static_str(name);
        let mut value: *const CFType = ptr::null();
        // SAFETY: valid element, name, and out-pointer; the result is +1.
        let code = unsafe { AXUIElementCopyAttributeValue(self.as_ptr(), &name, &mut value) };
        AxError::check(code)?;
        NonNull::new(value.cast_mut())
            .map(|value| unsafe { CFRetained::from_raw(value) })
            .ok_or(AxError::NO_VALUE)
    }

    fn typed<T: ConcreteType>(&self, name: &'static str) -> Option<CFRetained<T>> {
        self.attribute(name).ok()?.downcast::<T>().ok()
    }

    pub fn bool_attribute(&self, name: &'static str) -> Option<bool> {
        self.typed::<CFBoolean>(name).map(|value| value.as_bool())
    }

    pub fn string_attribute(&self, name: &'static str) -> Option<String> {
        self.typed::<CFString>(name).map(|value| value.to_string())
    }

    pub fn element_attribute(&self, name: &'static str) -> Option<AxElement> {
        let element = Self::from_value(self.attribute(name).ok()?)?;
        element.set_timeout(crate::thresholds::AX_MESSAGING_TIMEOUT);
        Some(element)
    }

    /// An array of elements, such as `AXWindows` or `AXChildren`.
    pub fn elements_attribute(&self, name: &'static str) -> Result<Vec<AxElement>, AxError> {
        let value = self.attribute(name)?;
        let array = value.downcast::<CFArray>().map_err(|_| AxError::FAILURE)?;
        // SAFETY: an opaque array holds CF objects.
        let array = unsafe { CFRetained::cast_unchecked::<CFArray<CFType>>(array) };
        Ok(array.to_vec().into_iter().filter_map(Self::from_value).collect())
    }

    pub fn is_settable(&self, name: &'static str) -> bool {
        let name = CFString::from_static_str(name);
        let mut settable = 0;
        // SAFETY: valid element, name, and out-pointer.
        let code = unsafe { AXUIElementIsAttributeSettable(self.as_ptr(), &name, &mut settable) };
        code == AxError::SUCCESS && settable != 0
    }

    pub fn set_attribute(&self, name: &'static str, value: &CFType) -> Result<(), AxError> {
        let name = CFString::from_static_str(name);
        // SAFETY: valid element, name, and value.
        AxError::check(unsafe { AXUIElementSetAttributeValue(self.as_ptr(), &name, value) })
    }

    pub fn set_bool(&self, name: &'static str, on: bool) -> Result<(), AxError> {
        self.set_attribute(name, CFBoolean::new(on))
    }

    pub fn perform(&self, action: &'static str) -> Result<(), AxError> {
        let action = CFString::from_static_str(action);
        // SAFETY: valid element and action name.
        AxError::check(unsafe { AXUIElementPerformAction(self.as_ptr(), &action) })
    }

    /// Position and size in one round trip, falling back to two reads for apps that
    /// don't support batched reads.
    pub fn frame(&self) -> Result<Rect, AxError> {
        if let Some(frame) = self.batched_frame() {
            return Ok(frame);
        }
        let position = self.attribute(names::POSITION)?;
        let size = self.attribute(names::SIZE)?;
        value::read_frame(&position, &size).ok_or(AxError::FAILURE)
    }

    fn batched_frame(&self) -> Option<Rect> {
        let position = CFString::from_static_str(names::POSITION);
        let size = CFString::from_static_str(names::SIZE);
        let names = CFArray::<CFString>::from_objects(&[&position, &size]);
        let mut values: *const CFArray = ptr::null();
        // SAFETY: valid element, name array, and out-pointer; the result is +1.
        let code = unsafe {
            AXUIElementCopyMultipleAttributeValues(self.as_ptr(), names.as_opaque(), 0, &mut values)
        };
        AxError::check(code).ok()?;
        let values = NonNull::new(values.cast_mut()).map(|raw| unsafe { CFRetained::from_raw(raw) })?;
        // SAFETY: the array holds one CF object per requested attribute.
        let values = unsafe { CFRetained::cast_unchecked::<CFArray<CFType>>(values) };
        let (position, size) = (values.get(0)?, values.get(1)?);
        value::read_frame(&position, &size)
    }

    pub fn set_position(&self, origin: Point) -> Result<(), AxError> {
        let value = value::point(origin).ok_or(AxError::FAILURE)?;
        self.set_attribute(names::POSITION, &value)
    }

    pub fn set_size(&self, size: Size) -> Result<(), AxError> {
        let value = value::size(size).ok_or(AxError::FAILURE)?;
        self.set_attribute(names::SIZE, &value)
    }
}
