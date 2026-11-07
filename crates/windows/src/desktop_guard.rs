//! The native desktop guard. Workspaces belong to the virtual desktop that was
//! current when they started.
//!
//! The public `IVirtualDesktopManager` only knows windows that have a taskbar button, so
//! an invisible anchor window of Snappy's own can't stand in for a desktop without
//! showing a button. Instead the guard remembers the bound desktop's ID and asks about
//! any app window that lives there, usually a workspace member, parked or not. Desktop
//! switches cloak and uncloak windows, which the cloaking hook turns into checks.

use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
use windows::Win32::UI::Shell::{IVirtualDesktopManager, VirtualDesktopManager};
use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, IsWindow, IsWindowVisible};
use windows::core::{BOOL, GUID};

#[derive(Default)]
pub struct Guard {
    manager: Option<IVirtualDesktopManager>,
    /// The bound desktop, when one could be identified.
    bound: Option<GUID>,
    /// A window last seen on the bound desktop.
    reference: Option<HWND>,
    /// The answer last reported, so only changes become events.
    on_bound: bool,
}

unsafe extern "system" fn collect(window: HWND, data: LPARAM) -> BOOL {
    // SAFETY: `data` is the vector `visible_top_level` passed, alive for the enumeration.
    let found = unsafe { &mut *(data.0 as *mut Vec<isize>) };
    // SAFETY: a plain query.
    if unsafe { IsWindowVisible(window) }.as_bool() {
        found.push(window.0 as isize);
    }
    BOOL(1)
}

fn visible_top_level() -> Vec<HWND> {
    let mut handles: Vec<isize> = Vec::new();
    // SAFETY: the callback only pushes into `handles`, which outlives the call.
    let _ = unsafe { EnumWindows(Some(collect), LPARAM(&raw mut handles as isize)) };
    handles.into_iter().map(|handle| HWND(handle as *mut _)).collect()
}

impl Guard {
    fn manager(&mut self) -> Option<&IVirtualDesktopManager> {
        if self.manager.is_none() {
            // SAFETY: COM is initialized on this thread by the app.
            self.manager = unsafe { CoCreateInstance(&VirtualDesktopManager, None, CLSCTX_ALL) }.ok();
        }
        self.manager.as_ref()
    }

    /// The desktop `window` belongs to; `None` for windows without a taskbar button.
    fn desktop_of(manager: &IVirtualDesktopManager, window: HWND) -> Option<GUID> {
        // SAFETY: a valid interface; a stale window yields an error.
        unsafe { manager.GetWindowDesktopId(window) }
            .ok()
            .filter(|id| *id != GUID::zeroed())
    }

    fn is_current(manager: &IVirtualDesktopManager, window: HWND) -> Option<bool> {
        // SAFETY: as above.
        unsafe { manager.IsWindowOnCurrentVirtualDesktop(window) }
            .ok()
            .map(|on| on.as_bool())
    }

    /// Binds workspaces to the current virtual desktop.
    pub fn bind(&mut self) {
        self.clear();
        let Some(manager) = self.manager().cloned() else {
            return;
        };
        let found = visible_top_level().into_iter().find_map(|window| {
            let id = Self::desktop_of(&manager, window)?;
            Self::is_current(&manager, window)?.then_some((id, window))
        });
        if let Some((id, window)) = found {
            self.bound = Some(id);
            self.reference = Some(window);
        }
        self.on_bound = true;
    }

    /// A window on the bound desktop: the last one found while it still lives there,
    /// else any other.
    fn reference(&mut self, manager: &IVirtualDesktopManager, bound: GUID) -> Option<HWND> {
        let still_there = self.reference.filter(|window| {
            // SAFETY: a plain query.
            unsafe { IsWindow(Some(*window)) }.as_bool() && Self::desktop_of(manager, *window) == Some(bound)
        });
        self.reference = still_there.or_else(|| {
            visible_top_level()
                .into_iter()
                .find(|window| Self::desktop_of(manager, *window) == Some(bound))
        });
        self.reference
    }

    /// True while the bound desktop is current. With nothing bound, or no window left on
    /// the bound desktop to ask about, workspaces stay in control.
    pub fn on_managed_desktop(&mut self) -> bool {
        let (Some(bound), Some(manager)) = (self.bound, self.manager.clone()) else {
            return true;
        };
        self.reference(&manager, bound)
            .and_then(|window| Self::is_current(&manager, window))
            .unwrap_or(true)
    }

    /// After a window was cloaked or uncloaked: whether that moved the user onto or off
    /// the bound desktop.
    pub fn changed(&mut self) -> bool {
        if self.bound.is_none() {
            return false;
        }
        let now = self.on_managed_desktop();
        let changed = now != self.on_bound;
        self.on_bound = now;
        changed
    }

    pub fn clear(&mut self) {
        self.bound = None;
        self.reference = None;
    }

    /// Releases the COM interface before COM shuts down.
    pub fn close(&mut self) {
        self.clear();
        self.manager = None;
    }
}
