//! Connected displays. Mutter's `DisplayConfig` names each monitor and says how stage
//! coordinates relate to pixels; the extension reports where each monitor is and its work
//! area. When the extension isn't running, frames come from Mutter alone and the work area is
//! the whole display.
//!
//! In Mutter's logical layout, stage coordinates are logical pixels, which are points, so a
//! display's scale for Snappy (window units per point) is 1. In the physical layout they are
//! device pixels and the scale is the monitor's.

use std::collections::HashMap;

use snappy_core::geometry::Rect;
use snappy_linux::display_id::display_id;
use snappy_platform::Display;
use zbus::zvariant::OwnedValue;

use crate::protocol::Monitor;

/// Connector, vendor, product, and serial.
pub type MonitorSpec = (String, String, String, String);
/// ID, width, height, refresh rate, preferred scale, supported scales, properties.
pub type Mode = (String, i32, i32, f64, f64, Vec<f64>, HashMap<String, OwnedValue>);
pub type PhysicalMonitor = (MonitorSpec, Vec<Mode>, HashMap<String, OwnedValue>);
/// X, Y, scale, transform, primary, its monitors, properties.
pub type LogicalMonitor = (
    i32,
    i32,
    f64,
    u32,
    bool,
    Vec<MonitorSpec>,
    HashMap<String, OwnedValue>,
);
/// Serial, monitors, logical monitors, properties.
pub type CurrentState = (
    u32,
    Vec<PhysicalMonitor>,
    Vec<LogicalMonitor>,
    HashMap<String, OwnedValue>,
);

/// `layout-mode` 1 is logical, 2 physical.
const LOGICAL_LAYOUT: u32 = 1;
/// Transforms 1, 3, 5, and 7 turn the monitor a quarter.
fn rotated(transform: u32) -> bool {
    transform % 2 == 1
}

/// The facts Snappy needs about one logical monitor.
#[derive(Debug, Clone, PartialEq)]
pub struct LogicalDisplay {
    pub id: String,
    pub name: String,
    pub origin: (i32, i32),
    pub primary: bool,
    /// The monitor's scale.
    pub scale: f64,
    /// Width and height in stage coordinates, from the current mode.
    pub size: Option<(f64, f64)>,
}

/// Reads Mutter's state: the logical monitors and whether the layout is logical.
pub fn logical_displays(state: &CurrentState) -> (Vec<LogicalDisplay>, bool) {
    let (_, monitors, logical_monitors, properties) = state;
    let logical_layout = properties
        .get("layout-mode")
        .and_then(|value| u32::try_from(value).ok())
        .is_none_or(|mode| mode == LOGICAL_LAYOUT);
    let displays = logical_monitors
        .iter()
        .map(|(x, y, scale, transform, primary, specs, _)| {
            let spec = specs.first();
            let monitor = spec.and_then(|spec| monitors.iter().find(|(candidate, ..)| candidate == spec));
            let connector = spec.map_or_else(|| format!("monitor-{x}-{y}"), |spec| spec.0.clone());
            let identity = spec.map(|(_, vendor, product, serial)| format!("{vendor}\0{product}\0{serial}"));
            let name = monitor
                .and_then(|(_, _, properties)| properties.get("display-name"))
                .and_then(|value| String::try_from(value.try_clone().ok()?).ok())
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| connector.clone());
            let size = monitor
                .and_then(|(_, modes, _)| current_size(modes))
                .map(|(width, height)| {
                    let (width, height) = if rotated(*transform) {
                        (height, width)
                    } else {
                        (width, height)
                    };
                    if logical_layout && *scale > 0.0 {
                        (width / scale, height / scale)
                    } else {
                        (width, height)
                    }
                });
            LogicalDisplay {
                id: display_id(&connector, identity.as_deref().map(str::as_bytes)),
                name,
                origin: (*x, *y),
                primary: *primary,
                scale: *scale,
                size,
            }
        })
        .collect();
    (displays, logical_layout)
}

fn current_size(modes: &[Mode]) -> Option<(f64, f64)> {
    modes
        .iter()
        .find(|(.., properties)| {
            properties
                .get("is-current")
                .and_then(|value| bool::try_from(value).ok())
                .unwrap_or(false)
        })
        .map(|(_, width, height, ..)| (f64::from(*width), f64::from(*height)))
}

/// Window units per point for a monitor of `scale`.
fn units_per_point(logical_layout: bool, scale: f64) -> f64 {
    if logical_layout || !scale.is_finite() || scale <= 0.0 {
        1.0
    } else {
        scale
    }
}

/// Every display, primary first. `monitors` is the extension's report, when it runs.
pub fn merge(state: Option<&CurrentState>, monitors: Option<&[Monitor]>) -> Vec<Display> {
    let (logical, logical_layout) = state.map(logical_displays).unwrap_or((Vec::new(), true));
    let mut displays: Vec<(bool, Display)> = match monitors {
        Some(monitors) => monitors
            .iter()
            .map(|monitor| {
                let frame = Rect::from(monitor.frame);
                let known = logical.iter().find(|display| {
                    (f64::from(display.origin.0), f64::from(display.origin.1)) == (frame.x, frame.y)
                });
                let display = Display {
                    id: known.map_or_else(|| format!("monitor-{}", monitor.index), |known| known.id.clone()),
                    name: known.map_or_else(
                        || format!("Display {}", monitor.index + 1),
                        |known| known.name.clone(),
                    ),
                    frame,
                    work_area: Rect::from(monitor.work_area),
                    scale: units_per_point(logical_layout, known.map_or(monitor.scale, |known| known.scale)),
                };
                (monitor.primary, display)
            })
            .collect(),
        None => logical
            .iter()
            .filter_map(|display| {
                let (width, height) = display.size?;
                let frame = Rect::new(
                    f64::from(display.origin.0),
                    f64::from(display.origin.1),
                    width,
                    height,
                );
                Some((
                    display.primary,
                    Display {
                        id: display.id.clone(),
                        name: display.name.clone(),
                        frame,
                        work_area: frame,
                        scale: units_per_point(logical_layout, display.scale),
                    },
                ))
            })
            .collect(),
    };
    displays.sort_by_key(|(primary, _)| !primary);
    displays.into_iter().map(|(_, display)| display).collect()
}

