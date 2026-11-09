// What Snappy's KWin script does for the daemon. KWin scripts can call D-Bus methods but can't
// offer any, so the daemon owns the bus name: events go out with `Event`, and requests are
// collected with `Take` when the daemon rings the doorbell, a shortcut action KDE's shortcut
// service triggers for it. Answers go back with `Answer`, whose reply carries any requests
// queued meanwhile. Nothing runs while nobody asks.
//
// main.qml hands over everything from KWin and QML through `start`; this file keeps the state.

.import "windows.js" as Windows
.import "parking.js" as Parking

/** Changes whenever a request, answer, or event changes shape. The daemon checks it. */
var PROTOCOL_VERSION = 2;

var BUS_NAME = 'io.github.farhadzaidi.Snappy';
var OBJECT_PATH = '/io/github/farhadzaidi/Snappy/KWin';
var INTERFACE = 'io.github.farhadzaidi.Snappy.KWin';
var DOORBELL = 'snappy-requests';

/** The value of KWin's `MaximizeArea` client area: a display minus panels. */
var MAXIMIZE_AREA = 2;

/**
 * The shortest time between pointer reports during a drag, in milliseconds: about two per
 * frame, however fast the mouse reports. The release always carries the final position.
 */
var SAMPLE_INTERVAL_MS = 8;

var ctx = null;

/** Collecting: between a ring and an empty batch. A ring meanwhile means look again after. */
var collecting = false;
var rangWhileCollecting = false;

var press = false;
var drag = false;
/** The window KWin is moving or resizing for the pointer, while it does. */
var grabbed = null;
var lastPointer = null;
var lastSample = 0;
var sampling = false;

/** Window ID to its window and the handlers that report its moves. */
var followed = new Map();

/**
 * The displays as the daemon last read them, to tell when panels changed a work area. KWin
 * says nothing when a panel's reserved space changes.
 */
var reportedDisplays = null;
/** A panel changed during a drag; the displays are checked when the drag ends. */
var displaysPending = false;

/** `context`: workspace, emit(json), take(), answer(json). */
function start(context) {
    ctx = context;
    Parking.start(ctx.workspace);
    for (const window of Array.from(ctx.workspace.windows))
        watchLifetime(window);
    emit({type: 'ready', version: PROTOCOL_VERSION});
}

function emit(event) {
    ctx.emit(JSON.stringify(event));
}

// Collecting requests.

function rung() {
    if (collecting) {
        rangWhileCollecting = true;
        return;
    }
    collecting = true;
    rangWhileCollecting = false;
    ctx.take();
}

function collected(json) {
    let requests;
    try {
        requests = JSON.parse(json);
    } catch (error) {
        requests = [];
    }
    if (requests.length === 0) {
        collecting = false;
        if (rangWhileCollecting)
            rung();
        return;
    }
    const answers = requests.map(request => {
        try {
            return {seq: request.seq, ok: handle(request)};
        } catch (error) {
            return {seq: request.seq, error: `${error.message || error}`};
        }
    });
    ctx.answer(JSON.stringify(answers));
}

/** The daemon didn't answer: it quit or crashed. */
function collectFailed() {
    collecting = false;
    daemonGone();
}

/**
 * Takes down what the daemon set up, so a crashed daemon wakes nothing. Parked windows stay
 * parked; the daemon brings them back when it starts again.
 */
function daemonGone() {
    setMonitoring(false, false);
    unfollowAll();
}

function windowById(id) {
    const window = Windows.find(ctx.workspace, id);
    if (window === null)
        throw new Error('gone');
    return window;
}

function record(window) {
    return window === null ? null : Windows.describe(window, ctx.workspace, Parking.isParked(window));
}

function handle(request) {
    const workspace = ctx.workspace;
    switch (request.op) {
    case 'windows':
        return Windows.managed(workspace).map(record);
    case 'window':
        return record(Windows.find(workspace, request.id));
    case 'focused': {
        const active = workspace.activeWindow;
        return active && Windows.isManaged(active) ? record(active) : null;
    }
    case 'follow':
        follow(request.ids);
        return null;
    case 'setPosition':
        Windows.setPosition(windowById(request.id), request.x, request.y);
        return null;
    case 'setSize':
        Windows.setSize(windowById(request.id), request.width, request.height);
        return null;
    case 'unmaximize':
        Windows.restore(windowById(request.id));
        return null;
    case 'activate':
        workspace.activeWindow = windowById(request.id);
        return null;
    case 'clearFocus':
        workspace.activeWindow = null;
        return null;
    case 'pointer':
        return pointer();
    case 'monitors': {
        const list = monitors();
        reportedDisplays = JSON.stringify(list);
        return list;
    }
    case 'monitoring':
        setMonitoring(request.press, request.drag);
        return null;
    case 'bind':
        Parking.bind();
        return null;
    case 'onBoundDesktop':
        return Parking.onBoundDesktop();
    case 'clearDesktops':
        return Parking.clear();
    case 'park':
        return Parking.park(windowById(request.id));
    case 'unpark':
        Parking.unpark(windowById(request.id));
        return null;
    default:
        throw new Error(`unknown request ${request.op}`);
    }
}

function pointer() {
    const position = ctx.workspace.cursorPos;
    return {x: position.x, y: position.y};
}

