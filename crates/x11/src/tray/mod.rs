//! The tray icon: a StatusNotifierItem on the session bus. GNOME shows it only
//! with the AppIndicator extension.

use std::sync::mpsc::Sender;

use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::{CheckmarkItem, StandardItem, SubMenu};
use ksni::{Icon, MenuItem, ToolTip, Tray};
use snappy_platform::{PlatformEvent, TrayMenu};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};

use snappy_linux::menu::{MenuEntry, build};

const ICON_SIZES: [u32; 2] = [22, 44];

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
            key,
            checked,
            enabled,
            event,
        } => {
            let label = escape(&title);
            let shortcut = if key.is_empty() {
                Vec::new()
            } else {
                vec![vec!["Control".to_owned(), key.to_owned()]]
            };
            let activate = Box::new(move |tray: &mut SnappyTray| {
                let _ = tray.events.send(event.clone());
            });
            match checked {
                Some(checked) => CheckmarkItem {
                    label,
                    enabled,
                    checked,
                    shortcut,
                    activate,
                    ..CheckmarkItem::default()
                }
                .into(),
                None => StandardItem {
                    label,
                    enabled,
                    shortcut,
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
            icons: ICON_SIZES.into_iter().filter_map(icon).collect(),
        };
        match tray.assume_sni_available(true).spawn() {
            Ok(handle) => Some(TrayIcon { handle }),
            Err(error) => {
                eprintln!("snappy: no tray icon: {error}");
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

/// A white two-zone outline: a rounded frame split by a divider, in ARGB32 network order.
fn icon(size: u32) -> Option<Icon> {
    let mut pixmap = Pixmap::new(size, size)?;
    let unit = size as f32 / 22.0;
    let mut paint = Paint::default();
    paint.set_color(Color::WHITE);
    paint.anti_alias = true;
    let stroke = Stroke {
        width: 1.6 * unit,
        ..Stroke::default()
    };
    let frame = PathBuilder::from_rect(Rect::from_xywh(3.0 * unit, 4.0 * unit, 16.0 * unit, 14.0 * unit)?);
    pixmap.stroke_path(&frame, &paint, &stroke, Transform::identity(), None);
    let left = PathBuilder::from_rect(Rect::from_xywh(5.0 * unit, 6.0 * unit, 6.0 * unit, 10.0 * unit)?);
    pixmap.fill_path(&left, &paint, FillRule::Winding, Transform::identity(), None);

    let data = pixmap
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|&[r, g, b, a]| [a, r, g, b])
        .collect();
    Some(Icon {
        width: size as i32,
        height: size as i32,
        data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_are_argb_in_network_order() {
        let icon = icon(22).unwrap();
        assert_eq!(icon.data.len(), 22 * 22 * 4);
        // Inside the filled left zone: opaque white.
        let offset = ((10 * 22 + 7) * 4) as usize;
        assert_eq!(&icon.data[offset..offset + 4], &[255, 255, 255, 255]);
        // The corner is clear.
        assert_eq!(&icon.data[0..4], &[0, 0, 0, 0]);
    }

    #[test]
    fn labels_keep_their_underscores() {
        assert_eq!(escape("my_layout"), "my__layout");
    }
}
