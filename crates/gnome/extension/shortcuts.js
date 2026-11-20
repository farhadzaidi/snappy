// Global shortcuts through GNOME Shell's accelerator grabs. The app sends each chord as
// physical keycodes and modifiers, so shortcuts follow the key on every keyboard layout. A
// grab is refused when GNOME Shell, Mutter, or another program already binds the chord, which
// doubles as the availability probe.
//
// Accelerators fire on presses alone. To hear a shortcut that reports being let go come up,
// the extension takes a keyboard grab of GNOME Shell's own from its press until its key comes
// up, as the window switcher does to see Alt let go; meanwhile keys go to the grab, not to
// apps. Its modifiers are sampled each frame meanwhile too, and it's let go once they aren't
// all held, so the grab never outlasts the Snappy key. When the grab is refused, the sampling
// alone follows it, and letting go of its key while the modifiers stay down goes unseen until
// it's pressed again. A chord without modifiers is let go at once.

import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';

import {SAMPLE_INTERVAL_MS} from './input.js';

/** Shortcuts work in the normal session only, not in the overview or on the lock screen. */
const ACTION_MODES = Shell.ActionMode.NORMAL;

/**
 * Grabs `accelerator`, without key repeats, in the normal session. Returns the grab's action,
 * or `Meta.KeyBindingAction.NONE` when something already binds the chord.
 */
export function grab(accelerator) {
    const action = global.display.grab_accelerator(accelerator, Meta.KeyBindingFlags.IGNORE_AUTOREPEAT);
    if (action !== Meta.KeyBindingAction.NONE)
        Main.wm.allowKeybinding(Meta.external_binding_name_for_action(action), ACTION_MODES);
    return action;
}

export function ungrab(action) {
    Main.wm.allowKeybinding(Meta.external_binding_name_for_action(action), Shell.ActionMode.NONE);
    global.display.ungrab_accelerator(action);
}

export class Shortcuts {
    constructor(emit) {
        this._emit = emit;
        /** Grab action to the app's shortcut: `{id, release}`. */
        this._actions = new Map();
        /** The shortcut down now, for one that reports being let go: `{id, keycode, mask}`. */
        this._held = null;
        this._sampler = 0;
        /** The keyboard grab while the shortcut is held, and the actor it gives the keys to. */
        this._grab = null;
        this._grabActor = null;
        global.display.connectObject('accelerator-activated', (_display, action) => {
            const shortcut = this._actions.get(action);
            if (shortcut !== undefined)
                this._pressed(shortcut);
        }, this);
    }

    /**
     * Replaces every grab with `keys`, `[{id, accelerator, release}]`, where `release`,
     * `{keycode, mask}`, asks to report the shortcut let go. A shortcut held is forgotten,
     * its release unreported. Returns the IDs refused.
     */
    register(keys) {
        this.unregisterAll();
        const refused = [];
        for (const {id, accelerator, release} of keys) {
            const action = grab(accelerator);
            if (action === Meta.KeyBindingAction.NONE)
                refused.push(id);
            else
                this._actions.set(action, {id, release});
        }
        return refused;
    }

    unregisterAll() {
        this._forget();
        for (const action of this._actions.keys())
            ungrab(action);
        this._actions.clear();
    }

    /**
     * Reports a press, and follows a shortcut that reports being let go from now on. A press
     * while one is held means that one's release went unseen. It stays unreported, so the
     * press is taken for a new one rather than for the end of a hold.
     */
    _pressed({id, release}) {
        this._forget();
        this._emit({type: 'hotkey', id});
        if (release === undefined)
            return;
        this._held = {id, keycode: release.keycode, mask: release.mask};
        if (release.mask !== 0)
            this._grabKeyboard();
        this._sampler = GLib.timeout_add(GLib.PRIORITY_DEFAULT, SAMPLE_INTERVAL_MS, () => {
            try {
                this._sample();
            } catch (error) {
                logError(error, 'Snappy: following a held shortcut');
            }
            return this._sampler === 0 ? GLib.SOURCE_REMOVE : GLib.SOURCE_CONTINUE;
        });
        this._sample();
    }

    /** Let go once the chord's modifiers aren't all held. A chord without any can't be followed. */
    _sample() {
        if (this._held === null)
            return;
        const [, , state] = global.get_pointer();
        const {mask} = this._held;
        if (mask === 0 || (state & mask) !== mask)
            this._release();
    }

    /**
     * Takes the keyboard, so the held key's coming up reaches the extension. Its repeats, and
     * any other key, go nowhere meanwhile. GNOME Shell refuses the grab while something of
     * its own holds the keyboard; the sampling then follows the shortcut alone.
     */
    _grabKeyboard() {
        if (this._grabActor === null) {
            this._grabActor = new Clutter.Actor({reactive: true});
            this._grabActor.connect('key-release-event', (_actor, event) => {
                if (this._held !== null && event.get_key_code() === this._held.keycode)
                    this._release();
                return Clutter.EVENT_STOP;
            });
            this._grabActor.connect('key-press-event', () => Clutter.EVENT_STOP);
            Main.uiGroup.add_child(this._grabActor);
        }
        const taken = Main.pushModal(this._grabActor, {actionMode: ACTION_MODES});
        if ((taken.get_seat_state() & Clutter.GrabState.KEYBOARD) === 0) {
            Main.popModal(taken);
            return;
        }
        this._grab = taken;
    }

    _release() {
        if (this._held === null)
            return;
        const {id} = this._held;
        this._forget();
        this._emit({type: 'hotkeyReleased', id});
    }

    /** Stops following the shortcut held, if any, without reporting it let go. */
    _forget() {
        this._held = null;
        this._stopFollowing();
    }

    /** Stops sampling, and gives the keyboard back. */
    _stopFollowing() {
        if (this._sampler !== 0)
            GLib.source_remove(this._sampler);
        this._sampler = 0;
        if (this._grab !== null)
            Main.popModal(this._grab);
        this._grab = null;
    }

    /** Whether nothing binds `accelerator`: grabbed and released straight away. */
    isFree(accelerator) {
        const action = grab(accelerator);
        if (action === Meta.KeyBindingAction.NONE)
            return false;
        ungrab(action);
        return true;
    }

    destroy() {
        this.unregisterAll();
        global.display.disconnectObject(this);
        this._grabActor?.destroy();
        this._grabActor = null;
    }
}
