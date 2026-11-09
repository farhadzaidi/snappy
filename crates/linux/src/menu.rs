//! The menu's structure, built from the engine's `TrayMenu` as plain data, so the tray
//! icon on X11 and KDE Plasma and the top bar item on GNOME only have to materialize it.
//! Items carry no shortcut hints: in a tray or top bar menu they would be hints only, and no
//! key does what they say.

use snappy_platform::{PlatformEvent, TrayCommand, TrayDisplay, TrayItem, TrayMenu};

#[derive(Debug, Clone, PartialEq)]
pub enum MenuEntry {
    /// A disabled line of text.
    Label(String),
    Separator,
    Item {
        title: String,
        /// `None` for a plain item, `Some(checked)` for a checkable one.
        checked: Option<bool>,
        enabled: bool,
        event: PlatformEvent,
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
        MenuEntry::Label(heading.to_owned()),
        MenuEntry::Separator,
        command("Open Snappy…", PlatformEvent::OpenSettingsRequested),
        MenuEntry::Separator,
        MenuEntry::Label("Layouts".to_owned()),
    ];
    entries.extend(menu.displays.iter().map(display_submenu));
    entries.push(MenuEntry::Separator);
    entries.push(workspaces_submenu(&menu.workspaces));
    entries.push(MenuEntry::Separator);
    entries.push(command("Quit Snappy", PlatformEvent::QuitRequested));
    entries
}

fn command(title: &str, event: PlatformEvent) -> MenuEntry {
    MenuEntry::Item {
        title: title.to_owned(),
        checked: None,
        enabled: true,
        event,
    }
}

fn choice(item: &TrayItem, command: TrayCommand) -> MenuEntry {
    MenuEntry::Item {
        title: item.title.clone(),
        checked: Some(item.checked),
        enabled: item.enabled,
        event: PlatformEvent::Tray(command),
    }
}

fn display_submenu(display: &TrayDisplay) -> MenuEntry {
    let layout = |item: &TrayItem| {
        choice(
            item,
            TrayCommand::SelectLayout {
                display: display.id.clone(),
                layout: item.id.clone(),
            },
        )
    };
    let mut entries = vec![MenuEntry::Label("Presets".to_owned())];
    entries.extend(display.presets.iter().map(layout));
    if !display.custom.is_empty() {
        entries.push(MenuEntry::Separator);
        entries.push(MenuEntry::Label("My layouts".to_owned()));
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
            PlatformEvent::Tray(TrayCommand::ManageWorkspaces),
        ),
        MenuEntry::Separator,
    ];
    entries.extend(
        workspaces
            .iter()
            .map(|item| choice(item, TrayCommand::SwitchWorkspace(item.id.clone()))),
    );
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
            tooltip: "Snappy".into(),
            displays: vec![TrayDisplay {
                id: "DP-1".into(),
                name: "DELL U2720Q".into(),
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
                MenuEntry::Label(title) => title.clone(),
                MenuEntry::Separator => "-".into(),
                MenuEntry::Item { title, .. } | MenuEntry::Submenu { title, .. } => title.clone(),
            })
            .collect()
    }

    fn submenu(entries: &[MenuEntry], wanted: &str) -> Vec<MenuEntry> {
        entries
            .iter()
            .find_map(|entry| match entry {
                MenuEntry::Submenu { title, entries } if title == wanted => Some(entries.clone()),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn lists_heading_open_layouts_workspaces_and_quit() {
        let entries = build(&sample(true, Vec::new()));
        assert_eq!(
            titles(&entries),
            [
                "Snappy · Live",
                "-",
                "Open Snappy…",
                "-",
                "Layouts",
                "DELL U2720Q",
                "-",
                "Workspaces",
                "-",
                "Quit Snappy"
            ]
        );
        assert_eq!(titles(&build(&sample(false, Vec::new())))[0], "Snappy · Paused");
    }

    #[test]
    fn display_submenus_check_the_current_layout_and_keep_disabled_ones() {
        let entries = build(&sample(true, vec![item("mine", false, true)]));
        let layouts = submenu(&entries, "DELL U2720Q");
        assert_eq!(
            titles(&layouts),
            ["Presets", "HALVES", "THIRDS", "-", "My layouts", "MINE"]
        );
        assert!(matches!(
            &layouts[1],
            MenuEntry::Item {
                checked: Some(true),
                enabled: true,
                ..
            }
        ));
        assert!(matches!(&layouts[2], MenuEntry::Item { enabled: false, .. }));
    }

    #[test]
    fn choices_carry_their_commands() {
        let entries = build(&sample(true, Vec::new()));
        let MenuEntry::Item { event, .. } = &submenu(&entries, "DELL U2720Q")[1] else {
            panic!("expected a layout item");
        };
        assert_eq!(
            *event,
            PlatformEvent::Tray(TrayCommand::SelectLayout {
                display: "DP-1".into(),
                layout: "halves".into()
            })
        );
        let workspaces = submenu(&entries, "Workspaces");
        assert_eq!(
            titles(&workspaces),
            ["Manage workspaces…", "-", "WORKSPACE-1", "WORKSPACE-2"]
        );
        let MenuEntry::Item { event, .. } = &workspaces[3] else {
            panic!("expected a workspace item");
        };
        assert_eq!(
            *event,
            PlatformEvent::Tray(TrayCommand::SwitchWorkspace("workspace-2".into()))
        );
    }
}
