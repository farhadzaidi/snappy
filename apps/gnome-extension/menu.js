// Snappy's top bar item, standing in for the tray icon other desktops show. The app sends
// the menu's entries; choosing one sends back the entry's ID. The item shows only while the
// app is running.

import Gio from 'gi://Gio';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';

/** The item's name in the top bar's status area. */
export const STATUS_AREA_ROLE = 'snappy';

export class Menu {
    constructor(extensionPath, emit) {
        this._iconPath = `${extensionPath}/icons/snappy-symbolic.svg`;
        this._emit = emit;
        this._button = null;
    }

    _ensureButton(tooltip) {
        if (!this._button) {
            this._button = new PanelMenu.Button(0.0, 'Snappy', false);
            this._button.add_child(new St.Icon({
                gicon: Gio.icon_new_for_string(this._iconPath),
                style_class: 'system-status-icon',
            }));
            Main.panel.addToStatusArea(STATUS_AREA_ROLE, this._button);
        }
        this._button.accessible_name = tooltip;
        return this._button;
    }

    /** Replaces the menu with `entries`. */
    update(tooltip, entries) {
        const button = this._ensureButton(tooltip);
        button.menu.removeAll();
        this._addEntries(button.menu, entries);
    }

    _addEntries(menu, entries) {
        for (const entry of entries)
            menu.addMenuItem(this._item(entry));
    }

    _item(entry) {
        switch (entry.kind) {
        case 'label':
            return new PopupMenu.PopupMenuItem(entry.title, {reactive: false, can_focus: false});
        case 'separator':
            return new PopupMenu.PopupSeparatorMenuItem();
        case 'submenu': {
            const submenu = new PopupMenu.PopupSubMenuMenuItem(entry.title);
            this._addEntries(submenu.menu, entry.entries);
            return submenu;
        }
        default: {
            const item = new PopupMenu.PopupMenuItem(entry.title);
            if (entry.checked !== null)
                item.setOrnament(entry.checked ? PopupMenu.Ornament.CHECK : PopupMenu.Ornament.NONE);
            item.setSensitive(entry.enabled);
            item.connect('activate', () => this._emit({type: 'menu', item: entry.id}));
            return item;
        }
        }
    }

    /** Takes the item out of the top bar, as when the app quits. */
    remove() {
        this._button?.destroy();
        this._button = null;
    }

    destroy() {
        this.remove();
    }
}
