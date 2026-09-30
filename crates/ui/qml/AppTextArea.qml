// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import org.auroraw.ui

// A multi-line input (D-136): the same `sunken` field as AppTextField, its text starting at the top, 8px in on the
// sides and 6px down. It wraps at the word (`wrapMode` is the caller's to change).
TextArea {
    id: control
    leftPadding: 8
    rightPadding: 8
    topPadding: 6
    bottomPadding: 6
    wrapMode: TextEdit.Wrap
    selectByMouse: true
    opacity: enabled ? 1 : 0.5

    background: Rectangle {
        radius: Theme.radiusControl
        color: Theme.surface.sunken
        border.width: 1
        border.color: control.activeFocus ? Theme.accent : Theme.controlEdge
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
