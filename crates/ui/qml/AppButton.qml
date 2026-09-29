// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import org.auroraw.ui

// A button that looks disabled when it is: the application's buttons are all this one. D-127 (the
// visual refresh) gave it a flat, custom-drawn background — a solid fill and a hairline border, no
// Fusion bevel gradient — plus the interface's one motion exception, a ~150ms colour fade on hover
// and press. `highlighted` (the default action) and `checked` (a toggle) both fill solid `accent`.
Button {
    id: control
    opacity: enabled ? 1 : 0.4
    readonly property bool filled: highlighted || checked

    background: Rectangle {
        radius: Theme.radiusControl
        border.width: 1
        color: control.filled
               ? (control.pressed ? Qt.darker(Theme.accent, 1.15) : (control.hovered ? Qt.lighter(Theme.accent, 1.12) : Theme.accent))
               : (control.pressed ? Theme.surface.sunken : Theme.surface.hover)
        border.color: control.filled ? color : (control.hovered ? Theme.quiet : Theme.surface.border)
        Behavior on color { ColorAnimation { duration: Theme.motion } }
        Behavior on border.color { ColorAnimation { duration: Theme.motion } }
    }
    contentItem: Label {
        text: control.text
        font: control.font
        color: control.filled ? "#ffffff" : Theme.surface.text
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
}
