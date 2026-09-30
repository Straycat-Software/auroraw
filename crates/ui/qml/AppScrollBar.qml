// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import org.auroraw.ui

// D-130 (Patrick's own review, "make the scrollbar a bit more visible"): Fusion's own ScrollBar draws
// a thin, low-contrast handle that all but disappears on the `surface` tiers. This is a flat,
// `border`-coloured handle, a little wider, that brightens on hover and press (D-129's motion
// exception). Still an overlay (`policy: AsNeeded`, no track): drawn only while the content overflows,
// and out of the way otherwise.
//
// D-132 (Patrick's own review, screenshot in hand): an overlay ScrollBar paints its handle flush
// against its Flickable's edge, and reserving room on the Flickable (a `rightMargin`) shrinks the
// view and the handle's own room together, so content and handle stay in contact however much is
// reserved. The gap is made from both ends: a list's rows are `AppListView.rowWidth` wide (its width
// less this control's), and the handle sits `gap` pixels in from this control's own left edge.
//
// What places the handle is padding, not `x` (a review of the pull request measured it on Qt 6.4.2, and D-132's
// first explanation, an `x` binding, was wrong): a style's ScrollBar lays its handle out again on every
// change of position, at `leftPadding` and `availableWidth` wide, whatever `x` and `width` the handle asks
// for. So the padding is set here, `gap` on the left and `margin` on the right, and the handle is
// `thumbWidth` wide by construction; this control is `thumbWidth + gap + margin` wide.
//
// In a `ScrollView` (the Import dialog's form, the paste dialog's list, the XMP export's form) the bar is placed by the
// style: Fusion's `ScrollView` gives the bar it creates `x`, `y` and `height` (the right edge, the view's own room), and a
// bar given to `ScrollBar.vertical` in its place has none, so it stayed at 0, 0 at its implicit size, a 6px dot at the top
// left of the form (a review of #22 saw it in a snapshot and measured it: x 0, y 0, 12 by 6). A `Flickable` (a `ListView`, a
// `GridView`) lays its attached bar out itself, so it is only when the parent is a `ScrollView`, which has an
// `availableHeight` where a Flickable has none, that this bar places itself, as Fusion's does.
//
// Vertical only: the padding and `implicitWidth` assume a vertical bar, on the right.
ScrollBar {
    id: control
    readonly property int thumbWidth: 6
    // Between the end of a row and the handle, and between the handle and the list's own edge.
    readonly property int gap: 4
    readonly property int margin: 2
    implicitWidth: thumbWidth + gap + margin
    padding: 0
    leftPadding: gap
    rightPadding: margin
    policy: ScrollBar.AsNeeded

    readonly property bool inScrollView: parent !== null && typeof parent.availableHeight === "number"
    Binding { target: control; property: "x"; when: control.inScrollView
              value: control.parent ? control.parent.width - control.width : 0 }
    Binding { target: control; property: "y"; when: control.inScrollView
              value: control.parent ? control.parent.topPadding : 0 }
    Binding { target: control; property: "height"; when: control.inScrollView
              value: control.parent ? control.parent.availableHeight : 0 }
    contentItem: Rectangle {
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
