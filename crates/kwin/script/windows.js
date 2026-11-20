// Other apps' windows: plain facts about each one, and moves on request. Wayland and Xwayland
// windows are both KWin windows and are treated the same. Window titles are never read.

.pragma library

/** Hexadecimal digits of KWin's UUID that make a window's ID: few enough to stay exact as a
 * JavaScript number, plenty to keep a session's windows apart. */
var ID_DIGITS = 12;
/** The error for lowering a window that isn't the active one. */
var NOT_ACTIVE = 'notActive';

/** A window's ID, taken from KWin's UUID for it, so it outlives the script and the daemon. */
function id(window) {
    const digits = String(window.internalId).replace(/[^0-9a-fA-F]/g, '');
    return parseInt(digits.slice(0, ID_DIGITS), 16);
}

/** Windows KWin manages for apps, rather than its own or Plasma's surfaces. */
function isManaged(window) {
    return window.managed && !window.deleted && !window.popupWindow && !window.outline &&
        !window.desktopWindow && !window.dock && !window.onScreenDisplay && !window.notification &&
        !window.criticalNotification && !window.appletPopup && !window.inputMethod &&
        !window.splash && !window.tooltip;
}

/** Managed windows, top of the stack first. */
function managed(workspace) {
    return Array.from(workspace.stackingOrder).filter(isManaged).reverse();
}

function find(workspace, wanted) {
    return managed(workspace).find(window => id(window) === wanted) || null;
}

function onCurrentDesktop(window, workspace) {
    return window.onAllDesktops || Array.from(window.desktops).includes(workspace.currentDesktop);
}

/** A window's frame, as the daemon reads rectangles. */
function frameOf(window) {
    const frame = window.frameGeometry;
    return {x: frame.x, y: frame.y, width: frame.width, height: frame.height};
}

/** A window's facts, for the daemon to decide whether and how to manage it. */
function describe(window, workspace, parked) {
    return {
        id: id(window),
        uuid: String(window.internalId),
        pid: window.pid,
        appId: window.desktopFileName || null,
        wmClass: window.resourceClass || null,
        frame: frameOf(window),
        normal: window.normalWindow,
        transient: window.transient,
        allowsMove: window.moveable,
        allowsResize: window.resizeable,
        fullScreen: window.fullScreen,
        // A window in one of KWin's tiles is held at the tile's size, as a maximized one is.
        maximized: window.maximizeMode !== 0 || window.tile !== null,
        minimized: window.minimized,
        onCurrentDesktop: onCurrentDesktop(window, workspace),
        onAllDesktops: window.onAllDesktops,
        parked,
    };
}

function checkedNumbers(...values) {
    if (!values.every(Number.isFinite))
        throw new Error('not a number');
}

function setPosition(window, x, y) {
    checkedNumbers(x, y);
    restore(window);
    const frame = window.frameGeometry;
    window.frameGeometry = Qt.rect(Math.round(x), Math.round(y), frame.width, frame.height);
}

/**
 * Resizes from the current top-left corner, within the window's own limits. KWin keeps to
 * an app's minimum and maximum size only when the user resizes, so they are kept here, as
 * other window managers keep them; the daemon sees the size the app got.
 */
function setSize(window, width, height) {
    checkedNumbers(width, height);
    restore(window);
    const frame = window.frameGeometry;
    const [allowedWidth, allowedHeight] = withinLimits(window, width, height);
    window.frameGeometry = Qt.rect(frame.x, frame.y, Math.round(allowedWidth), Math.round(allowedHeight));
}

/** A frame size held within the window's limits, which KWin gives for its content alone. */
function withinLimits(window, width, height) {
    const frame = window.frameGeometry;
    const content = window.clientGeometry;
    const border = {width: frame.width - content.width, height: frame.height - content.height};
    const limit = (value, minimum, maximum, extra) => {
        const upper = maximum > 0 ? maximum + extra : Infinity;
        return Math.min(Math.max(value, minimum + extra), upper);
    };
    return [
        limit(width, window.minSize.width, window.maxSize.width, border.width),
        limit(height, window.minSize.height, window.maxSize.height, border.height),
    ];
}

/**
 * Puts the active window behind every other. KWin's scripts can lower only the active window,
 * and KWin then activates the window on top.
 */
function lower(workspace, window) {
    if (workspace.activeWindow !== window)
        throw new Error(NOT_ACTIVE);
    workspace.slotWindowLower();
}

/** Restores a maximized or tiled window; Wayland apps confirm the new size later. */
function restore(window) {
    if (window.maximizeMode !== 0)
        window.setMaximize(false, false);
    if (window.tile !== null)
        window.tile = null;
}
