//! Frame math: decorations from `_NET_FRAME_EXTENTS`, client-side shadows from
//! `_GTK_FRAME_EXTENTS`, and `_NET_MOVERESIZE_WINDOW` requests.

use snappy_core::geometry::{Point, Rect, Size, round};

/// `_NET_MOVERESIZE_WINDOW` gravity: positions name the client window itself.
const STATIC_GRAVITY: u32 = 10;
const HAS_X: u32 = 1 << 8;
const HAS_Y: u32 = 1 << 9;
const HAS_WIDTH: u32 = 1 << 10;
const HAS_HEIGHT: u32 = 1 << 11;
/// Source indication 2: a pager, which window managers obey without focus-stealing checks.
const FROM_PAGER: u32 = 2 << 12;

/// Window positions and sizes travel as 16-bit values in the X protocol.
const COORDINATE_RANGE: std::ops::RangeInclusive<f64> = (i16::MIN as f64)..=(i16::MAX as f64);
const SIZE_RANGE: std::ops::RangeInclusive<f64> = 1.0..=(i16::MAX as f64);

/// Margins on each side, as the four cardinals left, right, top, bottom.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Extents {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}

impl Extents {
    /// Missing or short properties count as no margin.
    pub fn from_cardinals(values: Option<&[u32]>) -> Extents {
        match values {
            Some([left, right, top, bottom, ..]) => Extents {
                left: f64::from(*left),
                right: f64::from(*right),
                top: f64::from(*top),
                bottom: f64::from(*bottom),
            },
            _ => Extents::default(),
        }
    }
}

/// How a client window relates to the frame users see.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FrameGeometry {
    /// Window manager decorations outside the client.
    pub decorations: Extents,
    /// Invisible shadow margins the client draws inside its own window.
    pub shadows: Extents,
}

impl FrameGeometry {
    /// The visible frame of a client occupying `client` in root coordinates.
    pub fn visible(&self, client: Rect) -> Rect {
        let (d, s) = (self.decorations, self.shadows);
        Rect::new(
            client.x - d.left + s.left,
            client.y - d.top + s.top,
            client.width + d.left + d.right - s.left - s.right,
            client.height + d.top + d.bottom - s.top - s.bottom,
        )
    }

    /// Where the client window goes so the visible frame starts at `origin`.
    pub fn client_origin(&self, origin: Point) -> Point {
        let (d, s) = (self.decorations, self.shadows);
        Point::new(origin.x + d.left - s.left, origin.y + d.top - s.top)
    }

    /// The client size that gives a visible frame of `size`.
    pub fn client_size(&self, size: Size) -> Size {
        let (d, s) = (self.decorations, self.shadows);
        Size::new(
            size.width - d.left - d.right + s.left + s.right,
            size.height - d.top - d.bottom + s.top + s.bottom,
        )
    }
}

/// The five data words of a `_NET_MOVERESIZE_WINDOW` client message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveResize([u32; 5]);

impl MoveResize {
    /// Moves the client window to `origin`, or `None` when X can't represent it.
    pub fn position(origin: Point) -> Option<MoveResize> {
        let x = coordinate(origin.x)?;
        let y = coordinate(origin.y)?;
        Some(MoveResize([
            STATIC_GRAVITY | HAS_X | HAS_Y | FROM_PAGER,
            x as u32,
            y as u32,
            0,
            0,
        ]))
    }

    /// Resizes the client window to `size`, keeping its origin.
    pub fn size(size: Size) -> Option<MoveResize> {
        let width = dimension(size.width)?;
        let height = dimension(size.height)?;
        Some(MoveResize([
            STATIC_GRAVITY | HAS_WIDTH | HAS_HEIGHT | FROM_PAGER,
            0,
            0,
            width,
            height,
        ]))
    }

    pub fn data(self) -> [u32; 5] {
        self.0
    }
}

fn coordinate(value: f64) -> Option<i32> {
    let value = round(value);
    COORDINATE_RANGE.contains(&value).then_some(value as i32)
}

fn dimension(value: f64) -> Option<u32> {
    let value = round(value);
    SIZE_RANGE.contains(&value).then_some(value as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decorated() -> FrameGeometry {
        FrameGeometry {
            decorations: Extents {
                left: 4.0,
                right: 4.0,
                top: 28.0,
                bottom: 4.0,
            },
            shadows: Extents::default(),
        }
    }

    fn client_side() -> FrameGeometry {
        FrameGeometry {
            decorations: Extents::default(),
            shadows: Extents {
                left: 23.0,
                right: 23.0,
                top: 15.0,
                bottom: 31.0,
            },
        }
    }

    #[test]
    fn frames_include_window_manager_decorations() {
        let frame = decorated().visible(Rect::new(104.0, 228.0, 800.0, 600.0));
        assert_eq!(frame, Rect::new(100.0, 200.0, 808.0, 632.0));
    }

    #[test]
    fn frames_exclude_client_side_shadows() {
        let frame = client_side().visible(Rect::new(77.0, 185.0, 846.0, 646.0));
        assert_eq!(frame, Rect::new(100.0, 200.0, 800.0, 600.0));
    }

    #[test]
    fn client_geometry_round_trips_to_the_requested_frame() {
        for geometry in [decorated(), client_side(), FrameGeometry::default()] {
            let wanted = Rect::new(-50.0, 30.0, 960.0, 1040.0);
            let origin = geometry.client_origin(wanted.origin());
            let size = geometry.client_size(wanted.size());
            let client = Rect::from_origin_size(origin, size);
            assert_eq!(geometry.visible(client), wanted);
        }
    }

    #[test]
    fn missing_extents_are_zero() {
        assert_eq!(Extents::from_cardinals(None), Extents::default());
        assert_eq!(Extents::from_cardinals(Some(&[1, 2])), Extents::default());
        assert_eq!(
            Extents::from_cardinals(Some(&[1, 2, 3, 4])),
            Extents {
                left: 1.0,
                right: 2.0,
                top: 3.0,
                bottom: 4.0
            }
        );
    }

    #[test]
    fn moves_use_static_gravity_from_a_pager() {
        let request = MoveResize::position(Point::new(-864.4, 12.6)).unwrap();
        assert_eq!(request.data()[0], 10 | 0x100 | 0x200 | 0x2000);
        assert_eq!(request.data()[1] as i32, -864);
        assert_eq!(request.data()[2], 13);
    }

    #[test]
    fn resizes_carry_only_width_and_height() {
        let request = MoveResize::size(Size::new(640.0, 480.0)).unwrap();
        assert_eq!(request.data(), [10 | 0x400 | 0x800 | 0x2000, 0, 0, 640, 480]);
    }

    #[test]
    fn values_x_cannot_represent_are_refused() {
        assert_eq!(MoveResize::position(Point::new(-40000.0, 0.0)), None);
        assert_eq!(MoveResize::position(Point::new(f64::NAN, 0.0)), None);
        assert_eq!(MoveResize::size(Size::new(0.0, 100.0)), None);
        assert_eq!(MoveResize::size(Size::new(100.0, 40000.0)), None);
    }
}
