// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import org.auroraw.ui

// D-130 (Patrick's own review, "make the scrollbar a bit more visible"): Fusion's own ScrollBar
// draws a thin, low-contrast handle that all but disappears on the new `surface` tiers. A flat
// `border`-coloured handle, a little wider, that brightens on hover and press (the interface's one
// motion exception) — still an overlay (`policy: AsNeeded`, no track), still out of the way until
// the pointer is near it or the list is actually scrolled.
//
// D-130, a second pass (Patrick's own review again, "list items are stuck to the edges and covered
// by the scrollbar"), then D-132 (Patrick's own review again, screenshot in hand, of that first fix:
// content no longer sat *under* the thumb, but touched it with nothing between): an overlay
// `ScrollBar` always paints its thumb flush against its Flickable's own edge — reserving room on the
// Flickable itself (`anchors.rightMargin`/`Layout.rightMargin` bound to this control's `width`, still
// needed so the thumb is not clipped) only shrinks the Flickable as a whole, and a delegate's own
// `width: ListView.view.width` shrinks right along with it, so the two stay flush no matter how much
// room is reserved. The gap that is actually visible comes from the *other* side of this: every
// delegate in this interface is `ListView.view.width - <thisScrollBar>.width` wide (a plain
// `Layout.rightMargin`/`Layout.preferredHeight`-style reservation cannot reach inside a delegate the
// way it can an ordinary Layout child, so each list's own delegate does it explicitly) — and the
// thumb itself sits `gap` pixels in from *this* control's own left edge (`x`, not `width`: a style's
// own ScrollBar always stretches `contentItem.width` to `control.availableWidth` regardless of a
// child's own size hint, `leftPadding` included — `SpinBox` reads padding for its own contentItem
// sizing, D-130's own lesson there, but `ScrollBar` does not), so the two ends of that `gap` are what
// a row's content and the thumb actually touch, both a fixed distance from this control's own edge.
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
        opacity: control.policy === ScrollBar.AlwaysOn || control.size < 1.0 ? 1 : 0
        Behavior on color { ColorAnimation { duration: Theme.motion } }
        Behavior on opacity { OpacityAnimator { duration: Theme.motion } }
    }
}
