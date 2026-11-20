//! What exported functions answer, where it isn't a `snappy-core` type as it is.

use std::collections::BTreeMap;

use serde::Serialize;
use snappy_core::keys::{KeyChord, LabelParts};
use snappy_core::layout::{UnitRect, Zone};
use snappy_core::settings::Settings;
use snappy_core::shortcuts::ShortcutSection;
use snappy_core::shortcuts::{Refusal, ShortcutId};

/// Settings put back to their defaults, whether that changes anything, and what to say.
#[derive(Serialize)]
pub struct ResetOutcome {
    pub settings: Settings,
    pub changed: bool,
    pub message: String,
}

/// Titles by the name the settings file gives each shortcut, such as `"focusUp"`.
pub type Titles = BTreeMap<String, String>;

/// The titles of a layout's own shortcuts.
#[derive(Serialize)]
pub struct LayoutShortcutTitles {
    pub select: String,
    /// One per zone, in the layout's order.
    pub zones: Vec<String>,
}

/// One section of global shortcuts, as the Settings page shows it.
#[derive(Serialize)]
pub struct SectionOfShortcuts {
    pub section: ShortcutSection,
    pub title: &'static str,
    pub shortcuts: Vec<ListedShortcut>,
}

/// A shortcut as a section lists it: what it is, its title, what's said under the title if
/// that doesn't say it all, and its chord.
#[derive(Serialize)]
pub struct ListedShortcut {
    pub id: ShortcutId,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    pub chord: Option<KeyChord>,
}

/// A chord's label as messages and assistive technology say it, and in two parts for
/// recorders, cards, and references, which draw the Snappy key their own way.
#[derive(Serialize)]
pub struct ChordLabels {
    pub long: String,
    pub parts: LabelParts,
}

/// A chord to ask the OS about before the Snappy key changes: its concrete modifiers, and
/// what to say when it is taken.
#[derive(Serialize)]
pub struct AvailabilityProbe {
    pub chord: KeyChord,
    pub refusal: Refusal,
}

#[derive(Serialize)]
pub struct ModifierNames {
    pub name: &'static str,
    /// The name with its symbol, as messages show it: "Shift ⇧".
    pub label: String,
}

/// The most of each thing the app counts that core allows.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    pub zones: usize,
    pub workspaces: usize,
    /// Window moves that undo remembers.
    pub undo_steps: usize,
    /// Characters in a layout's or a workspace's name.
    pub name_length: usize,
}

/// A layout's zones and zone shortcuts after a merge.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergedLayout {
    pub zones: Vec<Zone>,
    pub zone_shortcuts: BTreeMap<String, KeyChord>,
    /// The merged zone's ID.
    pub kept: String,
}

/// The outcome of merging zones: the merged layout, or why the zones can't be merged.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MergeOutcome {
    Merged(MergedLayout),
    Refused(String),
}

/// Where copies go, or why they can't be placed.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlaceOutcome {
    Placed(Vec<UnitRect>),
    Refused(String),
}

/// The outcome of changing the Snappy key: nothing to change, the settings with the new key,
/// or why it is refused.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SnappyKeyChange {
    Unchanged,
    Changed(Box<Settings>),
    Refused(String),
}

/// The outcome of a settings edit: nothing to change, the edited settings (not yet
/// validated), or why it is refused.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EditOutcome {
    Unchanged,
    Edited(Box<Edited>),
    Refused(String),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Edited {
    pub settings: Settings,
    /// The ID of the workspace the edit added.
    pub added_workspace: Option<String>,
}

/// The outcome of validating settings: the normalized settings, or the first problem.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Validation {
    Valid(Box<Settings>),
    Invalid(String),
}
