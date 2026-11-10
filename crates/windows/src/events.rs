//! WinEvent hooks, turned into `PlatformEvent`s on the main thread.
//!
//! Global hooks see foreground changes, move and size loops (which stand in for
//! primary-button presses, so ordinary clicks cost nothing), un-minimizing, and windows
//! appearing and closing. Location changes are frequent, so they're hooked per process,
//! only for processes whose windows Snappy has described. Cloaking reveals virtual
//! desktop switches, which the desktop guard confirms.

use std::collections::HashMap;

use snappy_core::geometry::Point;
use snappy_core::keys::{Modifier, Mods};
use snappy_platform::PlatformEvent;
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CHILDID_SELF, EVENT_OBJECT_CLOAKED, EVENT_OBJECT_DESTROY, EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_SHOW,
    EVENT_OBJECT_UNCLOAKED, EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MOVESIZEEND,
    EVENT_SYSTEM_MOVESIZESTART, GetCursorPos, OBJID_WINDOW, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
};

use crate::main_state::{Main, with_main};
use crate::managed::record::{self, handle};
use crate::shared::lock;

/// Installed hooks; each is removed when no longer wanted and all of them at teardown.
#[derive(Default)]
pub struct Hooks {
    global: Vec<HWINEVENTHOOK>,
    locations: HashMap<u32, HWINEVENTHOOK>,
}

fn hook(first: u32, last: u32, pid: u32, flags: u32) -> Option<HWINEVENTHOOK> {
    // SAFETY: an out-of-context hook with a static callback; events arrive on this
    // thread's message loop.
    let hook = unsafe { SetWinEventHook(first, last, None, Some(on_event), pid, 0, flags) };
    (!hook.is_invalid()).then_some(hook)
}

fn unhook(hook: HWINEVENTHOOK) {
    // SAFETY: a hook this thread installed, removed once.
    let _ = unsafe { UnhookWinEvent(hook) };
}

impl Hooks {
    /// The always-on hooks. Ranges are kept to single events where neighbors, such as
    /// mouse capture changes, would fire on every click.
    pub fn install() -> Hooks {
        let others = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
        let global = [
            (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, 0, others),
            (EVENT_SYSTEM_MOVESIZESTART, EVENT_SYSTEM_MOVESIZEEND, 0, others),
            (EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZEEND, 0, others),
            (EVENT_OBJECT_DESTROY, EVENT_OBJECT_SHOW, 0, others),
            (EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED, 0, others),
        ]
        .into_iter()
        .filter_map(|(first, last, pid, flags)| hook(first, last, pid, flags))
        .collect();
        Hooks {
            global,
            locations: HashMap::new(),
        }
    }

    /// Reports moves of `pid`'s windows.
    pub fn watch_process(&mut self, pid: u32) {
        if self.locations.contains_key(&pid) {
            return;
        }
        let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
        if let Some(hook) = hook(
            EVENT_OBJECT_LOCATIONCHANGE,
            EVENT_OBJECT_LOCATIONCHANGE,
            pid,
            flags,
        ) {
            self.locations.insert(pid, hook);
        }
    }

    pub fn unwatch_process(&mut self, pid: u32) {
        if let Some(hook) = self.locations.remove(&pid) {
            unhook(hook);
        }
    }

    pub fn remove_all(&mut self) {
        for hook in self
            .global
            .drain(..)
            .chain(self.locations.drain().map(|(_, hook)| hook))
        {
            unhook(hook);
        }
    }
}

unsafe extern "system" fn on_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    object: i32,
    child: i32,
    _thread: u32,
    _time: u32,
) {
    let whole_window = object == OBJID_WINDOW.0 && child == CHILDID_SELF as i32;
    with_main(|main| main.window_event(event, hwnd, whole_window));
}

/// The pointer position now.
pub fn cursor() -> Point {
    let mut point = POINT::default();
    // SAFETY: a valid out pointer.
    let _ = unsafe { GetCursorPos(&mut point) };
    Point::new(f64::from(point.x), f64::from(point.y))
}

fn is_down(vk: u16) -> bool {
    // SAFETY: a plain query.
    let state = unsafe { GetAsyncKeyState(i32::from(vk)) };
    state < 0
}

