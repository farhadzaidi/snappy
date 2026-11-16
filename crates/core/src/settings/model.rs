//! The settings themselves: their sections, their defaults, and the layout each display
//! uses.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{LayoutOptions, Layouts, MoveHistory, Navigation, Workspace, Workspaces};
use crate::keys::{Modifier, Os, SnappyKey};
use crate::layout::{self, DEFAULT_LAYOUT_ID, FALLBACK_LAYOUT_ID, Layout};
use crate::placement::swap::{SwapModifier, WhenTaken};
use crate::shortcuts;

pub const FORMAT_VERSION: u32 = 1;
pub const MAX_FILE_BYTES: usize = 2_000_000;
pub const MAX_CUSTOM_LAYOUTS: usize = 100;
pub const MAX_LAYOUT_OPTIONS: usize = 200;
pub const MAX_WORKSPACES: usize = 32;
/// How many workspaces the settings start with.
pub const DEFAULT_WORKSPACES: usize = 3;
pub const MAX_WORKSPACE_ID_LENGTH: usize = 128;
pub const MAX_DISPLAY_ASSIGNMENTS: usize = 32;

/// Display ID to layout ID.
pub type DisplayAssignments = BTreeMap<String, String>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub version: u32,
    pub snapping: Snapping,
    /// The modifiers every shortcut stored with the Snappy key holds. Files from before it
    /// could be chosen lack it, get the default, and have their chords read as they were
    /// then (`read_as_before_the_snappy_key`).
    #[serde(default)]
    pub snappy_key: SnappyKey,
    /// Caps Lock, held, stands for the Snappy key, and tapped alone still turns Caps Lock on
    /// or off. Off unless the user turns it on.
    #[serde(default)]
    pub caps_lock_as_snappy_key: bool,
    pub navigation: Navigation,
    pub layouts: Layouts,
    pub workspaces: Workspaces,
    #[serde(default)]
    pub history: MoveHistory,
    /// Record what Snappy does to the debug log.
    #[serde(default)]
    pub debug_log: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapping {
    pub enabled: bool,
    pub drag_modifier: DragModifier,
    /// Held during a drag, it does the opposite of `when_taken` for that drop.
    pub swap_modifier: SwapModifier,
    /// What a keyboard move or drop into a zone another window fills does.
    #[serde(default)]
    pub when_taken: WhenTaken,
    pub shortcuts_enabled: bool,
}

/// The key held while dragging to show zones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DragModifier {
    Shift,
    Alt,
    Ctrl,
}

impl DragModifier {
    pub fn modifier(self) -> Modifier {
        match self {
            DragModifier::Shift => Modifier::Shift,
            DragModifier::Alt => Modifier::Alt,
            DragModifier::Ctrl => Modifier::Ctrl,
        }
    }
}

impl Settings {
    pub fn defaults(platform: Os) -> Self {
        Self {
            version: FORMAT_VERSION,
            snapping: Snapping {
                enabled: true,
                drag_modifier: DragModifier::Shift,
                swap_modifier: match platform {
                    Os::MacOs => SwapModifier::Meta,
                    Os::Windows | Os::Linux => SwapModifier::Ctrl,
                },
                when_taken: WhenTaken::default(),
                shortcuts_enabled: true,
            },
            snappy_key: SnappyKey::DEFAULT,
            caps_lock_as_snappy_key: false,
            navigation: Navigation::defaults(),
            layouts: Layouts {
                default_layout_id: DEFAULT_LAYOUT_ID.to_owned(),
                displays: DisplayAssignments::new(),
                custom: Vec::new(),
                options: BTreeMap::new(),
            },
            workspaces: Workspaces::defaults(),
            history: MoveHistory::default(),
            debug_log: false,
        }
    }

    /// Presets first, then custom layouts.
    pub fn layouts(&self) -> impl Iterator<Item = &Layout> {
        layout::presets().iter().chain(&self.layouts.custom)
    }

