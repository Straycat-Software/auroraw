// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import org.auroraw.ui

// The flat button of a toolbar (D-136, a follow-up of D-129: Fusion's own ToolButton was the one control on the
// image view's toolbars that still had a bevel): no fill and no edge at rest, a soft tint of the text colour on
// hover (the interface's one motion exception, a ~150ms fade), and while it is on (`checked`) or pressed a solid
// `hover` or `sunken` face with a hairline edge. A glyph button (✕, ◀, ▶, the stars) takes its colour from
// `palette.buttonText`, which is how the stars and the flag are drawn in their meaning colours.
ToolButton {
    id: control
    // Dimmed once, here: the label reads the plain text colour, not the palette's disabled one, or a disabled
    // tool would be dimmed twice.
    opacity: enabled ? 1 : 0.35
    topPadding: 6
    bottomPadding: 6
    leftPadding: 8
    rightPadding: 8

    background: Rectangle {
        radius: Theme.radiusControl
        border.width: 1
        color: control.down ? Theme.surface.sunken
               : control.checked ? Theme.surface.hover
               : control.hovered ? Qt.rgba(Theme.surface.text.r, Theme.surface.text.g, Theme.surface.text.b, 0.08) : "transparent"
        border.color: control.checked || control.down ? Theme.surface.border : "transparent"
        Behavior on color { ColorAnimation { duration: Theme.motion } }
        Behavior on border.color { ColorAnimation { duration: Theme.motion } }
        // Keyboard focus only, as on AppButton: the flat background has no bevel to hang Fusion's frame on.
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
        color: control.enabled ? control.palette.buttonText : Theme.surface.text
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
}
