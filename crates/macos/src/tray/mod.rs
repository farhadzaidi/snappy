//! The menu bar item and its menu. Main thread only.

mod menu;

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::sel;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAccessibility, NSControlStateValueOff, NSControlStateValueOn, NSImage, NSMenu, NSMenuItem,
    NSSquareStatusItemLength, NSStatusBar, NSStatusItem,
};
use objc2_foundation::{NSString, ns_string};
use snappy_platform::{PlatformEvent, TrayMenu};

use crate::delegate::Delegate;
use crate::events::emit;
use menu::{MenuAction, MenuEntry};

const ICON_SYMBOL: &str = "rectangle.split.3x1";

struct Tray {
    item: Retained<NSStatusItem>,
    latest: TrayMenu,
    /// Actions of the menu last built, indexed by item tag.
    actions: Vec<MenuAction>,
}

thread_local! {
    static TRAY: RefCell<Option<Tray>> = const { RefCell::new(None) };
}

/// Adds the status item. Its menu stays empty until it first opens.
pub fn install(delegate: &Delegate, mtm: MainThreadMarker) {
    let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSSquareStatusItemLength);
    if let Some(button) = item.button(mtm) {
        let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str(ICON_SYMBOL),
            Some(ns_string!("Snappy")),
        );
        if let Some(image) = image {
            image.setTemplate(true);
            button.setImage(Some(&image));
        }
    }
    let menu = NSMenu::new(mtm);
    menu.setAutoenablesItems(false);
    menu.setDelegate(Some(ProtocolObject::from_ref(delegate)));
    item.setMenu(Some(&menu));
    TRAY.with(|tray| {
        *tray.borrow_mut() = Some(Tray {
            item,
            latest: TrayMenu::default(),
            actions: Vec::new(),
        })
    });
}

/// Keeps the latest content and refreshes the tooltip.
pub fn update(menu: TrayMenu, mtm: MainThreadMarker) {
    TRAY.with(|tray| {
        let mut tray = tray.borrow_mut();
        let Some(tray) = tray.as_mut() else { return };
        if let Some(button) = tray.item.button(mtm) {
            let tooltip = NSString::from_str(&menu.tooltip);
            button.setToolTip(Some(&tooltip));
            button.setAccessibilityLabel(Some(&tooltip));
        }
        tray.latest = menu;
    });
}

/// Fills `menu` from the latest content, as it opens.
pub fn rebuild(menu: &NSMenu, target: &Delegate, mtm: MainThreadMarker) {
    TRAY.with(|tray| {
        let mut tray = tray.borrow_mut();
        let Some(tray) = tray.as_mut() else { return };
        let entries = menu::build(&tray.latest);
        tray.actions.clear();
        menu.removeAllItems();
        add_entries(menu, &entries, target, &mut tray.actions, mtm);
    });
}

/// Forwards the chosen item's action.
pub fn choose(tag: isize) {
    let action = TRAY.with(|tray| {
        let tray = tray.borrow();
        let index = usize::try_from(tag).ok()?;
        tray.as_ref()?.actions.get(index).cloned()
    });
    let event = match action {
        Some(MenuAction::OpenSettings) => PlatformEvent::OpenSettingsRequested,
        Some(MenuAction::Quit) => PlatformEvent::QuitRequested,
        Some(MenuAction::Tray(command)) => PlatformEvent::Tray(command),
        None => return,
    };
    emit(event);
}

fn add_entries(
    menu: &NSMenu,
    entries: &[MenuEntry],
    target: &Delegate,
    actions: &mut Vec<MenuAction>,
    mtm: MainThreadMarker,
) {
    for entry in entries {
        let item = match entry {
            MenuEntry::Heading(title) => heading(title, mtm),
            MenuEntry::SectionHeader(title) => {
                NSMenuItem::sectionHeaderWithTitle(&NSString::from_str(title), mtm)
            }
            MenuEntry::Separator => NSMenuItem::separatorItem(mtm),
            MenuEntry::Item {
                title,
                key,
                checked,
                enabled,
                action,
            } => {
                let item = action_item(title, key, target, actions.len(), mtm);
                item.setState(if *checked {
                    NSControlStateValueOn
                } else {
                    NSControlStateValueOff
                });
                item.setEnabled(*enabled);
                actions.push(action.clone());
                item
            }
            MenuEntry::Submenu { title, entries } => {
                let submenu = NSMenu::new(mtm);
                submenu.setAutoenablesItems(false);
                add_entries(&submenu, entries, target, actions, mtm);
                let item = plain_item(title, mtm);
                item.setSubmenu(Some(&submenu));
                item
            }
        };
        menu.addItem(&item);
    }
}

fn plain_item(title: &str, mtm: MainThreadMarker) -> Retained<NSMenuItem> {
    // SAFETY: designated initializer with no action.
    unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            None,
            ns_string!(""),
        )
    }
}

fn heading(title: &str, mtm: MainThreadMarker) -> Retained<NSMenuItem> {
    let item = plain_item(title, mtm);
    item.setEnabled(false);
    item
}

fn action_item(
    title: &str,
    key: &str,
    target: &Delegate,
    tag: usize,
    mtm: MainThreadMarker,
) -> Retained<NSMenuItem> {
    // SAFETY: `chooseMenuItem:` is implemented by `Delegate`, which outlives the menu.
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            Some(sel!(chooseMenuItem:)),
            &NSString::from_str(key),
        )
    };
    let target: &AnyObject = target;
    // SAFETY: as above.
    unsafe { item.setTarget(Some(target)) };
    item.setTag(isize::try_from(tag).unwrap_or(isize::MAX));
    item
}
