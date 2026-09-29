// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import org.auroraw.ui

// A dialog's footer, its buttons right-aligned: every `AppDialog`'s footer is one of these.
//
// Fusion's own default `DialogButtonBox` paints an opaque, square, `window`-coloured strip flush
// with the bottom edge — it covered the dialog's rounded corners and border there, the same bug a
// transparent `header:` fixes at the top (Patrick's own review of D-127). Transparent here too, with
// more room around the buttons (his own review: Fusion's own padding read as cramped).
//
// D-128 (Patrick's own review, "the Add button is misplaced"): Fusion's own `DialogButtonBox`
// lays its buttons out by platform convention, which on Linux splits an `ActionRole` button away
// from a `RejectRole` one with a stretch between them (as this file's own doc comment used to
// promise "right-aligned" without actually delivering it). Every dialog here wants one fixed
// order instead — whatever it declares its buttons in, Cancel last — so `contentItem` is replaced
// outright: a plain `Row` over the box's own `contentModel`, anchored to the right, ignoring
// `DialogButtonBox.buttonRole` for position (it is still set on each button, and still read for
// accessibility) entirely. (A `RowLayout` here, the more obvious choice, recurses into a stack
// overflow: its own Layout machinery and `Container.contentModel` fight over the same children.)
DialogButtonBox {
    id: box
    background: Rectangle { color: "transparent" }
    padding: 20
    topPadding: 16
    spacing: 12
    contentItem: Item {
        implicitWidth: row.implicitWidth
        implicitHeight: row.implicitHeight
        Row {
            id: row
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: box.spacing
            Repeater { model: box.contentModel }
        }
    }
}
