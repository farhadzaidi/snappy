//! The layouts section: custom layouts, which display uses which, and each layout's
//! options and shortcuts.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::DisplayAssignments;
use crate::keys::KeyChord;
use crate::layout::{self, Layout};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layouts {
    pub default_layout_id: String,
    /// Each display's own layout, used while workspaces are off.
    pub displays: DisplayAssignments,
    pub custom: Vec<Layout>,
    pub options: BTreeMap<String, LayoutOptions>,
}

/// Per-layout activation and shortcuts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutOptions {
    pub enabled: bool,
    pub select_shortcut: Option<KeyChord>,
    pub zone_shortcuts: BTreeMap<String, KeyChord>,
}

impl LayoutOptions {
    pub fn empty() -> Self {
        Self {
            enabled: true,
            select_shortcut: None,
            zone_shortcuts: BTreeMap::new(),
        }
    }

    pub(super) fn is_valid(&self) -> bool {
        self.zone_shortcuts.len() <= layout::MAX_ZONES
            && self.select_shortcut.as_ref().is_none_or(KeyChord::is_valid)
            && self.zone_shortcuts.values().all(KeyChord::is_valid)
    }
}

/// A layout's own shortcuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutShortcut {
    /// Apply the layout to the display under the pointer.
    Select,
    /// Move the focused window into the zone at this index of the layout's zones.
    Zone(usize),
}

impl LayoutShortcut {
    /// The name the app and conflict messages use for the layout named `layout`.
    pub fn title(self, layout: &str) -> String {
        match self {
            LayoutShortcut::Select => format!("Select {layout}"),
            LayoutShortcut::Zone(index) => format!("Zone {} of {layout}", index + 1),
        }
    }
}
