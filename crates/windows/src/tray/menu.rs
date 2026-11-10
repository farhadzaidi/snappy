//! The notification-area menu's content, built from the engine's `TrayMenu` as
//! plain data so the Win32 side only has to create it.

use snappy_platform::{PlatformEvent, TrayCommand, TrayDisplay, TrayItem, TrayMenu};

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
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
        entries: Vec<Entry>,
    },
}

pub fn build(menu: &TrayMenu) -> Vec<Entry> {
    let heading = if menu.live {
        "Snappy · Live"
    } else {
        "Snappy · Paused"
    };
    let mut entries = vec![
        Entry::Label(heading.to_owned()),
        Entry::Separator,
        command("Open Snappy…", PlatformEvent::OpenSettingsRequested),
        Entry::Separator,
        Entry::Label("Layouts".to_owned()),
    ];
    entries.extend(menu.displays.iter().map(display_submenu));
    entries.push(Entry::Separator);
    entries.push(workspaces_submenu(menu));
    entries.push(Entry::Separator);
    entries.push(command("Quit Snappy", PlatformEvent::QuitRequested));
    entries
}

fn command(title: &str, event: PlatformEvent) -> Entry {
    Entry::Item {
        title: title.to_owned(),
        checked: None,
        enabled: true,
        event,
    }
}

fn choice(item: &TrayItem, enabled: bool, command: TrayCommand) -> Entry {
    Entry::Item {
        title: item.title.clone(),
        checked: Some(item.checked),
        enabled,
        event: PlatformEvent::Tray(command),
    }
}

fn display_submenu(display: &TrayDisplay) -> Entry {
    let layout = |item: &TrayItem| {
        choice(
            item,
            item.enabled,
            TrayCommand::SelectLayout {
                display: display.id.clone(),
                layout: item.id.clone(),
            },
        )
    };
    let mut entries = vec![Entry::Label("Presets".to_owned())];
    entries.extend(display.presets.iter().map(layout));
    if !display.custom.is_empty() {
        entries.push(Entry::Separator);
        entries.push(Entry::Label("My layouts".to_owned()));
        entries.extend(display.custom.iter().map(layout));
    }
    Entry::Submenu {
        title: display.name.clone(),
        entries,
    }
}

fn workspaces_submenu(menu: &TrayMenu) -> Entry {
    let mut entries = vec![
        command(
            "Manage workspaces…",
            PlatformEvent::Tray(TrayCommand::ManageWorkspaces),
        ),
        Entry::Separator,
    ];
    entries.extend(menu.workspaces.iter().map(|item| {
        choice(
            item,
            item.enabled && menu.workspaces_running,
            TrayCommand::SwitchWorkspace(item.id.clone()),
        )
    }));
    Entry::Submenu {
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

    fn sample(live: bool, running: bool) -> TrayMenu {
        TrayMenu {
            live,
            tooltip: "Snappy".into(),
            displays: vec![TrayDisplay {
                id: r"\\?\DISPLAY#QEMU".into(),
                name: "QEMU Monitor".into(),
                presets: vec![item("halves", true, true), item("thirds", false, false)],
                custom: vec![item("mine", false, true)],
            }],
            workspaces: vec![item("workspace-1", true, true), item("workspace-2", false, true)],
            workspaces_running: running,
        }
    }

    fn titles(entries: &[Entry]) -> Vec<String> {
        entries
            .iter()
            .map(|entry| match entry {
                Entry::Label(title) => title.clone(),
                Entry::Separator => "-".into(),
                Entry::Item { title, .. } | Entry::Submenu { title, .. } => title.clone(),
            })
            .collect()
    }

    fn submenu(entries: &[Entry], wanted: &str) -> Vec<Entry> {
        entries
            .iter()
            .find_map(|entry| match entry {
                Entry::Submenu { title, entries } if title == wanted => Some(entries.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    #[test]
    fn the_menu_lists_heading_layouts_workspaces_and_quit() {
        let entries = build(&sample(true, true));
        assert_eq!(
            titles(&entries),
            [
                "Snappy · Live",
                "-",
                "Open Snappy…",
                "-",
                "Layouts",
                "QEMU Monitor",
                "-",
                "Workspaces",
                "-",
                "Quit Snappy"
            ]
        );
        assert_eq!(titles(&build(&sample(false, true)))[0], "Snappy · Paused");
    }

    #[test]
    fn layouts_check_the_current_one_and_disable_disabled_ones() {
        let layouts = submenu(&build(&sample(true, true)), "QEMU Monitor");
        assert_eq!(
            titles(&layouts),
            ["Presets", "HALVES", "THIRDS", "-", "My layouts", "MINE"]
        );
        assert!(matches!(
            &layouts[1],
            Entry::Item {
                checked: Some(true),
                enabled: true,
                ..
            }
        ));
        assert!(matches!(&layouts[2], Entry::Item { enabled: false, .. }));
    }

    #[test]
    fn workspaces_are_enabled_only_while_running() {
        let running = submenu(&build(&sample(true, true)), "Workspaces");
        assert!(matches!(
            &running[2],
            Entry::Item {
                checked: Some(true),
                enabled: true,
                ..
            }
        ));
        let stopped = submenu(&build(&sample(true, false)), "Workspaces");
        assert!(matches!(&stopped[2], Entry::Item { enabled: false, .. }));
        assert!(matches!(&stopped[0], Entry::Item { enabled: true, .. }));
    }

    #[test]
    fn choices_carry_their_commands() {
        let layouts = submenu(&build(&sample(true, true)), "QEMU Monitor");
        let Entry::Item { event, .. } = &layouts[1] else {
            panic!("not an item");
        };
        assert_eq!(
            event,
            &PlatformEvent::Tray(TrayCommand::SelectLayout {
                display: r"\\?\DISPLAY#QEMU".into(),
                layout: "halves".into()
            })
        );
    }
}
