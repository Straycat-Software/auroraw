// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import org.auroraw.ui

// D-128 (Patrick's own review, "make the scrollbar a bit more visible"): Fusion's own ScrollBar
// draws a thin, low-contrast handle that all but disappears on the new `surface` tiers. A flat
// `border`-coloured handle, a little wider, that brightens on hover and press (the interface's one
// motion exception) — still an overlay (`policy: AsNeeded`, no track), still out of the way until
// the pointer is near it or the list is actually scrolled.
//
// D-128, a second pass (Patrick's own review again, "list items are stuck to the edges and covered
// by the scrollbar"): an overlay `ScrollBar` anchors itself to its Flickable's own right edge, on
// top of whatever the Flickable draws there — reserving it real room, not just visual room, needs
// the Flickable's own `anchors.rightMargin` bound to this control's `width`, at every one of this
// interface's lists (`Layout.rightMargin` on a plain Item would not reach a `Flickable`'s content).
ScrollBar {
    id: control
    implicitWidth: 10
    policy: ScrollBar.AsNeeded
    contentItem: Rectangle {
        implicitWidth: 6
        implicitHeight: 6
        radius: width / 2
        color: control.pressed ? Theme.accent : (control.hovered ? Theme.quiet : Theme.surface.border)
        opacity: control.policy === ScrollBar.AlwaysOn || control.size < 1.0 ? 1 : 0
        Behavior on color { ColorAnimation { duration: Theme.motion } }
        Behavior on opacity { OpacityAnimator { duration: Theme.motion } }
    }
}