    pub fn layout(&self, id: &str) -> Option<&Layout> {
        self.layouts().find(|layout| layout.id == id)
    }

    /// Stored options, or empty ones for a layout that has none.
    pub fn options(&self, layout: &Layout) -> LayoutOptions {
        self.layouts
            .options
            .get(&layout.id)
            .cloned()
            .unwrap_or_else(LayoutOptions::empty)
    }

    /// A workspace to add: `Workspace::numbered`, without the switch shortcut when another
    /// binding already uses it. A workspace keeps its shortcuts when others are reordered
    /// or deleted, so its number's chord may belong to another workspace by then.
    pub fn new_workspace(&self, id: impl Into<String>, number: usize) -> Workspace {
        let mut workspace = Workspace::numbered(id, number);
        let used = shortcuts::bound_combos(self);
        workspace.switch_shortcut = workspace
            .switch_shortcut
            .filter(|chord| !used.contains(&chord.combo(self.snappy_key)));
        workspace
    }

    pub fn is_enabled(&self, layout: &Layout) -> bool {
        self.options(layout).enabled
    }

    pub fn enabled_layouts(&self) -> impl Iterator<Item = &Layout> {
        self.layouts().filter(|layout| self.is_enabled(layout))
    }

    /// The layout a display uses now: in the active workspace while workspaces are on.
    pub fn layout_for_display(&self, display_id: &str) -> &Layout {
        let active = self
            .workspaces
            .enabled
            .then(|| self.workspaces.active())
            .flatten();
        self.layout_in(active, display_id)
    }

