// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import org.auroraw.ui

// D-128 (Patrick's own review, "add a frame to item lists"): a scrollable list of rows (the
// workspace list, a source, a keyword or a collection tree, the duplicates report, a metadata or
// info panel) used to sit directly on whatever was behind it, edge to edge. Not for the photo grid
// or a thumbnail's own grid (`Library.qml`, `SimilarPanel.qml`): those already have their own
// full-bleed treatment.
//
// D-128, a second pass (Patrick's own review again: metadata fields, a keyword's check box and a
// collection's name all read poorly inside it, "the scrollview's background should have a
// different colour, given the conflicts with other widgets"): the well was `surface-sunken` at
// first, the same tone the design system already used for "list bases" — but most of what actually
// sits in one of these lists (a text field, a check box, a spin box) is *also* drawn on
// `surface-sunken`, so the two collapsed into each other instead of one framing the other. `surface`
// (the plain canvas, one step lighter) is the frame's ground now: still visibly its own bounded area
// next to a dialog's `raised` background or a panel's own `surface` ground, but it no longer fights
// a sunken child for the same colour.
//
// The list inside still owns its own right margin for AppScrollBar (see its own doc comment): an
// overlay scrollbar anchors to its Flickable's own edge, not to whatever wraps that Flickable, so
// reserving room for it has to happen on the ListView/GridView itself, not here.
Rectangle {
    id: frame
    default property alias content: holder.data
    property color fill: Theme.surface.window

    color: frame.fill
    border.width: 1
    border.color: Theme.surface.border
    radius: Theme.radiusControl
    clip: true

    Item {
        id: holder
        anchors.fill: parent
        anchors.margins: 4
    }
}
