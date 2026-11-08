// The live tests' view of a throwaway GNOME Shell: input injected through Clutter's virtual
// devices, which GNOME Shell handles like hardware, and window frames as Mutter knows them.
// Wayland apps can't place or read their own windows, so the tests do it here. Fixture
// windows are found by their titles, which only this test helper reads.

import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

/** Snappy's top bar item: STATUS_AREA_ROLE in apps/gnome-extension/menu.js. */
const SNAPPY_ITEM = 'snappy';
/** The name Snappy's extension gives its overlay actors, in apps/gnome-extension/overlay.js. */
const SNAPPY_OVERLAY = 'snappy-overlay';

const INTERFACE = `
<node>
  <interface name="io.github.farhadzaidi.SnappyTest">
    <method name="Call">
      <arg name="request" type="s" direction="in"/>
      <arg name="reply" type="s" direction="out"/>
    </method>
  </interface>
</node>`;

/** Layers as the live tests count them: normal windows, then floating ones above. */
const NORMAL_LAYER = 0;
const FLOATING_LAYER = 3;

function now() {
    return GLib.get_monotonic_time();
}

function frameOf(window) {
    const frame = window.get_frame_rect();
    return {x: frame.x, y: frame.y, width: frame.width, height: frame.height};
}

function windowTitled(title) {
    return global.display.list_all_windows().find(window => window.get_title() === title) ?? null;
}

export default class SnappyTestExtension extends Extension {
    enable() {
        const seat = Clutter.get_default_backend().get_default_seat();
        this._pointer = seat.create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
        this._keyboard = seat.create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
        this._dbus = Gio.DBusExportedObject.wrapJSObject(INTERFACE, this);
        this._dbus.export(Gio.DBus.session, '/io/github/farhadzaidi/SnappyTest');
        this._ready = false;
        // Whether GNOME Shell is moving or resizing a window for the pointer right now.
        this._grabbing = false;
        global.display.connectObject(
            'grab-op-begin', () => {
                this._grabbing = true;
            },
            'grab-op-end', () => {
                this._grabbing = false;
            },
            this);
        if (Main.layoutManager._startingUp)
            Main.layoutManager.connectObject('startup-complete', () => this._started(), this);
        else
            this._started();
    }

    _started() {
        Main.overview.hide();
        this._ready = true;
    }

    disable() {
        Main.layoutManager.disconnectObject(this);
        global.display.disconnectObject(this);
        this._dbus.unexport();
        this._dbus = null;
        this._pointer = null;
        this._keyboard = null;
    }

    CallAsync([request], invocation) {
        const reply = result => invocation.return_value(new GLib.Variant('(s)', [JSON.stringify(result)]));
        try {
            const parsed = JSON.parse(request);
            if (parsed.op === 'screenshot') {
                this._screenshot(parsed.path).then(() => reply({ok: true}), error => reply({error: `${error}`}));
                return;
            }
            reply({ok: this._handle(parsed)});
        } catch (error) {
            reply({error: `${error.message ?? error}`});
        }
    }

