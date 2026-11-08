// Drags, observed through GNOME Shell's own window moves and resizes. Clicks on Wayland
// windows go straight to their apps, so a press is reported when a move or resize starts
// rather than for every click. While one runs and the app asks, the pointer and modifiers are
// sampled each frame; nothing is sampled otherwise, and no keystroke is ever reported.
//
// GNOME pulls a window moved with Shift held onto nearby screen and window edges, and with
// Ctrl makes other windows' edges resist. While Snappy's drag key is held, the window is put
// back under the pointer each time GNOME moves it, before the frame is drawn, so it follows
// the pointer and Snappy's zones are the only guide.

import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';

import {KEYBOARD_GRAB, UNCONSTRAINED_GRAB} from './compat.js';

/** About one sample per frame at 60 Hz. */
const SAMPLE_INTERVAL_MS = 16;

/** Mouse button bits, which aren't modifiers. */
const BUTTON_MASK =
    Clutter.ModifierType.BUTTON1_MASK | Clutter.ModifierType.BUTTON2_MASK |
    Clutter.ModifierType.BUTTON3_MASK | Clutter.ModifierType.BUTTON4_MASK |
    Clutter.ModifierType.BUTTON5_MASK;

function pointer() {
    const [x, y, mask] = global.get_pointer();
    return {x, y, mods: mask & ~BUTTON_MASK};
}

/** Where the event being handled happened, with the modifiers held at that moment. */
function currentEventPointer() {
    const event = Clutter.get_current_event();
    if (!event)
        return null;
    const type = event.type();
    if (type !== Clutter.EventType.BUTTON_RELEASE && type !== Clutter.EventType.TOUCH_END)
        return null;
    const [x, y] = event.get_coords();
    return {x, y, mods: event.get_state() & ~BUTTON_MASK};
}

export class Drags {
    constructor(emit) {
        this._emit = emit;
        this._press = false;
        this._drag = false;
        /** The drag key's modifier mask; 0 leaves GNOME's moves alone. */
        this._follow = 0;
        /** The dragged window's offset from the pointer, and its size when measured. */
        this._anchor = null;
        this._following = {};
        this._correcting = false;
        this._grabbed = null;
        this._sampler = 0;
        this._last = null;
        global.display.connectObject(
            'grab-op-begin', (_display, window, op) => this._begin(window, op),
            'grab-op-end', (_display, window, op) => this._end(window, op),
            this);
    }

    /**
     * `press`: report moves and resizes starting and ending. `drag`: sample while one runs.
     * `follow`: keep a moved window under the pointer while these modifiers are held.
     */
    setMonitoring(press, drag, follow) {
        this._press = press;
        this._drag = drag;
        this._follow = follow;
        this._updateSampler();
    }

    _isMove(op) {
        return (op & ~UNCONSTRAINED_GRAB) === Meta.GrabOp.MOVING;
    }

    _startFollowing(window) {
        this._anchor = null;
        this._measure(window);
        window.connectObject('position-changed', () => this._keepUnderPointer(window), this._following);
    }

    _stopFollowing(window) {
        window?.disconnectObject(this._following);
        this._anchor = null;
    }

    /** Where the window sits relative to the pointer, as GNOME placed it. */
    _measure(window) {
        const [x, y] = global.get_pointer();
        const frame = window.get_frame_rect();
        this._anchor = {dx: frame.x - x, dy: frame.y - y, width: frame.width, height: frame.height};
    }

    _keepUnderPointer(window) {
        if (this._correcting || !this._press || this._follow === 0)
            return;
        const [x, y, mask] = global.get_pointer();
        if ((mask & this._follow) !== this._follow)
            return;
        const frame = window.get_frame_rect();
        // A maximized or tiled window comes loose at a new size; follow from where GNOME put it.
        if (frame.width !== this._anchor.width || frame.height !== this._anchor.height) {
            this._measure(window);
            return;
        }
        const wantedX = Math.round(x + this._anchor.dx);
        const wantedY = Math.round(y + this._anchor.dy);
        if (frame.x === wantedX && frame.y === wantedY)
            return;
        this._correcting = true;
        try {
            window.move_frame(true, wantedX, wantedY);
        } finally {
            this._correcting = false;
        }
    }

    _isPointerGrab(window, op) {
        return window !== null && !window.is_override_redirect() &&
            op !== Meta.GrabOp.NONE && (op & KEYBOARD_GRAB) === 0;
    }

    _begin(window, op) {
        if (!this._isPointerGrab(window, op))
            return;
        this._grabbed = window;
        this._last = pointer();
        if (this._isMove(op))
            this._startFollowing(window);
        if (this._press || this._drag) {
            this._emit({
                type: 'pointerDown', ...this._last,
                window: window.get_stable_sequence(),
            });
        }
        this._updateSampler();
    }

    _end(window, op) {
        if (this._grabbed === null || window !== this._grabbed || !this._isPointerGrab(window, op))
            return;
        this._grabbed = null;
        this._stopFollowing(window);
        this._updateSampler();
        // The release itself says which modifiers were down when the button came up; asking
        // the seat now could already see a modifier let go right after it.
        const released = currentEventPointer() ?? pointer();
        this._last = null;
        if (this._press || this._drag)
            this._emit({type: 'pointerUp', ...released});
    }

    _updateSampler() {
        const wanted = this._drag && this._grabbed !== null;
        if (wanted && this._sampler === 0) {
            this._sampler = GLib.timeout_add(GLib.PRIORITY_HIGH, SAMPLE_INTERVAL_MS, () => {
                this._sample();
                return GLib.SOURCE_CONTINUE;
            });
        } else if (!wanted && this._sampler !== 0) {
            GLib.source_remove(this._sampler);
            this._sampler = 0;
        }
    }

    _sample() {
        const now = pointer();
        const last = this._last;
        this._last = now;
        if (last && now.mods !== last.mods)
            this._emit({type: 'modifiers', ...now});
        else if (!last || now.x !== last.x || now.y !== last.y)
            this._emit({type: 'pointerDragged', ...now});
    }

    destroy() {
        global.display.disconnectObject(this);
        this._stopFollowing(this._grabbed);
        this._grabbed = null;
        this._drag = false;
        this._updateSampler();
    }
}
