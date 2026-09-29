// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import org.auroraw.ui

// A check box (D-136): a 15px box on `sunken` with a hairline edge and `radius-control`; checked, it holds a light
// check mark (the ✓ glyph, as the interface already uses ✔ and ✖), and partly checked (`tristate`, the keyword
// tree's "some of the selected photos carry it") a short dash. Not a coloured fill: what a checked box says is
// the mark, and a fill under it would need its own contrast. With no text it is only the box.
CheckBox {
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
        radius: Theme.radiusControl
        color: Theme.surface.sunken
        border.width: 1
        border.color: control.hovered ? Theme.quiet : Theme.surface.border
        Behavior on border.color { ColorAnimation { duration: Theme.motion } }
        Text {
            anchors.centerIn: parent
            visible: control.checkState === Qt.Checked
            text: "✓"
            font.pixelSize: 11
            color: Theme.surface.text
        }
        Rectangle {
            anchors.centerIn: parent
            visible: control.checkState === Qt.PartiallyChecked
            width: 7
            height: 2
            color: Theme.surface.text
        }
        // Keyboard focus only.
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
        leftPadding: control.text !== "" ? control.indicator.width + control.spacing : control.indicator.width
        text: control.text
        font: control.font
        color: control.palette.windowText
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
}
