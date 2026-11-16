//! JSON shapes for editor types that `snappy-core` keeps free of serde.

use serde::{Deserialize, Serialize};
use snappy_core::editor::{self, ResizeHandle};
use snappy_core::geometry::Point;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    X,
    Y,
}

impl From<Axis> for editor::Axis {
    fn from(axis: Axis) -> Self {
        match axis {
            Axis::X => editor::Axis::X,
            Axis::Y => editor::Axis::Y,
        }
    }
}

impl From<editor::Axis> for Axis {
    fn from(axis: editor::Axis) -> Self {
        match axis {
            editor::Axis::X => Axis::X,
            editor::Axis::Y => Axis::Y,
        }
    }
}

/// What a cut goes through: `{ "zone": id }` for one zone, or `"through"` for every zone the
/// line crosses.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CutScope {
    Zone(String),
    Through,
}

impl CutScope {
    pub fn to_core(&self) -> editor::CutScope<'_> {
        match self {
            CutScope::Zone(id) => editor::CutScope::Zone(id),
            CutScope::Through => editor::CutScope::Through,
        }
    }
}

/// A resize handle by compass direction, for example `"northWest"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Handle {
    NorthWest,
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
}

impl From<Handle> for ResizeHandle {
    fn from(handle: Handle) -> Self {
        match handle {
            Handle::NorthWest => ResizeHandle::NorthWest,
            Handle::North => ResizeHandle::North,
            Handle::NorthEast => ResizeHandle::NorthEast,
            Handle::East => ResizeHandle::East,
            Handle::SouthEast => ResizeHandle::SouthEast,
            Handle::South => ResizeHandle::South,
            Handle::SouthWest => ResizeHandle::SouthWest,
            Handle::West => ResizeHandle::West,
        }
    }
}

impl From<ResizeHandle> for Handle {
    fn from(handle: ResizeHandle) -> Self {
        match handle {
            ResizeHandle::NorthWest => Handle::NorthWest,
            ResizeHandle::North => Handle::North,
            ResizeHandle::NorthEast => Handle::NorthEast,
            ResizeHandle::East => Handle::East,
            ResizeHandle::SouthEast => Handle::SouthEast,
            ResizeHandle::South => Handle::South,
            ResizeHandle::SouthWest => Handle::SouthWest,
            ResizeHandle::West => Handle::West,
        }
    }
}

/// Where a handle sits on a zone, relative to the zone's top-left corner.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HandlePosition {
    pub handle: Handle,
    pub center: Point,
}

/// A shared edge between zones.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Divider {
    pub id: String,
    pub zone_id: String,
    pub axis: Axis,
    pub line: f64,
    pub start: f64,
    pub end: f64,
}

impl From<editor::Divider> for Divider {
    fn from(divider: editor::Divider) -> Self {
        Self {
            id: divider.id(),
            zone_id: divider.zone_id,
            axis: divider.axis.into(),
            line: divider.line,
            start: divider.span.0,
            end: divider.span.1,
        }
    }
}

/// One of the four coordinate fields in the zone inspector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Field {
    X,
    Y,
    Width,
    Height,
}
