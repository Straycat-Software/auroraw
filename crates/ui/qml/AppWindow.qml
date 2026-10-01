// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Window

// A resizable dialog as a genuine secondary window (D-113), the resizable-dialog counterpart to `AppDialog.qml`:
// a Popup's own drag-resize fights the Popup/`contentItem` machinery (a plain child ends up in `contentData`,
// never part of what is actually drawn, once a dialog gives its own `contentItem`, as every one with a list or
// a report does), and even then is a hand-rolled `MouseArea` faking what a real window border already does. A
// `Window`, made modal, gets a native title bar, close button and resize border for free. Used where a dialog's
// content genuinely benefits from being resized; every other dialog stays an `AppDialog`.
Window {
    id: control
    property var hostWindow: null
    readonly property int minWidth: 420
    readonly property int minHeight: 280
    default property alias data: content.data

    flags: Qt.Dialog
    modality: Qt.ApplicationModal
    transientParent: hostWindow
    // A `Window` has its own palette context (`QQuickWindow.palette`, Qt 6.2+): without this every `AppButton`
    // and `Label` inside would fall back to Qt's default light Fusion colours instead of Auroraw's own —
    // `Main.qml`'s big `palette { ... }` block only reaches its own child items, not a separate top-level window.
    palette: control.hostWindow ? control.hostWindow.palette : control.palette
    color: control.palette.window
    width: 640
    height: 480
    minimumWidth: control.minWidth
    minimumHeight: control.minHeight

    // Centred over the host the first time it opens; free to move and resize after that, like any window.
    function open() {
        if (control.hostWindow) {
            control.x = Math.round(control.hostWindow.x + (control.hostWindow.width - control.width) / 2)
            control.y = Math.round(control.hostWindow.y + (control.hostWindow.height - control.height) / 2)
        }
        control.show()
    }

    // A `Window` has no font to hand down: what is drawn in it would be set in the system's own typeface and size, not in
    // the application's (`Main.qml` gives it to the main window only). A `Control` passes its font on to the labels and
    // buttons below it, so it carries the host window's.
    Control {
        id: fontCarrier
        anchors.fill: parent
        font: control.hostWindow ? control.hostWindow.font : Qt.application.font

        Item {
            id: content
            anchors.fill: parent
            anchors.margins: 16
        }
    }

    Shortcut {
        sequence: "Escape"
        onActivated: control.close()
    }
}