function monitors() {
    const workspace = ctx.workspace;
    return Array.from(workspace.screenOrder).map(output => {
        const frame = output.geometry;
        const work = workspace.clientArea(MAXIMIZE_AREA, output, workspace.currentDesktop);
        return {
            name: output.name,
            manufacturer: output.manufacturer,
            model: output.model,
            serial: output.serialNumber,
            frame: {x: frame.x, y: frame.y, width: frame.width, height: frame.height},
            workArea: {x: work.x, y: work.y, width: work.width, height: work.height},
            scale: output.devicePixelRatio,
        };
    });
}

// Windows coming and going, focus, displays, and desktops.

function windowAdded(window) {
    watchLifetime(window);
    if (window.dock)
        panelChanged();
    if (Windows.isManaged(window))
        emit({type: 'windowAdded', id: Windows.id(window), pid: window.pid});
}

function windowRemoved(window) {
    if (window.dock)
        panelChanged();
    const id = Windows.id(window);
    unfollow(id);
    if (grabbed === window)
        release();
    if (Windows.isManaged(window))
        emit({type: 'windowRemoved', id});
}

function windowActivated(window) {
    if (window && Windows.isManaged(window))
        emit({type: 'focus', pid: window.pid});
}

function displaysChanged() {
    reportedDisplays = null;
    emit({type: 'displays'});
}

/**
 * A panel came, went, moved, or resized, which can change a work area. KWin works out the
 * new areas once the current event is handled, so they are compared after it; during a drag,
 * after the drag, since the daemon ends drags when displays change.
 */
function panelChanged() {
    if (grabbed !== null) {
        displaysPending = true;
        return;
    }
    Qt.callLater(checkDisplays);
}

/** Reports the displays changed when their frames, work areas, or scales did. */
function checkDisplays() {
    displaysPending = false;
    if (reportedDisplays === null)
        return;
    const now = JSON.stringify(monitors());
    if (now === reportedDisplays)
        return;
    reportedDisplays = now;
    emit({type: 'displays'});
}

function desktopChanged() {
    emit({type: 'desktop'});
}

// Drags, observed through KWin's own interactive moves and resizes. Clicks on Wayland
// windows go straight to their apps, so a press is reported when a move or resize starts.
// While one runs and the daemon asks, pointer moves are reported, a few per frame at most;
// the pointer isn't watched otherwise. Modifiers aren't reported: scripts can't read them,
// so the daemon follows them itself.

function watchLifetime(window) {
    window.interactiveMoveResizeStarted.connect(() => begin(window));
    window.interactiveMoveResizeFinished.connect(() => end(window));
    if (window.dock)
        window.frameGeometryChanged.connect(panelChanged);
}

function setMonitoring(nextPress, nextDrag) {
    press = nextPress;
    drag = nextDrag;
    updateSampling();
}

function begin(window) {
    if (!Windows.isManaged(window))
        return;
    // A panel may have changed without KWin saying so, as when it stops reserving space;
    // the daemon hears of it before the press, so the drag uses the new zones.
    checkDisplays();
    grabbed = window;
    lastPointer = pointer();
    if (press || drag)
        emit(Object.assign({type: 'pointerDown', window: Windows.id(window)}, lastPointer));
    updateSampling();
}

function end(window) {
    if (grabbed !== window)
        return;
    release();
    if (press || drag)
        emit(Object.assign({type: 'pointerUp'}, pointer()));
    if (displaysPending)
        Qt.callLater(checkDisplays);
}

function release() {
    grabbed = null;
    lastPointer = null;
    updateSampling();
}

function updateSampling() {
    const wanted = drag && grabbed !== null;
    if (wanted === sampling)
        return;
    sampling = wanted;
    if (wanted)
        ctx.workspace.cursorPosChanged.connect(sample);
    else
        ctx.workspace.cursorPosChanged.disconnect(sample);
}

function sample() {
    const time = Date.now();
    if (time - lastSample < SAMPLE_INTERVAL_MS)
        return;
    const now = pointer();
    if (lastPointer !== null && now.x === lastPointer.x && now.y === lastPointer.y)
        return;
    lastSample = time;
    lastPointer = now;
    emit(Object.assign({type: 'pointerDragged'}, now));
}

// Following the windows the daemon handed to the engine, so a drag of any other window
// doesn't wake it for every frame.

function follow(ids) {
    for (const id of ids) {
        if (followed.has(id))
            continue;
        const window = Windows.find(ctx.workspace, id);
        if (window === null)
            continue;
        const moved = () => emit({type: 'windowMoved', id});
        const restored = () => {
            if (!window.minimized)
                moved();
        };
        window.frameGeometryChanged.connect(moved);
        window.desktopsChanged.connect(moved);
        window.minimizedChanged.connect(restored);
        followed.set(id, {window, moved, restored});
    }
}

function unfollow(id) {
    const entry = followed.get(id);
    if (!entry)
        return;
    entry.window.frameGeometryChanged.disconnect(entry.moved);
    entry.window.desktopsChanged.disconnect(entry.moved);
    entry.window.minimizedChanged.disconnect(entry.restored);
    followed.delete(id);
}

function unfollowAll() {
    for (const id of Array.from(followed.keys()))
        unfollow(id);
}
