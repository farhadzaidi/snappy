//! Reading what Snappy needs to know about one window. Every call here is a read of
//! window-manager state; none sends a message to the window's thread, so none can block
//! on a hung app.

use std::ffi::c_void;

use snappy_core::geometry::Rect;
use snappy_platform::{RecoveryKey, WindowInfo};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled;
use windows::Win32::UI::WindowsAndMessaging::{
    GA_ROOT, GW_ENABLEDPOPUP, GW_OWNER, GWL_EXSTYLE, GWL_STYLE, GetAncestor, GetWindow, GetWindowLongW,
    GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindowVisible, IsZoomed, WS_CAPTION,
    WS_EX_TOOLWINDOW, WS_THICKFRAME,
};

use crate::eligibility::WindowFacts;
use crate::identity::{Minted, Owner};
use crate::process;
use crate::shared::{ProcessFacts, Shared, lock};

/// The HWND for a 32-bit handle. Handles are sign-extended on 64-bit Windows.
pub fn hwnd(handle: u32) -> HWND {
    HWND(handle as i32 as isize as *mut c_void)
}

pub fn handle(hwnd: HWND) -> u32 {
    hwnd.0 as isize as u32
}

pub fn rect(rect: RECT) -> Rect {
    Rect::new(
        f64::from(rect.left),
        f64::from(rect.top),
        f64::from(rect.right - rect.left),
        f64::from(rect.bottom - rect.top),
    )
}

/// One window as Snappy sees it.
#[derive(Debug, Clone)]
pub struct Record {
    pub hwnd: HWND,
    pub owner: Owner,
    pub facts: WindowFacts,
    /// The visible frame.
    pub frame: Rect,
    pub process: ProcessFacts,
}

pub fn pid(hwnd: HWND) -> Option<u32> {
    let mut pid = 0u32;
    // SAFETY: a valid out pointer; a stale handle yields 0.
    let thread = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    (thread != 0 && pid != 0).then_some(pid)
}

/// The window rectangle, invisible borders included.
pub fn window_rect(hwnd: HWND) -> Option<Rect> {
    let mut bounds = RECT::default();
    // SAFETY: a valid out pointer.
    unsafe { GetWindowRect(hwnd, &mut bounds) }.ok()?;
    Some(rect(bounds))
}

/// The frame users see: the DWM extended frame bounds, else the window rectangle.
pub fn visible_frame(hwnd: HWND) -> Option<Rect> {
    let mut bounds = RECT::default();
    // SAFETY: the buffer is a RECT, as the attribute requires.
    let read = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&raw mut bounds).cast(),
            size_of::<RECT>() as u32,
        )
    };
    match read {
        Ok(()) => Some(rect(bounds)),
        Err(_) => window_rect(hwnd),
    }
}

pub fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked = 0u32;
    // SAFETY: the buffer is a u32, as the attribute requires.
    unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            (&raw mut cloaked).cast(),
            size_of::<u32>() as u32,
        )
    }
    .is_ok()
        && cloaked != 0
}

fn covers_monitor(hwnd: HWND) -> bool {
    let Some(window) = window_rect(hwnd) else {
        return false;
    };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..MONITORINFO::default()
    };
    // SAFETY: cbSize is set; a window always has a nearest monitor.
    let read = unsafe { GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut info) };
    read.as_bool() && window.contains_rect(rect(info.rcMonitor))
}

fn is_unowned_top_level(hwnd: HWND) -> bool {
    // SAFETY: plain queries.
    unsafe { GetAncestor(hwnd, GA_ROOT) == hwnd && GetWindow(hwnd, GW_OWNER).is_err() }
}

fn has_modal_popup(hwnd: HWND) -> bool {
    // SAFETY: plain queries.
    unsafe {
        !IsWindowEnabled(hwnd).as_bool()
            && GetWindow(hwnd, GW_ENABLEDPOPUP).is_ok_and(|popup| !popup.is_invalid() && popup != hwnd)
    }
}

/// Reads a window; `None` when it's gone or its process can't be identified.
pub fn read(hwnd: HWND, shared: &Shared) -> Option<Record> {
    let owner = process::owner(pid(hwnd)?)?;
    let process = shared.process(owner);
    // SAFETY: plain style reads; the bits are reinterpreted as the unsigned flags they are.
    let (style, extended) = unsafe {
        (
            GetWindowLongW(hwnd, GWL_STYLE) as u32,
            GetWindowLongW(hwnd, GWL_EXSTYLE) as u32,
        )
    };
    // SAFETY: plain queries.
    let (visible, minimized, maximized) = unsafe {
        (
            IsWindowVisible(hwnd).as_bool(),
            IsIconic(hwnd).as_bool(),
            IsZoomed(hwnd).as_bool(),
        )
    };
    let facts = WindowFacts {
        visible,
        unowned_top_level: is_unowned_top_level(hwnd),
        cloaked: is_cloaked(hwnd),
        caption: style & WS_CAPTION.0 == WS_CAPTION.0,
        sizing_border: style & WS_THICKFRAME.0 != 0,
        tool_window: extended & WS_EX_TOOLWINDOW.0 != 0,
        covers_monitor: covers_monitor(hwnd),
        elevated: process.elevated,
        minimized,
        maximized,
        has_modal_popup: has_modal_popup(hwnd),
    };
    Some(Record {
        hwnd,
        owner,
        facts,
        frame: visible_frame(hwnd)?,
        process,
    })
}

impl Record {
    /// Registers the window and describes it to the engine.
    pub fn describe(&self, shared: &Shared) -> (WindowInfo, Minted) {
        let handle = handle(self.hwnd);
        let minted = lock(&shared.registry).mint(handle, self.owner);
        let info = WindowInfo {
            id: minted.id,
            app: self.process.app.clone(),
            pid: self.owner.pid,
            frame: self.frame,
            recovery_key: Some(RecoveryKey {
                pid: self.owner.pid,
                launch_time: process::launch_seconds(self.owner),
                window: format!("{handle:#x}"),
            }),
        };
        (info, minted)
    }
}
