// Global shortcuts through GNOME Shell's accelerator grabs. The app sends each chord as
// physical keycodes and modifiers, so shortcuts follow the key on every keyboard layout. A
// grab is refused when GNOME Shell, Mutter, or another program already binds the chord, which
// doubles as the availability probe.

import Meta from 'gi://Meta';
import Shell from 'gi://Shell';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';

/** Shortcuts work in the normal session only, not in the overview or on the lock screen. */
const ACTION_MODES = Shell.ActionMode.NORMAL;

export class Shortcuts {
    constructor(emit) {
        this._emit = emit;
        /** Grab action to the app's shortcut ID. */
        this._actions = new Map();
        global.display.connectObject('accelerator-activated', (_display, action) => {
            const id = this._actions.get(action);
            if (id !== undefined)
                this._emit({type: 'hotkey', id});
        }, this);
    }

    _grab(accelerator) {
        const action = global.display.grab_accelerator(accelerator, Meta.KeyBindingFlags.IGNORE_AUTOREPEAT);
        if (action !== Meta.KeyBindingAction.NONE)
            Main.wm.allowKeybinding(Meta.external_binding_name_for_action(action), ACTION_MODES);
        return action;
    }

    _ungrab(action) {
        Main.wm.allowKeybinding(Meta.external_binding_name_for_action(action), Shell.ActionMode.NONE);
        global.display.ungrab_accelerator(action);
    }

    /** Replaces every grab with `keys`, `[{id, accelerator}]`. Returns the IDs refused. */
    register(keys) {
        this.unregisterAll();
        const refused = [];
        for (const {id, accelerator} of keys) {
            const action = this._grab(accelerator);
            if (action === Meta.KeyBindingAction.NONE)
                refused.push(id);
            else
                this._actions.set(action, id);
        }
        return refused;
    }

    unregisterAll() {
        for (const action of this._actions.keys())
            this._ungrab(action);
        this._actions.clear();
    }

    /** Whether nothing binds `accelerator`: grabbed and released straight away. */
    isFree(accelerator) {
        const action = this._grab(accelerator);
        if (action === Meta.KeyBindingAction.NONE)
            return false;
        this._ungrab(action);
        return true;
    }

    destroy() {
        this.unregisterAll();
        global.display.disconnectObject(this);
    }
}
