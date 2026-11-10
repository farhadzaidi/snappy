//! The engine's menu as the extension's top bar item draws it. Each choice gets a number for
//! the extension to send back, which maps to the event it stands for.

use snappy_linux::menu::{self as shared, MenuEntry as Shared};
use snappy_platform::{PlatformEvent, TrayMenu};

use crate::protocol::MenuEntry;

/// The entries to draw, and what each numbered choice does.
pub fn build(menu: &TrayMenu) -> (Vec<MenuEntry>, Vec<PlatformEvent>) {
    let mut actions = Vec::new();
    let entries = convert(&shared::build(menu), &mut actions);
    (entries, actions)
}

fn convert(entries: &[Shared], actions: &mut Vec<PlatformEvent>) -> Vec<MenuEntry> {
    entries
        .iter()
        .map(|entry| match entry {
            Shared::Label(title) => MenuEntry::Label { title: title.clone() },
            Shared::Separator => MenuEntry::Separator,
            Shared::Item {
                title,
                checked,
                enabled,
                event,
                ..
            } => {
                let id = u32::try_from(actions.len()).unwrap_or(u32::MAX);
                actions.push(event.clone());
                MenuEntry::Item {
                    id,
                    title: title.clone(),
                    checked: *checked,
                    enabled: *enabled,
                }
            }
            Shared::Submenu { title, entries } => MenuEntry::Submenu {
                title: title.clone(),
                entries: convert(entries, actions),
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use snappy_platform::{TrayCommand, TrayDisplay, TrayItem};

    use super::*;

    fn item(id: &str, checked: bool) -> TrayItem {
        TrayItem {
            id: id.into(),
            title: id.to_uppercase(),
            checked,
            enabled: true,
        }
    }

    #[test]
    fn every_choice_maps_back_to_its_event() {
        let menu = TrayMenu {
            live: true,
            tooltip: "Snappy".into(),
            displays: vec![TrayDisplay {
                id: "DP-1".into(),
                name: "Dell".into(),
                presets: vec![item("halves", true)],
                custom: Vec::new(),
            }],
            workspaces: vec![item("one", true)],
            workspaces_running: true,
        };
        let (entries, actions) = build(&menu);
        assert_eq!(
            entries[0],
            MenuEntry::Label {
                title: "Snappy · Live".into()
            }
        );
        assert!(actions.contains(&PlatformEvent::OpenSettingsRequested));
        assert!(actions.contains(&PlatformEvent::QuitRequested));
        assert!(actions.contains(&PlatformEvent::Tray(TrayCommand::SelectLayout {
            display: "DP-1".into(),
            layout: "halves".into()
        })));
        assert!(actions.contains(&PlatformEvent::Tray(TrayCommand::SwitchWorkspace("one".into()))));

        let MenuEntry::Item { id, .. } = entries
            .iter()
            .find(|entry| matches!(entry, MenuEntry::Item { title, .. } if title == "Quit Snappy"))
            .unwrap()
        else {
            panic!("Quit is an item");
        };
        assert_eq!(actions[*id as usize], PlatformEvent::QuitRequested);
    }
}
