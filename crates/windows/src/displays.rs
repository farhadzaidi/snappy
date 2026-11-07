//! Displays: monitor and work-area rectangles from `GetMonitorInfoW`, the scale from the
//! effective DPI, and stable IDs and names from `QueryDisplayConfig`. All coordinates are
//! physical pixels, since Snappy is per-monitor DPI aware.

use std::collections::HashMap;

use snappy_core::geometry::Rect;
use snappy_platform::Display;
use windows::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO,
    DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME, DisplayConfigGetDeviceInfo,
    GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig,
};
use windows::Win32::Foundation::{ERROR_SUCCESS, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::MONITORINFOF_PRIMARY;
use windows::core::BOOL;

use crate::managed::record::rect;
use crate::wide::from_wide;

/// The DPI at 100% scale.
const BASE_DPI: f64 = 96.0;

/// One monitor as `GetMonitorInfoW` describes it.
#[derive(Debug, Clone, PartialEq)]
pub struct Monitor {
    /// The GDI device name, such as `\\.\DISPLAY1`.
    pub device: String,
    pub frame: Rect,
    pub work_area: Rect,
    pub primary: bool,
    pub dpi: u32,
}

/// What `QueryDisplayConfig` knows about the monitor behind a GDI device.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Target {
    /// Stable across reconnects: the monitor's device interface path.
    pub path: String,
    pub friendly_name: String,
}

/// Displays in the engine's order: the primary first, then left to right, top to bottom.
pub fn assemble(mut monitors: Vec<Monitor>, targets: &HashMap<String, Target>) -> Vec<Display> {
    monitors.sort_by(|a, b| {
        b.primary
            .cmp(&a.primary)
            .then(a.frame.x.total_cmp(&b.frame.x))
            .then(a.frame.y.total_cmp(&b.frame.y))
    });
    monitors
        .into_iter()
        .enumerate()
        .map(|(index, monitor)| {
            let target = targets.get(&monitor.device);
            let id = target
                .map(|target| target.path.clone())
                .filter(|path| !path.is_empty())
                .unwrap_or_else(|| monitor.device.clone());
            let name = target
                .map(|target| target.friendly_name.clone())
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| format!("Display {}", index + 1));
            Display {
                id,
                name,
                frame: monitor.frame,
                work_area: monitor.work_area,
                scale: f64::from(monitor.dpi.max(1)) / BASE_DPI,
            }
        })
        .collect()
}

pub fn read() -> Vec<Display> {
    assemble(monitors(), &targets())
}

unsafe extern "system" fn collect_monitor(
    monitor: HMONITOR,
    _dc: HDC,
    _clip: *mut RECT,
    data: LPARAM,
) -> BOOL {
    // SAFETY: `data` is the vector `monitors` passed, alive for the enumeration.
    let found = unsafe { &mut *(data.0 as *mut Vec<HMONITOR>) };
    found.push(monitor);
    BOOL(1)
}

fn monitors() -> Vec<Monitor> {
    let mut handles: Vec<HMONITOR> = Vec::new();
    // SAFETY: the callback only pushes into `handles`, which outlives the call.
    let _ = unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(collect_monitor),
            LPARAM(&raw mut handles as isize),
        )
    };
    handles.into_iter().filter_map(describe).collect()
}

fn describe(monitor: HMONITOR) -> Option<Monitor> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    // SAFETY: cbSize says the extended structure is passed.
    unsafe { GetMonitorInfoW(monitor, (&raw mut info).cast()) }
        .as_bool()
        .then_some(())?;
    let (mut dpi, mut _dpi_y) = (0u32, 0u32);
    // SAFETY: valid out pointers.
    let dpi = match unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut _dpi_y) } {
        Ok(()) => dpi,
        Err(_) => BASE_DPI as u32,
    };
    Some(Monitor {
        device: from_wide(&info.szDevice),
        frame: rect(info.monitorInfo.rcMonitor),
        work_area: rect(info.monitorInfo.rcWork),
        primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
        dpi,
    })
}

/// Active display paths, keyed by the GDI device name of their source.
fn targets() -> HashMap<String, Target> {
    let (mut path_count, mut mode_count) = (0u32, 0u32);
    // SAFETY: valid out pointers.
    let sized =
        unsafe { GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count) };
    if sized != ERROR_SUCCESS {
        return HashMap::new();
    }
    let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
    let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
    // SAFETY: the arrays hold the counts passed.
    let queried = unsafe {
        QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut path_count,
            paths.as_mut_ptr(),
            &mut mode_count,
            modes.as_mut_ptr(),
            None,
        )
    };
    if queried != ERROR_SUCCESS {
        return HashMap::new();
    }
    paths.truncate(path_count as usize);
    paths.iter().filter_map(target).collect()
}

fn target(path: &DISPLAYCONFIG_PATH_INFO) -> Option<(String, Target)> {
    let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
            size: size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
            adapterId: path.sourceInfo.adapterId,
            id: path.sourceInfo.id,
        },
        ..Default::default()
    };
    // SAFETY: the header names the structure's own type and size.
    (unsafe { DisplayConfigGetDeviceInfo(&mut source.header) } == 0).then_some(())?;
    let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
            size: size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
            adapterId: path.targetInfo.adapterId,
            id: path.targetInfo.id,
        },
        ..Default::default()
    };
    // SAFETY: as above.
    let named = unsafe { DisplayConfigGetDeviceInfo(&mut target.header) } == 0;
    let found = if named {
        Target {
            path: from_wide(&target.monitorDevicePath),
            friendly_name: from_wide(&target.monitorFriendlyDeviceName),
        }
    } else {
        Target::default()
    };
    Some((from_wide(&source.viewGdiDeviceName), found))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(device: &str, x: f64, primary: bool, dpi: u32) -> Monitor {
        Monitor {
            device: device.into(),
            frame: Rect::new(x, 0.0, 1920.0, 1080.0),
            work_area: Rect::new(x, 0.0, 1920.0, 1032.0),
            primary,
            dpi,
        }
    }

    #[test]
    fn the_primary_display_comes_first() {
        let displays = assemble(
            vec![
                monitor(r"\\.\DISPLAY2", -1920.0, false, 96),
                monitor(r"\\.\DISPLAY1", 0.0, true, 144),
            ],
            &HashMap::new(),
        );
        assert_eq!(displays[0].id, r"\\.\DISPLAY1");
        assert_eq!(displays[0].scale, 1.5);
        assert_eq!(displays[1].frame.x, -1920.0);
    }

    #[test]
    fn display_ids_and_names_come_from_the_monitor_path() {
        let targets = HashMap::from([(
            r"\\.\DISPLAY1".to_owned(),
            Target {
                path: r"\\?\DISPLAY#DEL4321#5&1a2b".into(),
                friendly_name: "DELL U2720Q".into(),
            },
        )]);
        let displays = assemble(
            vec![
                monitor(r"\\.\DISPLAY1", 0.0, true, 96),
                monitor(r"\\.\DISPLAY2", 1920.0, false, 96),
            ],
            &targets,
        );
        assert_eq!(displays[0].id, r"\\?\DISPLAY#DEL4321#5&1a2b");
        assert_eq!(displays[0].name, "DELL U2720Q");
        assert_eq!(displays[1].id, r"\\.\DISPLAY2");
        assert_eq!(displays[1].name, "Display 2");
    }
}
