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
// by the scrollbar"), then D-130 (Patrick's own review again, screenshot in hand, of that first fix:
// content no longer sat *under* the thumb, but touched it with nothing between): an overlay
// `ScrollBar` always paints its thumb flush against its Flickable's own edge — reserving room on the
// Flickable itself (`anchors.rightMargin`/`Layout.rightMargin` bound to this control's `width`) only
// shrinks the Flickable as a whole, and a delegate's own `width: ListView.view.width` shrinks right
// along with it, so the two stay flush no matter how much room is reserved. The gap that is actually
// visible comes from the *other* side of this: every delegate in this interface is
// `ListView.view.width - Theme.scrollbarReserve` wide, and the thumb itself sits `Theme.scrollbarGap`
// pixels in from *this* control's own left edge (`x`, not `width`: a style's own ScrollBar always
// stretches `contentItem.width` to `control.availableWidth` regardless of a child's own size hint,
// `leftPadding` included — `SpinBox` reads padding for its own contentItem sizing, D-128's own lesson
// there, but `ScrollBar` does not), so the two ends of that gap are what a row's content and the
// thumb actually touch, both a fixed distance from this control's own edge.
//
// D-131 (Patrick's own review: a framed list's rows sometimes never showed at all until something
// reset the model): a delegate used to read `<thisScrollBar>.width` directly, off the live sibling
// control created in the very same breath as itself — sometimes before that control's own geometry
// had settled, and nothing ever retriggered the binding afterwards, so a delegate stuck with a bad
// first read of it stayed stuck, invisible, until its model reset recreated it from scratch. Every
// delegate reads `Theme.scrollbarReserve` now (a plain constant on a singleton that is fully live
// before any other component exists) instead, and this control's own sizing reads the same constants,
// so the two can never drift apart.
ScrollBar {
    id: control
    implicitWidth: Theme.scrollbarReserve
    policy: ScrollBar.AsNeeded
    contentItem: Rectangle {
        x: control.width - Theme.scrollbarThumb
        implicitWidth: Theme.scrollbarThumb
        implicitHeight: Theme.scrollbarThumb
        radius: width / 2
        color: control.pressed ? Theme.accent : (control.hovered ? Theme.quiet : Theme.surface.border)
        opacity: control.policy === ScrollBar.AlwaysOn || control.size < 1.0 ? 1 : 0
        Behavior on color { ColorAnimation { duration: Theme.motion } }
        Behavior on opacity { OpacityAnimator { duration: Theme.motion } }
    }
}