    /// The layout a display uses in `workspace`, or outside workspaces for `None`: the
    /// workspace's own choice, else the display's, else the default, and an enabled layout
    /// in place of one that is turned off.
    pub fn layout_in(&self, workspace: Option<&Workspace>, display_id: &str) -> &Layout {
        let chosen = workspace
            .and_then(|workspace| workspace.displays.get(display_id))
            .or_else(|| self.layouts.displays.get(display_id))
            .unwrap_or(&self.layouts.default_layout_id);

        self.layout(chosen)
            .filter(|layout| self.is_enabled(layout))
            .or_else(|| self.enabled_layouts().next())
            .or_else(|| layout::preset(FALLBACK_LAYOUT_ID))
            .unwrap_or(&layout::presets()[0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::KeyChord;
    use crate::settings::test_support::*;
    use crate::settings::{NavigationShortcut, WorkspaceStep};

    #[test]
    fn a_displays_layout_resolves_in_order() {
        let mut settings = Settings::defaults(MAC);
        assert_eq!(settings.layout_for_display("d").id, "focus");
        settings.layouts.displays.insert("d".into(), "halves".into());
        assert_eq!(settings.layout_for_display("d").id, "halves");
        settings.workspaces.enabled = true;
        settings.workspaces.items[0]
            .displays
            .insert("d".into(), "rows".into());
        assert_eq!(settings.layout_for_display("d").id, "rows");
        settings.layouts.options.insert(
            "rows".into(),
            LayoutOptions {
                enabled: false,
                ..LayoutOptions::empty()
            },
        );
        assert_eq!(
            settings.layout_for_display("d").id,
            "halves",
            "falls to the first enabled layout"
        );
        let inactive = settings.workspaces.items[1].clone();
        assert_eq!(
            settings.layout_in(Some(&inactive), "d").id,
            "halves",
            "a workspace without its own choice uses the display's"
        );
        assert_eq!(
            settings.layout_in(Some(&settings.workspaces.items[0]), "d").id,
            "halves"
        );
    }

    #[test]
    fn zones_have_no_default_shortcuts() {
        let settings = Settings::defaults(MAC).validate(MAC).unwrap();
        assert_eq!(settings.layouts.options.len(), layout::presets().len());
        assert!(
            settings
                .layouts
                .options
                .values()
                .all(|options| options.zone_shortcuts.is_empty())
        );
    }

    const PLATFORMS: [Os; 3] = [Os::MacOs, Os::Windows, Os::Linux];

    #[test]
    fn default_shortcuts_are_the_snappy_key_and_one_key_on_every_platform() {
        let combo = |chord: Option<&KeyChord>| chord.map(|chord| chord.combo(SnappyKey::DEFAULT));
        for platform in PLATFORMS {
            let settings = Settings::defaults(platform).validate(platform).unwrap();
            let workspaces = &settings.workspaces;
            let steps = WorkspaceStep::ALL.map(|step| combo(workspaces.step(step)));
            assert_eq!(
                steps,
                [
                    snappy("Semicolon"),
                    snappy("Quote"),
                    snappy("Period"),
                    snappy("Slash"),
                ],
                "{platform:?}"
            );
            let switches: Vec<_> = workspaces
                .items
                .iter()
                .map(|item| combo(item.switch_shortcut.as_ref()))
                .collect();
            assert_eq!(switches, [snappy("Digit1"), snappy("Digit2"), snappy("Digit3")]);
            assert!(
                workspaces
                    .items
                    .iter()
                    .all(|item| item.send_shortcut.is_none() && item.send_and_switch_shortcut.is_none())
            );
            let navigation = NavigationShortcut::ALL.map(|shortcut| combo(settings.navigation.get(shortcut)));
            assert_eq!(
                navigation,
                [
                    snappy("ArrowLeft"),
                    snappy("ArrowRight"),
                    snappy("ArrowUp"),
                    snappy("ArrowDown"),
                    snappy("Space"),
                    snappy("KeyI"),
                    snappy("KeyJ"),
                    snappy("KeyK"),
                    snappy("KeyL"),
                    snappy("Enter"),
                    snappy("Backslash"),
                ],
                "{platform:?}"
            );
            assert_eq!(settings.history, MoveHistory::default());

            let every_chord = shortcuts::bound_combos(&settings);
            assert_eq!(every_chord.len(), 4 + 3 + 5 + 6, "{platform:?}");
            let snappy_key = SnappyKey::DEFAULT.modifiers();
            assert!(
                every_chord.iter().all(|combo| combo.mods == snappy_key),
                "nothing but the Snappy key and one key: {every_chord:?}"
            );
        }
        assert_eq!(
            Settings::defaults(Os::Windows).workspaces,
            Settings::defaults(MAC).workspaces
        );
        assert_eq!(
            Settings::defaults(Os::Linux).navigation,
            Settings::defaults(MAC).navigation
        );
    }

    #[test]
    fn a_new_workspace_skips_its_digit_when_another_binding_uses_it() {
        let mut settings = Settings::defaults(MAC);
        let added = settings.new_workspace("workspace-4", 4);
        assert_eq!(
            added
                .switch_shortcut
                .as_ref()
                .map(|chord| chord.combo(SnappyKey::DEFAULT)),
            snappy("Digit4")
        );

        settings.workspaces.items[2].switch_shortcut = Some(KeyChord::snappy("Digit4", []));
        let added = settings.new_workspace("workspace-4", 4);
        assert_eq!(added.switch_shortcut, None);
        settings.workspaces.items.push(added);
        assert_eq!(settings.validate(MAC).map(|_| ()), Ok(()));
    }

    #[test]
    fn the_drag_modifier_starts_as_shift_and_offers_three_choices() {
        assert_eq!(
            Settings::defaults(MAC).snapping.drag_modifier,
            DragModifier::Shift
        );
        let parse = |name: &str| serde_json::from_str::<DragModifier>(&format!("\"{name}\"")).ok();
        assert_eq!(
            ["shift", "alt", "ctrl", "meta"].map(parse),
            [
                Some(DragModifier::Shift),
                Some(DragModifier::Alt),
                Some(DragModifier::Ctrl),
                None
            ]
        );
    }
}
