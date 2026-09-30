// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import org.auroraw.ui

// A horizontal slider (D-136): a 4px groove on `sunken` with a hairline edge, filled with `accentFill` up to a
// 14px round handle in the text colour. (Horizontal only: the one slider of the interface is the grid's
// thumbnail size.)
Slider {
    id: control
    implicitWidth: 90
    implicitHeight: 20
    opacity: enabled ? 1 : 0.4

    background: Rectangle {
        x: control.leftPadding
        y: control.topPadding + (control.availableHeight - height) / 2
        width: control.availableWidth
        height: 4
        radius: 2
        color: Theme.surface.sunken
        border.width: 1
        border.color: Theme.controlEdge
        Rectangle {
            width: control.visualPosition * parent.width
            height: parent.height
            radius: parent.radius
            color: Theme.accentFill
        }
    }
    handle: Rectangle {
        x: control.leftPadding + control.visualPosition * (control.availableWidth - width)
        y: control.topPadding + (control.availableHeight - height) / 2
        implicitWidth: 14
        implicitHeight: 14
        radius: 7
        color: Theme.surface.text
        border.width: 1
        border.color: control.pressed ? Theme.accent : Theme.surface.border
        Behavior on border.color { ColorAnimation { duration: Theme.motion } }
        Rectangle {
            anchors.fill: parent
            anchors.margins: -3
            radius: width / 2
            color: "transparent"
            border.width: 2
            border.color: Theme.accent
            visible: control.visualFocus
        }
    }
}