#[cfg(test)]
mod tests {
    use zbus::zvariant::Value;

    use super::*;
    use crate::protocol::Frame;

    fn owned(value: Value<'_>) -> OwnedValue {
        value.try_to_owned().unwrap()
    }

    fn spec(connector: &str) -> MonitorSpec {
        (connector.into(), "DEL".into(), "U2720Q".into(), "ABC123".into())
    }

    fn state(layout_mode: u32) -> CurrentState {
        let current = HashMap::from([("is-current".to_owned(), owned(Value::from(true)))]);
        let monitor = |connector: &str, name: &str, width: i32, height: i32| -> PhysicalMonitor {
            (
                spec(connector),
                vec![
                    ("other".into(), 1280, 720, 60.0, 1.0, vec![1.0], HashMap::new()),
                    (
                        "current".into(),
                        width,
                        height,
                        60.0,
                        1.0,
                        vec![1.0, 2.0],
                        current.clone(),
                    ),
                ],
                HashMap::from([("display-name".to_owned(), owned(Value::from(name)))]),
            )
        };
        (
            7,
            vec![
                monitor("DP-1", "Dell 27\"", 3840, 2160),
                monitor("HDMI-1", "", 1920, 1080),
            ],
            vec![
                (1920, 0, 2.0, 0, true, vec![spec("DP-1")], HashMap::new()),
                (0, 0, 1.0, 1, false, vec![spec("HDMI-1")], HashMap::new()),
            ],
            HashMap::from([("layout-mode".to_owned(), owned(Value::from(layout_mode)))]),
        )
    }

    #[test]
    fn logical_monitors_get_stable_ids_names_and_sizes() {
        let (displays, logical) = logical_displays(&state(1));
        assert!(logical);
        assert_eq!(displays[0].name, "Dell 27\"");
        assert_eq!(
            displays[0].size,
            Some((1920.0, 1080.0)),
            "logical pixels at scale 2"
        );
        assert_eq!(
            displays[1].name, "HDMI-1",
            "an empty name falls back to the connector"
        );
        assert_eq!(displays[1].size, Some((1080.0, 1920.0)), "turned a quarter");
        assert_eq!(
            displays[0].id,
            display_id("DP-1", Some(b"DEL\0U2720Q\0ABC123".as_slice()))
        );
        assert_ne!(displays[0].id, displays[1].id);
    }

    #[test]
    fn the_physical_layout_keeps_device_pixels() {
        let (displays, logical) = logical_displays(&state(2));
        assert!(!logical);
        assert_eq!(displays[0].size, Some((3840.0, 2160.0)));
    }

    fn monitor(index: u32, primary: bool, x: f64, width: f64) -> Monitor {
        Monitor {
            index,
            primary,
            frame: Frame {
                x,
                y: 0.0,
                width,
                height: 1080.0,
            },
            work_area: Frame {
                x,
                y: 32.0,
                width,
                height: 1048.0,
            },
            scale: if x > 0.0 { 2.0 } else { 1.0 },
        }
    }

    #[test]
    fn the_extensions_monitors_take_mutters_names_and_the_primary_comes_first() {
        let monitors = [monitor(0, false, 0.0, 1080.0), monitor(1, true, 1920.0, 1920.0)];
        let displays = merge(Some(&state(1)), Some(&monitors));
        assert_eq!(displays.len(), 2);
        assert_eq!(displays[0].name, "Dell 27\"");
        assert_eq!(displays[0].work_area, Rect::new(1920.0, 32.0, 1920.0, 1048.0));
        assert_eq!(displays[0].scale, 1.0, "logical pixels are points");
        assert_eq!(displays[1].name, "HDMI-1");
    }

    #[test]
    fn in_the_physical_layout_the_scale_is_the_monitors() {
        let monitors = [monitor(1, true, 1920.0, 3840.0)];
        let displays = merge(Some(&state(2)), Some(&monitors));
        assert_eq!(displays[0].scale, 2.0);
    }

    #[test]
    fn without_the_extension_displays_come_from_mutter_alone() {
        let displays = merge(Some(&state(1)), None);
        assert_eq!(displays[0].frame, Rect::new(1920.0, 0.0, 1920.0, 1080.0));
        assert_eq!(displays[0].work_area, displays[0].frame);
        assert_eq!(displays[1].frame, Rect::new(0.0, 0.0, 1080.0, 1920.0));
    }

    #[test]
    fn without_mutters_state_monitors_get_numbered_names() {
        let displays = merge(None, Some(&[monitor(0, true, 0.0, 1920.0)]));
        assert_eq!(displays[0].id, "monitor-0");
        assert_eq!(displays[0].name, "Display 1");
        assert_eq!(displays[0].scale, 1.0);
        assert!(merge(None, None).is_empty());
    }
}
