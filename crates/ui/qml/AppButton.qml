// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import org.auroraw.ui

// A button that looks disabled when it is: the application's buttons are all this one. D-129 (the
// visual refresh) gave it a flat, custom-drawn background — a solid fill and a hairline border, no
// Fusion bevel gradient — plus the interface's one motion exception, a ~150ms colour fade on hover
// and press. `highlighted` (the default action) and `checked` (a toggle) both fill solid `accentFill` (D-129: white on `accent` was 3.0:1).
Button {
    id: control
    opacity: enabled ? 1 : 0.4
    readonly property bool filled: highlighted || checked
    // Fusion's own padding read as cramped (Patrick's own review of D-129): more room on every side.
    topPadding: 8
    bottomPadding: 8
    leftPadding: 16
    rightPadding: 16

    background: Rectangle {
        radius: Theme.radiusControl
        border.width: 1
        color: control.filled
               ? (control.pressed ? Qt.darker(Theme.accentFill, 1.25) : (control.hovered ? Qt.darker(Theme.accentFill, 1.1) : Theme.accentFill))
               : (control.pressed ? Theme.surface.sunken : Theme.surface.hover)
        border.color: control.filled ? color : (control.hovered ? Theme.quiet : Theme.surface.border)
        Behavior on color { ColorAnimation { duration: Theme.motion } }
        Behavior on border.color { ColorAnimation { duration: Theme.motion } }
        // Keyboard focus only (`visualFocus`): the flat background has no bevel for Fusion's own
        // focus frame to hang on, so it is drawn here, just outside the border.
        Rectangle {
            anchors.fill: parent
            anchors.margins: -3
            radius: parent.radius + 3
            color: "transparent"
            border.width: 2
            border.color: Theme.accent
            visible: control.visualFocus
        }
    }
    contentItem: Label {
        text: control.text
        font: control.font
        color: control.filled ? Theme.white : Theme.surface.text
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
}
