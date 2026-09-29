// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import org.auroraw.ui

// A task tab (D-127): no Fusion tab box any more, a transparent tab with a 2px accent underline on
// the current one — the header's Catalogue/Cull/Develop/Publish. See KeywordPanel.qml's own
// `PanelTab` for the side panel's tabs, which need their own width math but the same look.
TabButton {
    id: control
    opacity: enabled ? 1 : 0.4
    // Room between tabs (Patrick's own review: with no fill and no gap, inactive tabs read as one
    // run-on string); the underline below spans this same padded width, not just the label.
    leftPadding: 16
    rightPadding: 16

    background: Rectangle {
        color: "transparent"
        Rectangle {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: 2
            color: control.checked ? Theme.accent : "transparent"
            Behavior on color { ColorAnimation { duration: Theme.motion } }
        }
    }
    contentItem: Label {
        text: control.text
        font: control.font
        color: control.checked ? control.palette.windowText : Theme.quiet
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        Behavior on color { ColorAnimation { duration: Theme.motion } }
    }
}
