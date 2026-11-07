// Snappy's hands inside GNOME Shell. On Wayland only the compositor may move other apps'
// windows, watch input, and own global shortcuts, so the Snappy app asks this extension to
// do those things over D-Bus. Every decision stays in the app.
//
// It stays on while the screen is locked, so the app keeps its connection and windows hidden
// for a Snappy workspace stay hidden; shortcuts only work in the normal session.

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

import {Service} from './service.js';

export default class SnappyExtension extends Extension {
    enable() {
        this._service = new Service(this.path);
    }

    disable() {
        this._service?.destroy();
        this._service = null;
    }
}
