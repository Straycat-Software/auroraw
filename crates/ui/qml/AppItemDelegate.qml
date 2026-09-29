// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import org.auroraw.ui

// A row of a combo box's list (D-136), drawn like a menu row (AppMenuItem): 32px, 8px in on the sides, and a soft
// rounded tint of the text colour while it is under the pointer or the keyboard's choice (`highlighted`).
ItemDelegate {
    id: control
    implicitHeight: 32
    leftPadding: 8
    rightPadding: 8
    topPadding: 0
    bottomPadding: 0

    background: Rectangle {
        radius: Theme.radiusControl
        color: control.highlighted || control.hovered
               ? Qt.rgba(Theme.surface.text.r, Theme.surface.text.g, Theme.surface.text.b, 0.08) : "transparent"
        Behavior on color { ColorAnimation { duration: Theme.motion } }
    }
    contentItem: Label {
        text: control.text
        font: control.font
        color: control.enabled ? Theme.surface.text : Theme.surface.placeholder
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
}
