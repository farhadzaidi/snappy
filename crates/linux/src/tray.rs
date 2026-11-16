//! The tray icon: a StatusNotifierItem on the session bus, which KDE Plasma and most X11
//! desktops show. GNOME shows it only with the AppIndicator extension.

use std::sync::mpsc::Sender;

use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::{CheckmarkItem, StandardItem, SubMenu};
use ksni::{Icon, MenuItem, ToolTip, Tray};
use snappy_platform::{PlatformEvent, TrayMenu, glyph};

use crate::menu::{MenuEntry, build};

const ICON_SIZES: [usize; 2] = [22, 44];

pub struct SnappyTray {
    menu: TrayMenu,
    events: Sender<PlatformEvent>,
    icons: Vec<Icon>,
}

impl Tray for SnappyTray {
    const MENU_ON_ACTIVATE: bool = true;

    fn id(&self) -> String {
        "snappy".into()
    }

    fn title(&self) -> String {
        "Snappy".into()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        self.icons.clone()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: self.menu.tooltip.clone(),
            ..ToolTip::default()
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        build(&self.menu).into_iter().map(item).collect()
    }
}

fn item(entry: MenuEntry) -> MenuItem<SnappyTray> {
    match entry {
        MenuEntry::Label(title) => StandardItem {
            label: escape(&title),
            enabled: false,
            ..StandardItem::default()
        }
        .into(),
        MenuEntry::Separator => MenuItem::Separator,
        MenuEntry::Item {
            title,
            checked,
            enabled,
            event,
        } => {
            let label = escape(&title);
            let activate = Box::new(move |tray: &mut SnappyTray| {
                let _ = tray.events.send(event.clone());
            });
            match checked {
                Some(checked) => CheckmarkItem {
                    label,
                    enabled,
                    checked,
                    activate,
                    ..CheckmarkItem::default()
                }
                .into(),
                None => StandardItem {
                    label,
                    enabled,
                    activate,
                    ..StandardItem::default()
                }
                .into(),
            }
        }
        MenuEntry::Submenu { title, entries } => SubMenu {
            label: escape(&title),
            submenu: entries.into_iter().map(item).collect(),
            ..SubMenu::default()
        }
        .into(),
    }
}

/// Menu labels treat a single underscore as a mnemonic marker.
fn escape(label: &str) -> String {
    label.replace('_', "__")
}

/// The running tray service.
pub struct TrayIcon {
    handle: Handle<SnappyTray>,
}

impl TrayIcon {
    /// Registers the item, or `None` without a session bus. A missing tray host is not
    /// an error: the item appears once a desktop shell starts watching.
    pub fn start(events: Sender<PlatformEvent>) -> Option<TrayIcon> {
        let tray = SnappyTray {
            menu: TrayMenu::default(),
            events,
            icons: ICON_SIZES.into_iter().map(icon).collect(),
        };
        match tray.assume_sni_available(true).spawn() {
            Ok(handle) => Some(TrayIcon { handle }),
            Err(error) => {
                crate::journal::warn(format_args!("no tray icon: {error}"));
                None
            }
        }
    }

    pub fn update(&self, menu: TrayMenu) {
        self.handle.update(move |tray| tray.menu = menu);
    }

    pub fn stop(self) {
        self.handle.shutdown().wait();
    }
}

/// The glyph in white, as ARGB32 in network order.
fn icon(size: usize) -> Icon {
    let data = glyph::mask(size)
        .into_iter()
        .flat_map(|alpha| [alpha, 255, 255, 255])
        .collect();
    Icon {
        width: size as i32,
        height: size as i32,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_are_argb_in_network_order() {
        let icon = icon(22);
        assert_eq!(icon.data.len(), 22 * 22 * 4);
        // Inside the lit half zone: opaque white.
        let offset = (10 * 22 + 5) * 4;
        assert_eq!(&icon.data[offset..offset + 4], &[255, 255, 255, 255]);
        // The corner is clear.
        assert_eq!(icon.data[0], 0);
    }

    #[test]
    fn menu_items_show_no_shortcut_hints() {
        let (events, _) = std::sync::mpsc::channel();
        let tray = SnappyTray {
            menu: TrayMenu::default(),
            events,
            icons: Vec::new(),
        };
        let items = tray.menu();
        assert!(!items.is_empty());
        for item in items {
            if let MenuItem::Standard(item) = item {
                assert!(item.shortcut.is_empty(), "{} shows a shortcut", item.label);
            }
        }
    }

    #[test]
    fn labels_keep_their_underscores() {
        assert_eq!(escape("my_layout"), "my__layout");
    }
}
