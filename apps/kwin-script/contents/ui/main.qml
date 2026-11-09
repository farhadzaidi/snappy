// Snappy's KWin script: the daemon's hands inside KWin. It moves windows and reports drags,
// window changes, and display changes when the daemon asks; it makes no decisions. It draws
// nothing: the daemon draws its overlay itself. The daemon loads it over D-Bus at every start and unloads it when it quits.
import QtQuick
import org.kde.kwin
import "service.js" as Service

Item {
    id: root

    DBusCall {
        id: eventCall

        service: Service.BUS_NAME
        path: Service.OBJECT_PATH
        dbusInterface: Service.INTERFACE
        method: "Event"
        onFailed: Service.daemonGone()
    }

    DBusCall {
        id: takeCall

        service: Service.BUS_NAME
        path: Service.OBJECT_PATH
        dbusInterface: Service.INTERFACE
        method: "Take"
        onFinished: reply => Service.collected(reply[0])
        onFailed: Service.collectFailed()
    }

    DBusCall {
        id: answerCall

        service: Service.BUS_NAME
        path: Service.OBJECT_PATH
        dbusInterface: Service.INTERFACE
        method: "Answer"
        onFinished: reply => Service.collected(reply[0])
        onFailed: Service.collectFailed()
    }

    // The daemon rings this through KDE's shortcut service when requests wait. It has no key.
    ShortcutHandler {
        name: Service.DOORBELL
        text: "Snappy: pass requests to KWin (used by Snappy; leave unassigned)"
        sequence: ""
        onActivated: Service.rung()
    }


    Connections {
        target: Workspace

        function onWindowAdded(window) {
            Service.windowAdded(window);
        }

        function onWindowRemoved(window) {
            Service.windowRemoved(window);
        }

        function onWindowActivated(window) {
            Service.windowActivated(window);
        }

        function onScreensChanged() {
            Service.displaysChanged();
        }

        function onVirtualScreenGeometryChanged() {
            Service.displaysChanged();
        }

        function onCurrentDesktopChanged() {
            Service.desktopChanged();
        }

        function onDesktopsChanged() {
            Service.desktopChanged();
        }
    }

    Component.onCompleted: {
        Service.start({
            workspace: Workspace,
            emit: json => {
                eventCall.arguments = [json];
                eventCall.call();
            },
            take: () => takeCall.call(),
            answer: json => {
                answerCall.arguments = [json];
                answerCall.call();
            },
        });
    }
}
