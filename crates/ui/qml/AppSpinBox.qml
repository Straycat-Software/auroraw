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

    // D-128 (Patrick's own review, "a single digit is stuck to the right"): the gap from the
    // digit to the arrow column used to be a `rightPadding` set on the `TextInput` itself, which
    // this control's own base style does not reliably reserve room for — the number rendered all
    // but flush against the arrows. `leftPadding`/`rightPadding` here, on the control, are the one
    // sizing contract every style positions `contentItem` from, arrows included: reliable regardless
    // of style. A floor on `implicitWidth` keeps a one-digit value from making the whole control
    // needlessly narrow, too (Settings' three boxes stay a consistent width as their values change).
    leftPadding: 8
    rightPadding: arrowWidth + 8
    implicitWidth: Math.max(76, contentItem.implicitWidth + leftPadding + rightPadding)

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
