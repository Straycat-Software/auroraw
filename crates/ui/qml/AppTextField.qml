// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import org.auroraw.ui

// A single-line input (D-136): a `sunken` field with a hairline edge and `radius-control`, 28px high, its text
// starting 8px in. Focus fades the edge to `accent` and lays a soft 2px inner ring of it at 25 %, inside the
// field and not outside (a ring outside would be cut by the `clip` of every list and scroll view these sit in).
// Selected text and the hint read the palette's `highlight` and `placeholderText`, which Main.qml maps to
// `accentFill` and `placeholder`.
TextField {
    id: control
    implicitHeight: Math.max(28, contentHeight + topPadding + bottomPadding)
    leftPadding: 8
    rightPadding: 8
    topPadding: 4
    bottomPadding: 4
    verticalAlignment: TextInput.AlignVCenter
    selectByMouse: true
    opacity: enabled ? 1 : 0.5

    background: Rectangle {
        radius: Theme.radiusControl
        color: Theme.surface.sunken
        border.width: 1
        border.color: control.activeFocus ? Theme.accent : Theme.surface.border
        Behavior on border.color { ColorAnimation { duration: Theme.motion } }
        Rectangle {
            anchors.fill: parent
            anchors.margins: 1
            radius: parent.radius - 1
            color: "transparent"
            border.width: 2
            border.color: Qt.rgba(Theme.accent.r, Theme.accent.g, Theme.accent.b, control.activeFocus ? 0.25 : 0)
            Behavior on border.color { ColorAnimation { duration: Theme.motion } }
        }
    }
}
