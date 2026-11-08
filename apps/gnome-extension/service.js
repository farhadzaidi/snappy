// The D-Bus service the Snappy app talks to. One method takes a JSON request and returns a
// JSON reply; one signal carries every event as JSON, in the order it happened. The version
// lets an app and an extension from different Snappy releases notice each other.

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';

import {Drags} from './input.js';
import {Menu} from './menu.js';
import {Overlays} from './overlay.js';
import {Parking} from './parking.js';
import {Shortcuts} from './shortcuts.js';
import * as Windows from './windows.js';

/** Changes whenever a request, reply, or event changes shape. The app checks it. */
export const PROTOCOL_VERSION = 3;

const OBJECT_PATH = '/io/github/farhadzaidi/Snappy/Shell';
const INTERFACE = `
<node>
  <interface name="io.github.farhadzaidi.Snappy.Shell">
    <property name="Version" type="u" access="read"/>
    <method name="Call">
      <arg name="request" type="s" direction="in"/>
      <arg name="reply" type="s" direction="out"/>
    </method>
    <signal name="Event">
      <arg name="event" type="s"/>
    </signal>
  </interface>
</node>`;

/**
 * Requests that leave something behind for the app: grabs, monitoring, followed windows,
 * overlays, a menu.
 */
const STATEFUL = new Set(['monitoring', 'register', 'follow', 'overlayPrepare', 'overlayShow', 'menu']);

export class Service {
    /** Sets everything up, or, when any part fails, takes down what was set up and throws. */
    constructor(extensionPath) {
        this._app = null;
        this._appWatch = 0;
        try {
            const emit = event => this._emitEvent(event);
            this._parking = new Parking(emit);
            this._events = new Windows.WindowEvents(emit);
            this._drags = new Drags(emit);
            this._shortcuts = new Shortcuts(emit);
            this._overlays = new Overlays();
            this._menu = new Menu(extensionPath, emit);
            Main.layoutManager.connectObject('monitors-changed', () => emit({type: 'displays'}), this);
            global.display.connectObject('workareas-changed', () => emit({type: 'displays'}), this);
            Main.sessionMode.connectObject('updated', () => this._sessionModeChanged(), this);

            this._dbus = Gio.DBusExportedObject.wrapJSObject(INTERFACE, this);
            this._dbus.export(Gio.DBus.session, OBJECT_PATH);
            this._emitEvent({type: 'ready'});
        } catch (error) {
            this.destroy();
            throw error;
        }
    }

    /**
     * The extension keeps running on the lock screen so hidden windows stay hidden; nothing
     * of Snappy's shows there.
     */
    _sessionModeChanged() {
        const locked = Main.sessionMode.isLocked;
        if (locked)
            this._overlays.hide();
        this._menu.setVisible(!locked);
    }

    get Version() {
        return PROTOCOL_VERSION;
    }

    CallAsync([request], invocation) {
        let reply;
        try {
            const parsed = JSON.parse(request);
            if (STATEFUL.has(parsed.op))
                this._watchApp(invocation.get_sender());
            reply = {ok: this._handle(parsed)};
        } catch (error) {
            reply = {error: `${error.message ?? error}`};
        }
        invocation.return_value(new GLib.Variant('(s)', [JSON.stringify(reply)]));
    }

    _emitEvent(event) {
        this._dbus?.emit_signal('Event', new GLib.Variant('(s)', [JSON.stringify(event)]));
    }

    /**
     * Remembers the app that set things up, so they go away with it: a crashed app must not
     * leave its shortcuts grabbed or its overlay on screen. Parked windows stay parked; the
     * app brings them back when it starts again.
     */
    _watchApp(sender) {
        if (sender === this._app)
            return;
        this._unwatchApp();
        this._app = sender;
        this._appWatch = Gio.bus_watch_name_on_connection(
            Gio.DBus.session, sender, Gio.BusNameWatcherFlags.NONE, null, () => this._appGone());
    }

