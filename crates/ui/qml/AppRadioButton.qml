// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import org.auroraw.ui

// A radio button (D-136), the round counterpart of AppCheckBox: a 15px circle on `sunken` with a hairline edge,
// and, chosen, a dot of the text colour in it.
RadioButton {
    id: control
    spacing: 6
    padding: 0
    opacity: enabled ? 1 : 0.5
    implicitWidth: implicitContentWidth + leftPadding + rightPadding
    implicitHeight: Math.max(implicitContentHeight, indicator ? indicator.implicitHeight : 0) + topPadding + bottomPadding

    indicator: Rectangle {
        implicitWidth: 15
        implicitHeight: 15
        x: control.leftPadding
        y: control.topPadding + (control.availableHeight - height) / 2
        radius: width / 2
        color: Theme.surface.sunken
        border.width: 1
        border.color: control.hovered ? Theme.quiet : Theme.surface.border
        Behavior on border.color { ColorAnimation { duration: Theme.motion } }
        Rectangle {
            anchors.centerIn: parent
            width: 7
            height: 7
            radius: 3.5
            color: Theme.surface.text
            visible: control.checked
        }
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
    contentItem: Label {
        leftPadding: control.text !== "" ? control.indicator.width + control.spacing : control.indicator.width
        text: control.text
        font: control.font
        color: control.palette.windowText
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
}
