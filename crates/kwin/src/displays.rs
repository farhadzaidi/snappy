//! Connected displays, as the script reads them from KWin. KWin's coordinates are logical
//! pixels, which are points, so every display's scale for Snappy (window units per point)
//! is 1, whatever the monitor's own scale.

use snappy_core::geometry::{Point, Rect};
use snappy_linux::display_id::display_id;
use snappy_platform::Display;

use crate::overlay::Target;
use crate::protocol::Monitor;

/// Every display, in KWin's order, whose first is the primary one.
pub fn displays(monitors: &[Monitor]) -> Vec<Display> {
    monitors.iter().map(display).collect()
}

/// Where the display's overlay goes: its output, and where and how scaled that is.
pub fn target(monitor: &Monitor) -> Target {
    Target {
        output: monitor.name.clone(),
        origin: Point::new(monitor.frame.x, monitor.frame.y),
        scale: monitor.scale,
    }
}

fn display(monitor: &Monitor) -> Display {
    let identity = [
        monitor.manufacturer.as_str(),
        monitor.model.as_str(),
        monitor.serial.as_str(),
    ];
    let identity = identity
        .iter()
        .any(|part| !part.is_empty())
        .then(|| identity.join("\0"));
    let product = format!("{} {}", monitor.manufacturer, monitor.model);
    let name = match product.trim() {
        "" => monitor.name.clone(),
        product => product.to_owned(),
    };
    Display {
        id: display_id(&monitor.name, identity.as_deref().map(str::as_bytes)),
        name,
        frame: Rect::from(monitor.frame),
        work_area: Rect::from(monitor.work_area),
        scale: 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Frame;

    fn monitor(name: &str, manufacturer: &str, x: f64) -> Monitor {
        Monitor {
            name: name.into(),
            manufacturer: manufacturer.into(),
            model: if manufacturer.is_empty() {
                String::new()
            } else {
                "U2720Q".into()
            },
            serial: String::new(),
            frame: Frame {
                x,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
            },
            work_area: Frame {
                x,
                y: 0.0,
                width: 1920.0,
                height: 1036.0,
            },
            scale: 1.25,
        }
    }

    #[test]
    fn displays_are_named_for_their_monitor_and_keep_kwins_order() {
        let found = displays(&[monitor("DP-1", "Dell", 0.0), monitor("Virtual-1", "", 1920.0)]);
        assert_eq!(found[0].name, "Dell U2720Q");
        assert!(found[0].id.starts_with("DP-1-"));
        assert_eq!(found[0].work_area.height, 1036.0);
        assert_eq!(found[0].scale, 1.0, "KWin's coordinates are already points");
        assert_eq!(found[1].name, "Virtual-1");
        assert_eq!(found[1].id, "Virtual-1");
    }
}