    _unwatchApp() {
        if (this._appWatch !== 0)
            Gio.bus_unwatch_name(this._appWatch);
        this._appWatch = 0;
        this._app = null;
    }

    _appGone() {
        this._unwatchApp();
        this._shortcuts.unregisterAll();
        this._drags.setMonitoring(false, false, 0);
        this._events.unfollowAll();
        this._overlays.clear();
        this._menu.remove();
    }

    _window(id) {
        const window = Windows.findWindow(id);
        if (window === null)
            throw new Error('gone');
        return window;
    }

    _record(window) {
        return window === null ? null : Windows.describe(window, this._parking);
    }

    _handle(request) {
        switch (request.op) {
        case 'windows':
            return Windows.managedWindows().map(window => this._record(window));
        case 'window':
            return this._record(Windows.findWindow(request.id));
        case 'focused':
            return this._record(global.display.get_focus_window());
        case 'follow':
            this._events.follow(request.ids);
            return null;
        case 'setPosition':
            Windows.setPosition(this._window(request.id), request.x, request.y);
            return null;
        case 'setSize':
            Windows.setSize(this._window(request.id), request.width, request.height);
            return null;
        case 'unmaximize':
            Windows.restore(this._window(request.id));
            return null;
        case 'activate':
            Windows.activate(this._window(request.id));
            return null;
        case 'clearFocus':
            Windows.clearFocus();
            return null;
        case 'pointer': {
            const [x, y] = global.get_pointer();
            return {x, y};
        }
        case 'monitors':
            return this._monitors();
        case 'monitoring':
            this._drags.setMonitoring(request.press, request.drag, request.follow ?? 0);
            return null;
        case 'register':
            return this._shortcuts.register(request.keys);
        case 'unregisterAll':
            this._shortcuts.unregisterAll();
            return null;
        case 'isFree':
            return this._shortcuts.isFree(request.accelerator);
        case 'overlayPrepare':
            this._overlays.prepare(request.display, request.scene);
            return null;
        case 'overlayShow':
            this._overlays.show(request.display, request.scene);
            return null;
        case 'overlayHide':
            this._overlays.hide();
            return null;
        case 'overlayRetain':
            this._overlays.retain(request.displays);
            return null;
        case 'menu':
            this._menu.update(request.name, request.generation, request.entries);
            return null;
        case 'bind':
            this._parking.bind();
            return null;
        case 'onBoundWorkspace':
            return this._parking.onBoundWorkspace();
        case 'clearWorkspaces':
            return this._parking.clear();
        case 'park':
            return this._parking.park(this._window(request.id));
        case 'unpark':
            this._parking.unpark(this._window(request.id));
            return null;
        default:
            throw new Error(`unknown request ${request.op}`);
        }
    }

    _monitors() {
        const display = global.display;
        const workspace = global.workspace_manager.get_active_workspace();
        const monitors = [];
        for (let index = 0; index < display.get_n_monitors(); index++) {
            const frame = display.get_monitor_geometry(index);
            const work = workspace.get_work_area_for_monitor(index);
            monitors.push({
                index,
                primary: index === display.get_primary_monitor(),
                frame: {x: frame.x, y: frame.y, width: frame.width, height: frame.height},
                workArea: {x: work.x, y: work.y, width: work.width, height: work.height},
                scale: display.get_monitor_scale(index),
            });
        }
        return monitors;
    }

    /** Takes down whatever was set up; safe to call on a half-built service. */
    destroy() {
        Main.layoutManager.disconnectObject(this);
        global.display.disconnectObject(this);
        Main.sessionMode.disconnectObject(this);
        this._unwatchApp();
        this._drags?.destroy();
        this._shortcuts?.destroy();
        this._overlays?.clear();
        this._menu?.remove();
        this._events?.destroy();
        this._parking?.destroy();
        this._dbus?.unexport();
        this._dbus = null;
    }
}
