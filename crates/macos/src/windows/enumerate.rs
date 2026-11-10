//! Listing windows: visible ones, every one, and the ones filling zones at a swap drop.

use objc2::rc::Retained;
use objc2_app_kit::NSRunningApplication;
use snappy_core::error::WindowError;
use snappy_core::geometry::Rect;
use snappy_platform::WindowInfo;

use super::operations::has_sheet;
use super::{Windows, server, window_facts};
use crate::apps;
use crate::ax::{self, AxElement, names};
use crate::eligibility::AppFacts;
use crate::messages::{
    SWAP_LIST_UNREADABLE, SWAP_NEEDS_PERMISSION, SWAP_NOT_RESPONDING, SWAP_UNSUPPORTED_OCCUPANT, unavailable,
};
use crate::occupancy::{
    ServerWindow, SwapCandidate, is_on_screen, match_occupants, occupants_by_process, same_frame, unambiguous,
};
use crate::thresholds::{AX_ENUMERATION_TIMEOUT, AX_RECOVERY_TIMEOUT};

type App = (Retained<NSRunningApplication>, AppFacts);

impl Windows {
    /// Visible managed windows on the current Space, optionally of one app.
    pub fn visible_windows(&self, only: Option<&str>) -> Vec<WindowInfo> {
        if !ax::is_trusted() {
            return Vec::new();
        }
        let rows = server::on_screen_windows().unwrap_or_default();
        managed_apps()
            .filter(|(_, facts)| !facts.hidden && only.is_none_or(|only| facts.identifier == only))
            .flat_map(|(_, facts)| self.visible_windows_of(&facts, &rows))
            .collect()
    }

    /// Eligible, unminimized windows that the window server shows on screen. Windows
    /// sharing an identical frame are skipped unless one is focused.
    fn visible_windows_of(&self, facts: &AppFacts, rows: &[ServerWindow]) -> Vec<WindowInfo> {
        let Some(application) = AxElement::application(facts.pid, AX_ENUMERATION_TIMEOUT) else {
            return Vec::new();
        };
        let candidates: Vec<(AxElement, Rect)> = app_windows(&application, AX_ENUMERATION_TIMEOUT)
            .into_iter()
            .filter(|window| window_facts(window).check().is_ok() && !is_minimized(window))
            .filter_map(|window| window.frame().ok().map(|frame| (window, frame)))
            .filter(|(_, frame)| is_on_screen(*frame, facts.pid, rows))
            .collect();
        let focused = application.element_attribute(names::FOCUSED_WINDOW);
        let focused =
            focused.and_then(|focused| candidates.iter().position(|(window, _)| *window == focused));
        let frames: Vec<Rect> = candidates.iter().map(|(_, frame)| *frame).collect();
        candidates
            .iter()
            .zip(unambiguous(&frames, focused))
            .filter(|(_, keep)| *keep)
            .map(|((window, frame), _)| self.describe(window, *frame, facts))
            .collect()
    }

    /// Every managed window, including minimized, hidden, and parked ones. Journal
    /// replay relies on this list, so it uses the recovery timeout.
    pub fn all_windows(&self) -> Vec<WindowInfo> {
        if !ax::is_trusted() {
            return Vec::new();
        }
        managed_apps()
            .flat_map(|(_, facts)| {
                let Some(application) = AxElement::application(facts.pid, AX_RECOVERY_TIMEOUT) else {
                    return Vec::new();
                };
                app_windows(&application, AX_RECOVERY_TIMEOUT)
                    .into_iter()
                    .filter_map(|window| self.info(&window, &facts).ok())
                    .collect()
            })
            .collect()
    }

    /// The windows filling any of `zones` at a swap drop: window-server frames
    /// that fill a zone, matched to their owners' Accessibility windows. Any doubt
    /// cancels the swap with its message.
    pub fn windows_occupying(&self, zones: &[Rect]) -> Result<Vec<WindowInfo>, WindowError> {
        if !ax::is_trusted() {
            return Err(unavailable(SWAP_NEEDS_PERMISSION));
        }
        let rows = server::on_screen_windows().ok_or_else(|| unavailable(SWAP_LIST_UNREADABLE))?;
        let mut found = Vec::new();
        for (pid, frames) in occupants_by_process(&rows, zones, apps::own_pid()) {
            let app = apps::running(pid).ok_or_else(|| unavailable(SWAP_UNSUPPORTED_OCCUPANT))?;
            let facts = apps::facts(&app);
            if !facts.is_supported() {
                return Err(unavailable(SWAP_UNSUPPORTED_OCCUPANT));
            }
            found.extend(self.occupants_of(&facts, &frames)?);
        }
        Ok(found)
    }

    fn occupants_of(&self, facts: &AppFacts, frames: &[Rect]) -> Result<Vec<WindowInfo>, WindowError> {
        let application = AxElement::application(facts.pid, AX_ENUMERATION_TIMEOUT)
            .ok_or_else(|| unavailable(SWAP_NOT_RESPONDING))?;
        let windows = application
            .elements_attribute(names::WINDOWS)
            .map_err(|_| unavailable(SWAP_NOT_RESPONDING))?;
        let candidates: Vec<SwapCandidate> = windows
            .iter()
            .map(|window| {
                window.set_timeout(AX_ENUMERATION_TIMEOUT);
                swap_candidate(window, frames)
            })
            .collect();
        let matched = match_occupants(frames, &candidates)?;
        Ok(matched
            .into_iter()
            .filter_map(|index| Some(self.describe(&windows[index], candidates[index].frame?, facts)))
            .collect())
    }
}

/// Running apps whose windows Snappy may manage.
fn managed_apps() -> impl Iterator<Item = App> {
    apps::all_running()
        .into_iter()
        .map(|app| {
            let facts = apps::facts(&app);
            (app, facts)
        })
        .filter(|(_, facts)| facts.is_supported())
}

/// An app's windows, each set to answer within `timeout`. Unresponsive apps list none.
fn app_windows(application: &AxElement, timeout: f32) -> Vec<AxElement> {
    let windows = application.elements_attribute(names::WINDOWS).unwrap_or_default();
    for window in &windows {
        window.set_timeout(timeout);
    }
    windows
}

fn is_minimized(window: &AxElement) -> bool {
    window.bool_attribute(names::MINIMIZED).unwrap_or(false)
}

/// Reads a window's frame, and its swappability only when the frame matches one of
/// `frames`, since the others are never consulted.
fn swap_candidate(window: &AxElement, frames: &[Rect]) -> SwapCandidate {
    let frame = window.frame().ok();
    let relevant = frame.is_some_and(|frame| frames.iter().any(|server| same_frame(frame, *server)));
    SwapCandidate {
        frame,
        minimized: relevant && is_minimized(window),
        movable: !relevant || (window_facts(window).check().is_ok() && !has_sheet(window)),
    }
}
