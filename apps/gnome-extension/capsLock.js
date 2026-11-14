// Caps Lock as the Snappy key. The app writes an XKB option of its own, snappy:caps, whose
// Caps Lock sets the Snappy key's modifiers while held and keeps a real Caps Lock on Shift +
// Caps Lock. This adds the option to the keymap GNOME Shell builds, without touching the
// user's settings, and reports each press of Caps Lock alone so the app can tell a tap from a
// hold. Only while Caps Lock is down, every other key is grabbed with the Snappy key's
// modifiers and the pointer's buttons are sampled each frame, so the app hears that
// something else was used, never what.

import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';

import {keyboardManager, rebuildKeymap} from './compat.js';
import {grab, ungrab} from './shortcuts.js';

/** The app's XKB option, in `crates/linux/src/xkb.rs`. */
const OPTION = 'snappy:caps';
/** Caps Lock's keycode with nothing held, so a Shift + Caps Lock still reaches the real one. */
const CAPS_LOCK = '0x42';
/** About one sample per frame at 60 Hz, as during drags. */
const SAMPLE_INTERVAL_MS = 16;

const BUTTON_MASK =
    Clutter.ModifierType.BUTTON1_MASK | Clutter.ModifierType.BUTTON2_MASK |
    Clutter.ModifierType.BUTTON3_MASK | Clutter.ModifierType.BUTTON4_MASK |
    Clutter.ModifierType.BUTTON5_MASK;

export class CapsLock {
    constructor(emit) {
        this._emit = emit;
        /** What the app asked for: `{modifiers, mask, keycodes}`, while Caps Lock is the Snappy key. */
        this._held = null;
        this._locked = false;
        /** The manager whose options are wrapped, while they are. */
        this._wrapped = null;
        this._capsAction = Meta.KeyBindingAction.NONE;
        /** Grabs of the other keys, while Caps Lock is down. */
        this._keyActions = new Set();
        /** The press in progress: whether the modifiers were seen and a button reported. */
        this._press = null;
        this._sampler = 0;
        this._keyboard = null;
        global.display.connectObject('accelerator-activated', (_display, action) => this._activated(action), this);
    }

    _manager() {
        const manager = keyboardManager();
        if (manager === null)
            throw new Error('unsupported');
        return manager;
    }

    /** The XKB options GNOME builds from the user's settings, without the app's. */
    options() {
        const manager = this._manager();
        return Object.getPrototypeOf(manager)._buildOptionsString.call(manager);
    }

    /**
     * Makes Caps Lock the Snappy key: `modifiers` is the accelerator prefix of the Snappy key,
     * `mask` the modifier bits it sets, and `keycodes` the keys grabbed while it is down.
     * Applies the keymap again, so new symbols the app wrote take effect.
     */
    hold(held) {
        const manager = this._manager();
        this._held = held;
        this._update(manager);
    }

    restore() {
        this._held = null;
        const manager = keyboardManager();
        if (manager !== null)
            this._update(manager);
    }

    /** On the lock screen Caps Lock is a plain Caps Lock: the option and the grab go. */
    setLocked(locked) {
        if (locked === this._locked)
            return;
        this._locked = locked;
        const manager = keyboardManager();
        if (manager !== null && this._held !== null)
            this._update(manager);
    }

    /**
     * Turns Caps Lock on or off as its key does. GNOME's virtual keyboard finds Caps_Lock on
     * Shift + Caps Lock in the app's keymap and holds Shift for it.
     */
    toggle() {
        if (this._keyboard === null) {
            const seat = Clutter.get_default_backend().get_default_seat();
            this._keyboard = seat.create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
        }
        const time = GLib.get_monotonic_time();
        this._keyboard.notify_keyval(time, Clutter.KEY_Caps_Lock, Clutter.KeyState.PRESSED);
        this._keyboard.notify_keyval(time, Clutter.KEY_Caps_Lock, Clutter.KeyState.RELEASED);
    }

    /** Wraps or unwraps the options, grabs or lets go of Caps Lock, and applies the keymap. */
    _update(manager) {
        const active = this._held !== null && !this._locked;
        this._endPress(false);
        if (active) {
            this._wrap(manager);
            if (this._capsAction === Meta.KeyBindingAction.NONE)
                this._capsAction = grab(CAPS_LOCK);
        } else {
            this._unwrap();
            if (this._capsAction !== Meta.KeyBindingAction.NONE)
                ungrab(this._capsAction);
            this._capsAction = Meta.KeyBindingAction.NONE;
        }
        rebuildKeymap(manager);
    }

    _wrap(manager) {
        if (this._wrapped === manager)
            return;
        this._unwrap();
        const build = Object.getPrototypeOf(manager)._buildOptionsString;
        manager._buildOptionsString = function () {
            const options = build.call(this);
            return options === '' ? OPTION : `${options},${OPTION}`;
        };
        this._wrapped = manager;
    }

    _unwrap() {
        if (this._wrapped !== null)
            delete this._wrapped._buildOptionsString;
        this._wrapped = null;
    }

    _activated(action) {
        if (action === this._capsAction && action !== Meta.KeyBindingAction.NONE)
            this._pressed();
        else if (this._keyActions.has(action))
            this._emit({type: 'capsLock', phase: 'otherInput'});
    }

    _pressed() {
        if (this._press !== null)
            return;
        this._press = {seen: false, button: false};
        this._emit({type: 'capsLock', phase: 'pressed'});
        const {modifiers, keycodes} = this._held;
        for (const keycode of keycodes) {
            const action = grab(`${modifiers}0x${keycode.toString(16)}`);
            if (action !== Meta.KeyBindingAction.NONE)
                this._keyActions.add(action);
        }
        this._sampler = GLib.timeout_add(GLib.PRIORITY_DEFAULT, SAMPLE_INTERVAL_MS, () => {
            try {
                this._sample();
            } catch (error) {
                logError(error, 'Snappy: following Caps Lock');
            }
            return this._sampler === 0 ? GLib.SOURCE_REMOVE : GLib.SOURCE_CONTINUE;
        });
        this._sample();
    }

    /**
     * Caps Lock is up once the Snappy key's modifiers it set are, having been seen down, so a
     * sample taken before they show can't end the press. A button pressed meanwhile counts as
     * something else used.
     */
    _sample() {
        if (this._press === null)
            return;
        const [, , state] = global.get_pointer();
        if ((state & BUTTON_MASK) !== 0 && !this._press.button) {
            this._press.button = true;
            this._emit({type: 'capsLock', phase: 'otherInput'});
        }
        if ((state & this._held.mask) === this._held.mask)
            this._press.seen = true;
        else if (this._press.seen)
            this._endPress(true);
    }

    /** Lets go of the other keys and stops sampling; `released` reports Caps Lock coming up. */
    _endPress(released) {
        if (this._press === null)
            return;
        this._press = null;
        if (this._sampler !== 0)
            GLib.source_remove(this._sampler);
        this._sampler = 0;
        for (const action of this._keyActions)
            ungrab(action);
        this._keyActions.clear();
        if (released)
            this._emit({type: 'capsLock', phase: 'released'});
    }

    destroy() {
        this.restore();
        global.display.disconnectObject(this);
        this._keyboard = null;
    }
}
