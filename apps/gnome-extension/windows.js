// Other apps' windows: plain facts about each one, moves and focus on request, and the
// lifecycle events the app follows. Wayland and Xwayland windows are both Meta.Windows and
// are treated the same. Window titles are never read.

import Meta from 'gi://Meta';
import Shell from 'gi://Shell';

import {isMaximized, unmaximize} from './compat.js';

/** A window's facts, for the app to decide whether and how to manage it. */
export function describe(window, parking) {
    const frame = window.get_frame_rect();
    const app = Shell.WindowTracker.get_default().get_window_app(window);
    const active = global.workspace_manager.get_active_workspace();
    return {
        id: window.get_stable_sequence(),
        pid: window.get_pid(),
        // Apps without a desktop file get a placeholder app backed by the window itself.
        appId: app && !app.is_window_backed() ? app.get_id() : null,
        wmClass: window.get_wm_class(),
        frame: {x: frame.x, y: frame.y, width: frame.width, height: frame.height},
        normal: window.get_window_type() === Meta.WindowType.NORMAL,
        transient: window.get_transient_for() !== null,
        allowsMove: window.allows_move(),
        allowsResize: window.allows_resize(),
        fullScreen: window.is_fullscreen(),
        maximized: isMaximized(window),
        minimized: window.minimized,
        onActiveWorkspace: window.located_on_workspace(active),
        onAllWorkspaces: window.is_on_all_workspaces(),
        parked: parking.isParked(window),
    };
}

/** Managed windows, top of the stack first. */
export function managedWindows() {
    const windows = global.display.list_all_windows().filter(window => !window.is_override_redirect());
    return global.display.sort_windows_by_stacking(windows).reverse();
}

export function findWindow(id) {
    return managedWindows().find(window => window.get_stable_sequence() === id) ?? null;
}

export function setPosition(window, x, y) {
    unmaximize(window);
    window.move_frame(false, Math.round(x), Math.round(y));
}

/** Resizes from the current top-left corner. */
export function setSize(window, width, height) {
    unmaximize(window);
    const frame = window.get_frame_rect();
    window.move_resize_frame(false, frame.x, frame.y, Math.round(width), Math.round(height));
}

/** Restores a maximized or tiled window; Wayland apps confirm the new size later. */
export function restore(window) {
    unmaximize(window);
}

export function activate(window) {
    window.activate(global.get_current_time());
}

/** Moves keyboard focus off every window, so typing reaches no hidden one. */
export function clearFocus() {
    global.display.unset_input_focus(global.get_current_time());
}

/**
 * Reports windows appearing and going away, focus changes, and moves of the windows the app
 * asked to follow. Following only those keeps a drag of any other window from waking the app
 * for every frame.
 */
export class WindowEvents {
    constructor(emit) {
        this._emit = emit;
        this._followed = new Set();
        global.display.connectObject(
            'window-created', (_display, window) => this._created(window),
            'notify::focus-window', () => this._focusChanged(),
            this);
        for (const window of managedWindows())
            this._watchLifetime(window);
    }

    _created(window) {
        if (window.is_override_redirect())
            return;
        this._watchLifetime(window);
        this._emit({type: 'windowAdded', id: window.get_stable_sequence(), pid: window.get_pid()});
    }

    _watchLifetime(window) {
        window.connectObject('unmanaged', () => {
            const id = window.get_stable_sequence();
            window.disconnectObject(this);
            this._followed.delete(window);
            this._emit({type: 'windowRemoved', id});
        }, this);
    }

    _focusChanged() {
        const window = global.display.get_focus_window();
        if (window)
            this._emit({type: 'focus', pid: window.get_pid()});
    }

    /** Starts reporting moves, resizes, restores, and workspace changes of these windows. */
    follow(ids) {
        const wanted = new Set(ids);
        for (const window of managedWindows()) {
            if (!wanted.has(window.get_stable_sequence()) || this._followed.has(window))
                continue;
            this._followed.add(window);
            const moved = () => this._emit({type: 'windowMoved', id: window.get_stable_sequence()});
            window.connectObject(
                'position-changed', moved,
                'size-changed', moved,
                'workspace-changed', moved,
                'notify::minimized', () => {
                    if (!window.minimized)
                        moved();
                },
                this);
        }
    }

    destroy() {
        global.display.disconnectObject(this);
        for (const window of managedWindows())
            window.disconnectObject(this);
        this._followed.clear();
    }
}
