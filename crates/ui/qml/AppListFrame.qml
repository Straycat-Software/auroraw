// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import org.auroraw.ui

// D-128 (Patrick's own review, "add a frame to item lists"): a scrollable list of rows (the
// workspace list, a source, a keyword or a collection tree, the duplicates report, a metadata or
// info panel) used to sit directly on whatever was behind it, edge to edge, which the design system
// already called `surface-sunken` — "list bases" — without the code ever having drawn one. This is
// that well: a `radius-control` sunken fill and a hairline border around whatever list is put inside
// it. Not for the photo grid or a thumbnail's own grid (`Library.qml`, `SimilarPanel.qml`): those
// already have their own full-bleed treatment.
Rectangle {
    id: frame
    default property alias content: holder.data
    // Catalogue's own source rows are already sunken cards on the plain canvas (their own
    // contrast is what defines them): the frame around that list stays at the canvas tone instead,
    // so it reads as a boundary rather than a second, redundant well underneath them.
    property color fill: Theme.surface.sunken

    color: frame.fill
    border.width: 1
    border.color: Theme.surface.border
    radius: Theme.radiusControl
    clip: true

    Item {
        id: holder
        anchors.fill: parent
        anchors.margins: 1
    }
}
