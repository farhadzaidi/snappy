//! Display rules: work areas from panel struts, stable IDs, names from EDID, and the
//! scale from `Xft.dpi`.

use snappy_core::geometry::{Rect, Size};

const EDID_HEADER: [u8; 8] = [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00];
const EDID_DESCRIPTORS: [usize; 4] = [54, 72, 90, 108];
const EDID_NAME_TAG: u8 = 0xFC;
const BASE_DPI: f64 = 96.0;

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

/// The monitor name descriptor of an EDID block, when it has one.
pub fn edid_name(edid: &[u8]) -> Option<String> {
    if edid.len() < 128 || edid[..8] != EDID_HEADER {
        return None;
    }
    EDID_DESCRIPTORS.iter().find_map(|&offset| {
        let descriptor = &edid[offset..offset + 18];
        if descriptor[..3] != [0, 0, 0] || descriptor[3] != EDID_NAME_TAG {
            return None;
        }
        let text: String = descriptor[5..]
            .iter()
            .take_while(|byte| **byte != b'\n')
            .map(|byte| char::from(*byte))
            .collect();
        let name = text.trim();
        (!name.is_empty()).then(|| name.to_owned())
    })
}

/// `Xft.dpi / 96` from the `RESOURCE_MANAGER` string, or 1 when it isn't set.
pub fn scale(resources: &str) -> f64 {
    resources
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(key, _)| key.trim() == "Xft.dpi")
        .and_then(|(_, value)| value.trim().parse::<f64>().ok())
        .filter(|dpi| dpi.is_finite() && *dpi > 0.0)
        .map_or(1.0, |dpi| dpi / BASE_DPI)
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

    fn edid_with_name(name: &[u8]) -> Vec<u8> {
        let mut edid = vec![0u8; 128];
        edid[..8].copy_from_slice(&EDID_HEADER);
        edid[72..77].copy_from_slice(&[0, 0, 0, EDID_NAME_TAG, 0]);
        let mut text = [b' '; 13];
        text[..name.len()].copy_from_slice(name);
        if name.len() < 13 {
            text[name.len()] = b'\n';
        }
        edid[77..90].copy_from_slice(&text);
        edid
    }

    #[test]
    fn edid_names_come_from_the_name_descriptor() {
        assert_eq!(
            edid_name(&edid_with_name(b"DELL U2720Q")),
            Some("DELL U2720Q".into())
        );
        assert_eq!(
            edid_name(&edid_with_name(b"ABCDEFGHIJKLM")),
            Some("ABCDEFGHIJKLM".into())
        );
        assert_eq!(edid_name(&[0u8; 128]), None);
        assert_eq!(edid_name(&edid_with_name(b"X")[..100]), None);
    }

    #[test]
    fn scale_follows_xft_dpi() {
        assert_eq!(scale("Xft.antialias:\t1\nXft.dpi:\t144\n"), 1.5);
        assert_eq!(scale("Xft.dpi: 96"), 1.0);
        assert_eq!(scale("Xcursor.size:\t24\n"), 1.0);
        assert_eq!(scale("Xft.dpi:\tnope"), 1.0);
        assert_eq!(scale(""), 1.0);
    }
}
