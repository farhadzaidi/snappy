//! What the engine keeps about running workspaces between events.

use std::collections::BTreeMap;

use snappy_core::keys::Os;
use snappy_core::workspace::{FocusPolicy, Session};
use snappy_platform::{RecoveryKey, WindowId};

use super::containment::Containment;
use super::recovery::SavedWindow;
use super::switching::DeferredSwitch;

#[derive(Default)]
pub(crate) struct WorkspaceController {
    pub session: Option<Session>,
    /// On another native desktop.
    pub paused: bool,
    /// Live control went away; nothing moves until it returns (`resume`).
    pub suspended: bool,
    pub focus: FocusPolicy,
    pub keys: BTreeMap<WindowId, RecoveryKey>,
    pub remembered: Vec<SavedWindow>,
    pub containment: Containment,
    /// Displays changed and haven't settled yet.
    pub display_update_pending: bool,
    pub(super) display_generation: u64,
    /// A switch on its way; previous/next count from here.
    pub requested: Option<String>,
    /// A switch asked for while displays were changing, run once they settle.
    pub deferred_switch: Option<DeferredSwitch>,
}

impl WorkspaceController {
    pub fn is_running(&self) -> bool {
        self.session.is_some() && !self.paused && !self.suspended
    }

    pub fn window_count(&self, workspace: &str) -> usize {
        self.session
            .as_ref()
            .map_or(0, |session| session.window_count(workspace))
    }

    pub fn is_inactive(&self, window: WindowId) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.is_inactive(window))
    }

    /// The shown tab `window` sits behind, as last learned, else `window` itself.
    pub fn tab_root(&self, window: WindowId) -> WindowId {
        self.session
            .as_ref()
            .map_or(window, |session| session.tab_root(window))
    }

    /// The windows that stand for `windows`' tab groups, each once (`Session::tab_roots`).
    pub fn tab_roots(&self, windows: &[WindowId]) -> Vec<WindowId> {
        match &self.session {
            Some(session) => session.tab_roots(windows),
            None => windows.to_vec(),
        }
    }

    pub fn is_hidden_tab(&self, window: WindowId) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.is_hidden_tab(window))
    }

    pub fn pause_message(&self, platform: Os) -> Option<String> {
        self.paused
            .then_some(match platform {
                Os::MacOs => "Paused on another macOS desktop. Return to the original desktop to resume.",
                _ => "Paused on another virtual desktop. Return to the original desktop to resume.",
            })
            .map(str::to_owned)
    }
}
