// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick

// A soft drop shadow behind whatever this holds (D-127, Patrick's own second review): three stacked,
// low-opacity, rounded rectangles, each a little larger and fainter than the last, offset down a few
// px. Qt 6.4 (the floor this project builds on) has no shadow or blur effect available without a new
// module — `QtQuick.Effects`' `MultiEffect` needs 6.5+, `Qt5Compat.GraphicalEffects` a dependency
// this project does not otherwise carry — so this is the hand-rolled, shader-free approximation.
// Used behind a dialog and a popup menu, the two pieces of chrome that visibly float over what is
// behind them; not the floating toolbar or the histogram, which already read as floating on their
// own translucent `scrim-light`/`scrim-strong` fill.
Item {
    default property alias content: holder.data
    // The corner radius of what sits on top (0 for the menu, `Theme.radiusContainer` for a dialog):
    // each shadow layer is a few px rounder still, so the extra roundness doesn't show as a hard edge.
    property int shadowRadius: 0

    Rectangle { x: -6; y: 10; width: parent.width + 12; height: parent.height; radius: shadowRadius + 6; color: "#14000000" }
    Rectangle { x: -3; y: 6; width: parent.width + 6; height: parent.height; radius: shadowRadius + 3; color: "#1c000000" }
    Rectangle { x: 0; y: 3; width: parent.width; height: parent.height; radius: shadowRadius; color: "#26000000" }
    Item { id: holder; anchors.fill: parent }
}
