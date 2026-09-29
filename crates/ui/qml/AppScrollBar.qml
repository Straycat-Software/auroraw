// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import org.auroraw.ui

// D-130 (Patrick's own review, "make the scrollbar a bit more visible"): Fusion's own ScrollBar draws
// a thin, low-contrast handle that all but disappears on the `surface` tiers. This is a flat,
// `border`-coloured handle, a little wider, that brightens on hover and press (D-129's motion
// exception). Still an overlay (`policy: AsNeeded`, no track): out of the way until the list is
// actually scrolled.
//
// D-132 (Patrick's own review, screenshot in hand): an overlay ScrollBar paints its handle flush
// against its Flickable's edge, and reserving room on the Flickable (a `rightMargin`) shrinks the
// view and the handle's own room together, so content and handle stay in contact however much is
// reserved. The gap is instead made from both ends: a list's rows are `AppListView.rowWidth` wide
// (its width less this control's), and the handle sits `gap` pixels in from this control's own left
// edge. Note it is `x` that places the handle: the style stretches `contentItem.width` to the
// control's whole width whatever a child asks for, and ignores `leftPadding` too.
//
// Vertical only: `x` and `implicitWidth` assume a vertical bar.
ScrollBar {
    id: control
    readonly property int thumbWidth: 6
    readonly property int gap: 4
    implicitWidth: thumbWidth + gap
    policy: ScrollBar.AsNeeded
    contentItem: Rectangle {
        x: control.width - control.thumbWidth
        implicitWidth: control.thumbWidth
        implicitHeight: control.thumbWidth
        radius: width / 2
        color: control.pressed ? Theme.accent : (control.hovered ? Theme.quiet : Theme.surface.border)
        // `size` is 0 until the Flickable has measured its content: not yet known, so hidden, or the thumb
        // would show for a moment on every list that opens and fade out when the size turns out to be 1.
        opacity: control.policy === ScrollBar.AlwaysOn || (control.size > 0 && control.size < 1.0) ? 1 : 0
        Behavior on color { ColorAnimation { duration: Theme.motion } }
        Behavior on opacity { OpacityAnimator { duration: Theme.motion } }
    }
}
