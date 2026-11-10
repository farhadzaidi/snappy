// Other apps' windows: plain facts about each one, and moves on request. Wayland and Xwayland
// windows are both KWin windows and are treated the same. Window titles are never read.

.pragma library

/** Hexadecimal digits of KWin's UUID that make a window's ID: few enough to stay exact as a
 * JavaScript number, plenty to keep a session's windows apart. */
var ID_DIGITS = 12;

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

/** A window's facts, for the daemon to decide whether and how to manage it. */
function describe(window, workspace, parked) {
    const frame = window.frameGeometry;
    return {
        id: id(window),
        uuid: String(window.internalId),
        pid: window.pid,
        appId: window.desktopFileName || null,
        wmClass: window.resourceClass || null,
        frame: {x: frame.x, y: frame.y, width: frame.width, height: frame.height},
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

/** Resizes from the current top-left corner. */
function setSize(window, width, height) {
    checkedNumbers(width, height);
    restore(window);
    const frame = window.frameGeometry;
    window.frameGeometry = Qt.rect(frame.x, frame.y, Math.round(width), Math.round(height));
}

/** Restores a maximized or tiled window; Wayland apps confirm the new size later. */
function restore(window) {
    if (window.maximizeMode !== 0)
        window.setMaximize(false, false);
    if (window.tile !== null)
        window.tile = null;
}
