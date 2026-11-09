// Hiding windows for Snappy's workspaces: a virtual desktop of Snappy's own holds them, with
// their frames left alone. While parked they are left out of the task manager, the pager,
// and the window switcher, so they stay out of sight.

.pragma library

/** The parking desktop's name, which also finds it again after the script restarts. */
var PARKING_NAME = 'Snappy';
/** Why a window can't be parked, as the daemon's protocol names it. */
var REFUSED_ALL_DESKTOPS = 'allDesktops';

var workspace = null;
/** The desktop Snappy's workspaces belong to. */
var bound = null;
/** Window to whether it skipped the task manager, pager, and switcher before parking. */
var hidden = new Map();

function start(kwinWorkspace) {
    workspace = kwinWorkspace;
}

function exists(desktop) {
    return desktop !== null && desktops().includes(desktop);
}

function desktops() {
    return Array.from(workspace.desktops);
}

function parking() {
    return desktops().find(desktop => desktop.name === PARKING_NAME) || null;
}

function ensureParking() {
    if (parking() === null)
        workspace.createDesktop(desktops().length, PARKING_NAME);
    return parking();
}

/**
 * Where shown windows belong: the bound desktop, else the one in use, else the first. Never
 * the parking desktop, which the user may be looking at.
 */
function home() {
    const parked = parking();
    const candidates = [bound, workspace.currentDesktop].concat(desktops());
    return candidates.find(desktop => exists(desktop) && desktop !== parked) || null;
}

function isParked(window) {
    const parked = parking();
    const on = Array.from(window.desktops);
    return parked !== null && !window.onAllDesktops && on.length === 1 && on[0] === parked;
}

function bind() {
    bound = workspace.currentDesktop;
}

function onBoundDesktop() {
    return exists(bound) && workspace.currentDesktop === bound;
}

/** Hides a window on the parking desktop. Returns a refusal, or null. */
function park(window) {
    if (window.onAllDesktops)
        return REFUSED_ALL_DESKTOPS;
    const parked = ensureParking();
    if (parked === null)
        return REFUSED_ALL_DESKTOPS;
    if (!hidden.has(window)) {
        hidden.set(window, {
            skipTaskbar: window.skipTaskbar,
            skipPager: window.skipPager,
            skipSwitcher: window.skipSwitcher,
        });
    }
    window.desktops = [parked];
    window.skipTaskbar = true;
    window.skipPager = true;
    window.skipSwitcher = true;
    return isParked(window) ? null : REFUSED_ALL_DESKTOPS;
}

/** Brings a window back to where shown windows belong, as it was before. */
function unpark(window) {
    const target = home();
    if (target !== null && !window.onAllDesktops)
        window.desktops = [target];
    const before = hidden.get(window) || {skipTaskbar: false, skipPager: false, skipSwitcher: false};
    window.skipTaskbar = before.skipTaskbar;
    window.skipPager = before.skipPager;
    window.skipSwitcher = before.skipSwitcher;
    hidden.delete(window);
}

/**
 * Stops: unbinds, and removes the parking desktop once nothing is on it, after taking someone
 * looking at it to where shown windows belong. Returns whether it is gone.
 */
function clear() {
    const target = home();
    bound = null;
    const parked = parking();
    if (parked === null)
        return true;
    const occupied = Array.from(workspace.windows).some(window =>
        !window.onAllDesktops && Array.from(window.desktops).includes(parked));
    if (occupied)
        return false;
    if (workspace.currentDesktop === parked) {
        if (target === null)
            return false;
        workspace.currentDesktop = target;
    }
    workspace.removeDesktop(parked);
    return true;
}
