//! The menu bar menu's structure, built from the engine's `TrayMenu` as plain
//! data so AppKit only has to materialize it.

use snappy_platform::{TrayCommand, TrayDisplay, TrayItem, TrayMenu};

#[derive(Debug, Clone, PartialEq)]
pub enum MenuAction {
    OpenSettings,
    Quit,
    Tray(TrayCommand),
}

#[derive(Debug, Clone, PartialEq)]
pub enum MenuEntry {
    /// A disabled line of text.
    Heading(String),
    SectionHeader(String),
    Separator,
    Item {
        title: String,
        /// Command plus this key, or empty.
        key: &'static str,
        checked: bool,
        enabled: bool,
        action: MenuAction,
    },
    Submenu {
        title: String,
        entries: Vec<MenuEntry>,
    },
}

pub fn build(menu: &TrayMenu) -> Vec<MenuEntry> {
    let heading = if menu.live {
        "Snappy · Live"
    } else {
        "Snappy · Paused"
    };
    let mut entries = vec![
        MenuEntry::Heading(heading.to_owned()),
        MenuEntry::Separator,
        command("Open Snappy…", ",", MenuAction::OpenSettings),
        MenuEntry::SectionHeader("Layouts".to_owned()),
    ];
    entries.extend(menu.displays.iter().map(display_submenu));
    entries.push(MenuEntry::Separator);
    entries.push(workspaces_submenu(&menu.workspaces));
    entries.push(MenuEntry::Separator);
    entries.push(command("Quit Snappy", "q", MenuAction::Quit));
    entries
}

fn command(title: &str, key: &'static str, action: MenuAction) -> MenuEntry {
    MenuEntry::Item {
        title: title.to_owned(),
        key,
        checked: false,
        enabled: true,
        action,
    }
}

fn choice(item: &TrayItem, action: MenuAction) -> MenuEntry {
    MenuEntry::Item {
        title: item.title.clone(),
        key: "",
        checked: item.checked,
        enabled: item.enabled,
        action,
    }
}

fn display_submenu(display: &TrayDisplay) -> MenuEntry {
    let layout = |item: &TrayItem| {
        choice(
            item,
            MenuAction::Tray(TrayCommand::SelectLayout {
                display: display.id.clone(),
                layout: item.id.clone(),
            }),
        )
    };
    let mut entries = vec![MenuEntry::SectionHeader("Presets".to_owned())];
    entries.extend(display.presets.iter().map(layout));
    if !display.custom.is_empty() {
        entries.push(MenuEntry::Separator);
        entries.push(MenuEntry::SectionHeader("Your layouts".to_owned()));
        entries.extend(display.custom.iter().map(layout));
    }
    MenuEntry::Submenu {
        title: display.name.clone(),
        entries,
    }
}

fn workspaces_submenu(workspaces: &[TrayItem]) -> MenuEntry {
    let mut entries = vec![
        command(
            "Manage workspaces…",
            "",
            MenuAction::Tray(TrayCommand::ManageWorkspaces),
        ),
        MenuEntry::Separator,
    ];
    entries.extend(workspaces.iter().map(|item| {
        choice(
            item,
            MenuAction::Tray(TrayCommand::SwitchWorkspace(item.id.clone())),
        )
    }));
    MenuEntry::Submenu {
        title: "Workspaces".to_owned(),
        entries,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, checked: bool, enabled: bool) -> TrayItem {
        TrayItem {
            id: id.into(),
            title: id.to_uppercase(),
            checked,
            enabled,
        }
    }

    fn sample(live: bool, custom: Vec<TrayItem>) -> TrayMenu {
        TrayMenu {
            live,
            tooltip: "Snappy — window zones".into(),
            displays: vec![TrayDisplay {
                id: "d1".into(),
                name: "Studio Display".into(),
                presets: vec![item("halves", true, true), item("thirds", false, false)],
                custom,
            }],
            workspaces: vec![item("workspace-1", true, true), item("workspace-2", false, true)],
            workspaces_running: true,
        }
    }

    fn titles(entries: &[MenuEntry]) -> Vec<String> {
        entries
            .iter()
            .map(|entry| match entry {
                MenuEntry::Heading(title) | MenuEntry::SectionHeader(title) => title.clone(),
                MenuEntry::Separator => "-".into(),
                MenuEntry::Item { title, .. } | MenuEntry::Submenu { title, .. } => title.clone(),
            })
            .collect()
    }

    fn submenu<'a>(entries: &'a [MenuEntry], name: &str) -> &'a [MenuEntry] {
        entries
            .iter()
            .find_map(|entry| match entry {
                MenuEntry::Submenu { title, entries } if title == name => Some(entries.as_slice()),
                _ => None,
            })
            .unwrap_or_default()
    }

    #[test]
    fn top_level_items_come_in_order_with_their_key_equivalents() {
        let entries = build(&sample(true, vec![]));
        assert_eq!(
            titles(&entries),
            vec![
                "Snappy · Live",
                "-",
                "Open Snappy…",
                "Layouts",
                "Studio Display",
                "-",
                "Workspaces",
                "-",
                "Quit Snappy"
            ]
        );
        assert!(matches!(
            &entries[2],
            MenuEntry::Item {
                key: ",",
                action: MenuAction::OpenSettings,
                ..
            }
        ));
        assert!(matches!(
            &entries[8],
            MenuEntry::Item {
                key: "q",
                action: MenuAction::Quit,
                ..
            }
        ));
    }

    #[test]
    fn heading_reads_paused_when_not_live() {
        assert_eq!(titles(&build(&sample(false, vec![])))[0], "Snappy · Paused");
    }

    #[test]
    fn display_submenu_lists_presets_then_custom_layouts() {
        let entries = build(&sample(true, vec![item("mine", false, true)]));
        let layouts = submenu(&entries, "Studio Display");
        assert_eq!(
            titles(layouts),
            vec!["Presets", "HALVES", "THIRDS", "-", "Your layouts", "MINE"]
        );
        assert_eq!(
            layouts[2],
            MenuEntry::Item {
                title: "THIRDS".into(),
                key: "",
                checked: false,
                enabled: false,
                action: MenuAction::Tray(TrayCommand::SelectLayout {
                    display: "d1".into(),
                    layout: "thirds".into()
                }),
            }
        );
        assert!(matches!(&layouts[1], MenuEntry::Item { checked: true, .. }));
    }

    #[test]
    fn without_custom_layouts_has_no_your_layouts_section() {
        let entries = build(&sample(true, vec![]));
        assert_eq!(
            titles(submenu(&entries, "Studio Display")),
            vec!["Presets", "HALVES", "THIRDS"]
        );
    }

    #[test]
    fn workspaces_submenu_manages_and_switches() {
        let entries = build(&sample(true, vec![]));
        let workspaces = submenu(&entries, "Workspaces");
        assert_eq!(
            titles(workspaces),
            vec!["Manage workspaces…", "-", "WORKSPACE-1", "WORKSPACE-2"]
        );
        assert!(matches!(
            &workspaces[0],
            MenuEntry::Item {
                action: MenuAction::Tray(TrayCommand::ManageWorkspaces),
                ..
            }
        ));
        assert_eq!(
            workspaces[2],
            MenuEntry::Item {
                title: "WORKSPACE-1".into(),
                key: "",
                checked: true,
                enabled: true,
                action: MenuAction::Tray(TrayCommand::SwitchWorkspace("workspace-1".into())),
            }
        );
    }
}
