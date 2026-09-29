// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import org.auroraw.ui

// A number field with an up/down button column (Settings' series gap and similar-photos options):
// D-127 gave it the same flat, custom-drawn background as AppButton — no Fusion gradient. Square
// corners rather than `radius-control`: Qt 6.4 (the floor this project builds on) has no per-corner
// radius, and rounding the whole composite would round the up/down buttons' inner corners too.
SpinBox {
    id: control
    readonly property int arrowWidth: 20

    background: Rectangle {
        color: Theme.surface.sunken
        border.width: 1
        border.color: control.activeFocus ? Theme.accent : Theme.surface.border
        Behavior on border.color { ColorAnimation { duration: Theme.motion } }
    }
    contentItem: TextInput {
        text: control.textFromValue(control.value, control.locale)
        font: control.font
        color: Theme.surface.text
        horizontalAlignment: Qt.AlignRight
        verticalAlignment: Qt.AlignVCenter
        leftPadding: 8
        rightPadding: control.arrowWidth + 8
        selectByMouse: true
        readOnly: !control.editable
        validator: control.validator
        inputMethodHints: Qt.ImhFormattedNumbersOnly
    }
    up.indicator: Rectangle {
        x: control.width - control.arrowWidth
        width: control.arrowWidth
        height: control.height / 2
        border.width: 1
        border.color: Theme.surface.border
        color: control.up.pressed ? Qt.lighter(Theme.surface.hover, 1.15) : (control.up.hovered ? Qt.lighter(Theme.surface.hover, 1.08) : Theme.surface.hover)
        Behavior on color { ColorAnimation { duration: Theme.motion } }
        Text {
            anchors.centerIn: parent
            text: "▲"
            font.pixelSize: 7
            color: Theme.surface.text
        }
    }
    down.indicator: Rectangle {
        x: control.width - control.arrowWidth
        y: control.height / 2
        width: control.arrowWidth
        height: control.height / 2
        border.width: 1
        border.color: Theme.surface.border
        color: control.down.pressed ? Qt.lighter(Theme.surface.hover, 1.15) : (control.down.hovered ? Qt.lighter(Theme.surface.hover, 1.08) : Theme.surface.hover)
        Behavior on color { ColorAnimation { duration: Theme.motion } }
        Text {
            anchors.centerIn: parent
            text: "▼"
            font.pixelSize: 7
            color: Theme.surface.text
        }
    }
}
