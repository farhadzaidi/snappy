// Drags, observed through GNOME Shell's own window moves and resizes. Clicks on Wayland
// windows go straight to their apps, so a press is reported when a move or resize starts
// rather than for every click. While one runs and the app asks, the pointer and modifiers are
// sampled each frame; nothing is sampled otherwise, and no keystroke is ever reported.
//
// GNOME pulls a window moved with Shift held onto nearby screen and window edges, and with
// Ctrl makes other windows' edges resist. While the modifiers the app names are held, the
// window is put back under the pointer each time GNOME moves it, before the frame is drawn,
// so it follows the pointer and Snappy's zones are the only guide.

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

function isPointerGrab(window, op) {
    return window !== null && !window.is_override_redirect() &&
        op !== Meta.GrabOp.NONE && (op & KEYBOARD_GRAB) === 0;
}

function isMove(op) {
    return (op & ~UNCONSTRAINED_GRAB) === Meta.GrabOp.MOVING;
}

export class Drags {
    constructor(emit) {
        this._emit = emit;
        this._press = false;
        this._drag = false;
        /** The modifier mask that keeps a moved window under the pointer; 0 for none. */
        this._follow = 0;
        /** The window GNOME is moving or resizing for the pointer, while it does. */
        this._grabbed = null;
        /** Owner of the connections on the grabbed window, so they go away together. */
        this._grabConnections = {};
        /** The moved window's offset from the pointer, and its size when measured. */
        this._anchor = null;
        this._correcting = false;
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

    _begin(window, op) {
        if (!isPointerGrab(window, op))
            return;
        // GNOME runs one grab at a time; one that never reported its end is over.
        this._release();
        this._grabbed = window;
        this._last = pointer();
        window.connectObject('unmanaged', () => this._release(), this._grabConnections);
        if (isMove(op)) {
            this._measure(window);
            window.connectObject('position-changed', () => this._keepUnderPointer(window),
                this._grabConnections);
        }
        if (this._press || this._drag) {
            this._emit({
                type: 'pointerDown', ...this._last,
                window: window.get_stable_sequence(),
            });
        }
        this._updateSampler();
    }

    _end(window, op) {
        const grabbed = this._grabbed;
        this._release();
        if (grabbed === null || window !== grabbed || !isPointerGrab(window, op))
            return;
        // The release itself says which modifiers were down when the button came up; asking
        // the seat now could already see a modifier let go right after it.
        const released = currentEventPointer() ?? pointer();
        if (this._press || this._drag)
            this._emit({type: 'pointerUp', ...released});
    }

    /** Forgets the grab and everything that followed it. */
    _release() {
        this._grabbed?.disconnectObject(this._grabConnections);
        this._grabbed = null;
        this._anchor = null;
        this._last = null;
        this._updateSampler();
    }

    /** Where the window sits relative to the pointer, as GNOME placed it. */
    _measure(window) {
        const [x, y] = global.get_pointer();
        const frame = window.get_frame_rect();
        this._anchor = {dx: frame.x - x, dy: frame.y - y, width: frame.width, height: frame.height};
    }

    _keepUnderPointer(window) {
        if (this._correcting || this._follow === 0 || this._anchor === null)
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

    _updateSampler() {
        const wanted = this._drag && this._grabbed !== null;
        if (wanted && this._sampler === 0) {
            this._sampler = GLib.timeout_add(GLib.PRIORITY_DEFAULT, SAMPLE_INTERVAL_MS, () => {
                try {
                    this._sample();
                } catch (error) {
                    logError(error, 'Snappy: sampling the pointer');
                }
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
        this._drag = false;
        this._release();
    }
}
