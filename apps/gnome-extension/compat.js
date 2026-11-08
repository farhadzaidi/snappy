// Calls whose shape differs between the GNOME Shell versions the extension supports.

import Meta from 'gi://Meta';

import {PACKAGE_VERSION} from 'resource:///org/gnome/shell/misc/config.js';

export const SHELL_MAJOR = Number.parseInt(PACKAGE_VERSION.split('.')[0], 10);

/** Whether a window is maximized in either direction, or tiled to half the screen. */
export function isMaximized(window) {
    if (typeof window.get_maximized === 'function')
        return window.get_maximized() !== 0;
    if (typeof window.get_maximize_flags === 'function')
        return window.get_maximize_flags() !== 0;
    return window.is_maximized();
}

/** Restores a maximized or tiled window, so a new frame applies as given. */
export function unmaximize(window) {
    if (!isMaximized(window))
        return;
    if (typeof window.get_maximized === 'function')
        window.unmaximize(Meta.MaximizeFlags.BOTH);
    else
        window.unmaximize();
}

/** The flag grab operations driven from the keyboard carry, such as Alt+F7's move. */
export const KEYBOARD_GRAB = Meta.GrabOp.KEYBOARD_MOVING & ~Meta.GrabOp.MOVING;

/** The flag a move that ignores GNOME's placement constraints carries. */
export const UNCONSTRAINED_GRAB = (Meta.GrabOp.MOVING_UNCONSTRAINED ?? Meta.GrabOp.MOVING) & ~Meta.GrabOp.MOVING;
