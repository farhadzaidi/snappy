//! Native tab groups, where a platform makes each tab a window of its own and shows only
//! the selected one, as macOS does.

use snappy_core::window::WindowId;

use crate::windows::WindowInfo;

/// One window's native tabs. Moving the shown tab moves the group.
#[derive(Debug, Clone, PartialEq)]
pub struct TabGroup {
    /// The selected tab, the only one on screen.
    pub shown: WindowInfo,
    /// How many tabs the group's tab bar holds, counting tabs the platform hasn't seen.
    pub tabs: usize,
    /// Other tabs of the group that the platform has seen. Their frames are stale until
    /// they show, and moving one changes nothing on screen.
    pub hidden: Vec<HiddenTab>,
}

impl TabGroup {
    /// The windows of the hidden tabs.
    pub fn hidden_windows(&self) -> Vec<WindowId> {
        self.hidden.iter().map(|tab| tab.window).collect()
    }
}

/// A tab of a group that isn't showing, and what showed the platform that it belongs there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HiddenTab {
    pub window: WindowId,
    pub evidence: TabEvidence,
}

/// What showed that a window is one of a group's hidden tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabEvidence {
    /// It was found in the group before, and the group still shows one of its tabs.
    Remembered,
    /// It went out of sight with the frame of the tab now showing in its place.
    SameFrame,
    /// It went out of sight where the group now shows, off by no more than a tab bar's
    /// height: a group's first new tab adds the tab bar, and a new tab can push the group
    /// up to keep it on screen.
    NearFrame,
}

impl TabEvidence {
    /// The evidence's name in the debug log.
    pub fn name(self) -> &'static str {
        match self {
            TabEvidence::Remembered => "remembered",
            TabEvidence::SameFrame => "sameFrame",
            TabEvidence::NearFrame => "nearFrame",
        }
    }
}