    _handle(request) {
        switch (request.op) {
        case 'ready':
            return this._ready && !Main.overview.visible;
        case 'grabbing':
            return this._grabbing;
        case 'pointerMove':
            this._pointer.notify_absolute_motion(now(), request.x, request.y);
            return null;
        case 'button':
            this._pointer.notify_button(now(), Clutter.BUTTON_PRIMARY,
                request.pressed ? Clutter.ButtonState.PRESSED : Clutter.ButtonState.RELEASED);
            return null;
        case 'key':
            this._keyboard.notify_key(now(), request.keycode,
                request.pressed ? Clutter.KeyState.PRESSED : Clutter.KeyState.RELEASED);
            return null;
        case 'windowFrame': {
            const window = windowTitled(request.title);
            return window ? frameOf(window) : null;
        }
        case 'placeWindow': {
            const window = windowTitled(request.title);
            if (!window)
                throw new Error(`no window ${request.title}`);
            window.move_frame(true, request.x, request.y);
            return frameOf(window);
        }
        case 'focusWindow': {
            const window = this._window(request.title);
            Main.activateWindow(window);
            return frameOf(window);
        }
        case 'focused': {
            const window = global.display.get_focus_window();
            return window !== null && window.get_title() === request.title;
        }
        case 'releaseAll':
            // The tests name the keys they may have pressed.
            this._pointer.notify_button(now(), Clutter.BUTTON_PRIMARY, Clutter.ButtonState.RELEASED);
            for (const keycode of request.keycodes)
                this._keyboard.notify_key(now(), keycode, Clutter.KeyState.RELEASED);
            return null;
        case 'screenWindows':
            return this._screenWindows();
        case 'indicator': {
            const item = Main.panel.statusArea[SNAPPY_ITEM];
            return Boolean(item && item.visible && item.get_parent()?.visible);
        }
        case 'activeWorkspace':
            return global.workspace_manager.get_active_workspace_index();
        case 'workspaceCount':
            return global.workspace_manager.get_n_workspaces();
        case 'setDynamicWorkspaces':
            new Gio.Settings({schema_id: 'org.gnome.mutter'}).set_boolean('dynamic-workspaces', request.on);
            return null;
        case 'maximize': {
            const window = this._window(request.title);
            if (typeof window.get_maximized === 'function')
                window.maximize(Meta.MaximizeFlags.BOTH);
            else
                window.maximize();
            return null;
        }
        case 'stick':
            if (request.on)
                this._window(request.title).stick();
            else
                this._window(request.title).unstick();
            return null;
        case 'tabList':
            return global.display.get_tab_list(Meta.TabList.NORMAL_ALL, null).map(window => window.get_title());
        case 'activateMenuItem':
            return this._activateMenuItem(request.title);
        default:
            throw new Error(`unknown request ${request.op}`);
        }
    }

    _window(title) {
        const window = windowTitled(title);
        if (!window)
            throw new Error(`no window ${title}`);
        return window;
    }

    /** Chooses the item titled `title` in Snappy's top bar menu, submenus included. */
    _activateMenuItem(title) {
        const button = Main.panel.statusArea[SNAPPY_ITEM];
        if (!button)
            throw new Error('no Snappy item in the top bar');
        const pending = [button.menu];
        while (pending.length > 0) {
            const menu = pending.pop();
            for (const item of menu._getMenuItems()) {
                if (item.menu)
                    pending.push(item.menu);
                if (item.label?.text === title) {
                    item.activate(Clutter.get_current_event());
                    return true;
                }
            }
        }
        return false;
    }

    /** Front to back: Snappy's overlays while shown, then windows showing on screen. */
    _screenWindows() {
        const result = [];
        for (const actor of Main.layoutManager.uiGroup.get_children().reverse()) {
            if (actor.name !== SNAPPY_OVERLAY || !actor.visible || actor.get_n_children() === 0)
                continue;
            const [x, y] = actor.get_transformed_position();
            const [width, height] = actor.get_transformed_size();
            result.push({pid: -1, layer: FLOATING_LAYER, frame: {x, y, width, height}, overlay: true,
                focused: false});
        }
        const active = global.workspace_manager.get_active_workspace();
        const windows = global.display.sort_windows_by_stacking(global.display.list_all_windows()).reverse();
        for (const window of windows) {
            if (window.minimized || !window.located_on_workspace(active) || window.is_override_redirect())
                continue;
            result.push({pid: window.get_pid(), layer: NORMAL_LAYER, frame: frameOf(window), overlay: false,
                focused: window.has_focus()});
        }
        return result;
    }

    _screenshot(path) {
        const file = Gio.File.new_for_path(path);
        const stream = file.replace(null, false, Gio.FileCreateFlags.NONE, null);
        const shooter = new Shell.Screenshot();
        return shooter.screenshot(false, stream).finally(() => stream.close(null));
    }
}