/// The modifiers held now.
pub fn modifiers() -> Mods {
    [
        (Modifier::Shift, is_down(VK_SHIFT.0)),
        (Modifier::Ctrl, is_down(VK_CONTROL.0)),
        (Modifier::Alt, is_down(VK_MENU.0)),
        (Modifier::Meta, is_down(VK_LWIN.0) || is_down(VK_RWIN.0)),
    ]
    .into_iter()
    .filter(|(_, held)| *held)
    .map(|(modifier, _)| modifier)
    .collect()
}

impl Main {
    fn window_event(&mut self, event: u32, hwnd: HWND, whole_window: bool) {
        match event {
            EVENT_SYSTEM_FOREGROUND => self.foreground_changed(hwnd),
            EVENT_SYSTEM_MOVESIZESTART => self.drag_started(hwnd),
            EVENT_SYSTEM_MOVESIZEEND => self.drag_ended(hwnd),
            EVENT_SYSTEM_MINIMIZEEND => self.moved(hwnd),
            EVENT_OBJECT_LOCATIONCHANGE if whole_window => {
                if self.drag == Some(hwnd) {
                    self.dragged();
                } else {
                    self.moved(hwnd);
                }
            }
            EVENT_OBJECT_DESTROY if whole_window => self.destroyed(hwnd),
            EVENT_OBJECT_SHOW if whole_window => self.shown(hwnd),
            EVENT_OBJECT_CLOAKED | EVENT_OBJECT_UNCLOAKED if whole_window && self.guard.changed() => {
                self.emit(PlatformEvent::NativeDesktopChanged)
            }
            _ => {}
        }
    }

    fn foreground_changed(&mut self, hwnd: HWND) {
        if let Some(pid) = record::pid(hwnd) {
            self.emit(PlatformEvent::FocusChanged {
                pid,
                by_activation: true,
            });
        }
        self.follow_keyboard_layout(hwnd);
    }

    /// A move or size loop began: the press that started it, naming the window when
    /// Snappy manages it. Otherwise the engine's own lookup explains the refusal.
    fn drag_started(&mut self, hwnd: HWND) {
        if !self.shared.monitoring().presses() {
            return;
        }
        self.drag = Some(hwnd);
        let window = record::read(hwnd, &self.shared)
            .filter(|record| record.facts.check().is_ok())
            .map(|record| {
                let (info, minted) = record.describe(&self.shared);
                if minted.new_process {
                    self.hooks.watch_process(info.pid);
                }
                info.id
            });
        self.emit(PlatformEvent::PointerDown {
            at: cursor(),
            mods: modifiers(),
            window,
        });
    }

    fn drag_ended(&mut self, hwnd: HWND) {
        if self.drag != Some(hwnd) {
            return;
        }
        self.drag = None;
        self.emit(PlatformEvent::PointerUp {
            at: cursor(),
            mods: modifiers(),
        });
    }

    fn dragged(&mut self) {
        if self.shared.monitoring().drag {
            self.emit(PlatformEvent::PointerDragged {
                at: cursor(),
                mods: modifiers(),
            });
        }
    }

    fn moved(&mut self, hwnd: HWND) {
        let id = lock(&self.shared.registry).lookup(handle(hwnd));
        if let Some(id) = id {
            self.emit(PlatformEvent::WindowMoved(id));
        }
    }

    fn destroyed(&mut self, hwnd: HWND) {
        let removed = lock(&self.shared.registry).remove(handle(hwnd));
        let Some(removed) = removed else {
            return;
        };
        if self.drag == Some(hwnd) {
            self.drag = None;
        }
        if removed.last_of_process {
            self.hooks.unwatch_process(removed.pid);
            self.shared.forget_process(removed.pid);
        }
        self.emit(PlatformEvent::WindowDestroyed(removed.id));
    }

    /// A new top-level window Snappy could manage appeared.
    fn shown(&mut self, hwnd: HWND) {
        let Some(record) = record::read(hwnd, &self.shared) else {
            return;
        };
        if record.facts.check().is_ok() {
            self.emit(PlatformEvent::WindowCreated {
                pid: record.owner.pid,
            });
        }
    }
}
