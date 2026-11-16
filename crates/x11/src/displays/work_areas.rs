//! Work areas: a monitor minus the space docks and panels reserve with struts, since
//! `_NET_WORKAREA` is one rectangle for all monitors.

use snappy_core::geometry::{Rect, Size};

/// Space a dock or panel reserves at one screen edge, in root coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Strut {
    pub edge: Edge,
    /// How far the reservation reaches in from the screen edge.
    pub depth: f64,
    /// Where along the edge it starts and ends, inclusive.
    pub start: f64,
    pub end: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

/// The struts of one client: `_NET_WM_STRUT_PARTIAL` when present, else the legacy
/// `_NET_WM_STRUT`, which spans whole edges.
pub fn struts(partial: Option<&[u32]>, legacy: Option<&[u32]>, screen: Size) -> Vec<Strut> {
    let edges = [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom];
    let spans: Vec<(f64, f64, f64)> = match (partial, legacy) {
        (Some(values), _) if values.len() >= 12 => (0..4)
            .map(|i| {
                let value = |index: usize| f64::from(values[index]);
                (value(i), value(4 + 2 * i), value(5 + 2 * i))
            })
            .collect(),
        (_, Some(values)) if values.len() >= 4 => (0..4)
            .map(|i| {
                let along = if i < 2 { screen.height } else { screen.width };
                (f64::from(values[i]), 0.0, along - 1.0)
            })
            .collect(),
        _ => Vec::new(),
    };
    edges
        .into_iter()
        .zip(spans)
        .filter(|(_, (depth, start, end))| *depth > 0.0 && end >= start)
        .map(|(edge, (depth, start, end))| Strut {
            edge,
            depth,
            start,
            end,
        })
        .collect()
}

impl Strut {
    /// The reserved rectangle in root coordinates.
    fn reserved(&self, screen: Size) -> Rect {
        let span = self.end - self.start + 1.0;
        match self.edge {
            Edge::Left => Rect::new(0.0, self.start, self.depth, span),
            Edge::Right => Rect::new(screen.width - self.depth, self.start, self.depth, span),
            Edge::Top => Rect::new(self.start, 0.0, span, self.depth),
            Edge::Bottom => Rect::new(self.start, screen.height - self.depth, span, self.depth),
        }
    }
}

/// A monitor minus the struts that touch it. A reservation that would swallow the whole
/// monitor belongs to a neighbor, such as a panel on the inner edge of a second monitor,
/// and is ignored.
pub fn work_area(monitor: Rect, screen: Size, struts: &[Strut]) -> Rect {
    let (mut left, mut top) = (monitor.min_x(), monitor.min_y());
    let (mut right, mut bottom) = (monitor.max_x(), monitor.max_y());
    for strut in struts {
        let reserved = strut.reserved(screen);
        if reserved.intersection_area(monitor) <= 0.0 {
            continue;
        }
        match strut.edge {
            Edge::Left if reserved.max_x() < right => left = left.max(reserved.max_x()),
            Edge::Right if reserved.min_x() > left => right = right.min(reserved.min_x()),
            Edge::Top if reserved.max_y() < bottom => top = top.max(reserved.max_y()),
            Edge::Bottom if reserved.min_y() > top => bottom = bottom.min(reserved.min_y()),
            _ => {}
        }
    }
    Rect::new(left, top, right - left, bottom - top)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Size = Size::new(3840.0, 1080.0);
    const LEFT: Rect = Rect::new(0.0, 0.0, 1920.0, 1080.0);
    const RIGHT: Rect = Rect::new(1920.0, 0.0, 1920.0, 1080.0);

    fn partial(values: [u32; 12]) -> Vec<Strut> {
        struts(Some(&values), None, SCREEN)
    }

    #[test]
    fn top_panel_on_one_monitor_shrinks_only_that_monitor() {
        let panel = partial([0, 0, 28, 0, 0, 0, 0, 0, 0, 1919, 0, 0]);
        assert_eq!(
            work_area(LEFT, SCREEN, &panel),
            Rect::new(0.0, 28.0, 1920.0, 1052.0)
        );
        assert_eq!(work_area(RIGHT, SCREEN, &panel), RIGHT);
    }

    #[test]
    fn bottom_and_side_struts_shrink_their_edges() {
        let dock = partial([0, 0, 0, 48, 0, 0, 0, 0, 0, 0, 1920, 3839]);
        assert_eq!(
            work_area(RIGHT, SCREEN, &dock),
            Rect::new(1920.0, 0.0, 1920.0, 1032.0)
        );
        let side = partial([64, 0, 0, 0, 0, 1079, 0, 0, 0, 0, 0, 0]);
        assert_eq!(
            work_area(LEFT, SCREEN, &side),
            Rect::new(64.0, 0.0, 1856.0, 1080.0)
        );
        let right = partial([0, 30, 0, 0, 0, 0, 0, 1079, 0, 0, 0, 0]);
        assert_eq!(
            work_area(RIGHT, SCREEN, &right),
            Rect::new(1920.0, 0.0, 1890.0, 1080.0)
        );
    }

    #[test]
    fn a_panel_on_an_inner_edge_belongs_to_the_neighbor() {
        // A 40 px panel on the left edge of the right monitor, measured from the screen edge.
        let panel = partial([1960, 0, 0, 0, 0, 1079, 0, 0, 0, 0, 0, 0]);
        assert_eq!(work_area(LEFT, SCREEN, &panel), LEFT);
        assert_eq!(
            work_area(RIGHT, SCREEN, &panel),
            Rect::new(1960.0, 0.0, 1880.0, 1080.0)
        );
    }

    #[test]
    fn legacy_struts_span_whole_edges() {
        let legacy = struts(None, Some(&[0, 0, 24, 0]), SCREEN);
        assert_eq!(work_area(LEFT, SCREEN, &legacy).min_y(), 24.0);
        assert_eq!(work_area(RIGHT, SCREEN, &legacy).min_y(), 24.0);
    }

    #[test]
    fn partial_struts_win_over_legacy_ones() {
        let both = struts(
            Some(&[0, 0, 28, 0, 0, 0, 0, 0, 0, 1919, 0, 0]),
            Some(&[0, 0, 28, 0]),
            SCREEN,
        );
        assert_eq!(work_area(RIGHT, SCREEN, &both), RIGHT);
    }

    #[test]
    fn no_struts_leave_the_monitor_whole() {
        assert!(struts(None, None, SCREEN).is_empty());
        assert_eq!(work_area(LEFT, SCREEN, &[]), LEFT);
    }
}
