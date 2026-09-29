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
DialogButtonBox {
    background: Rectangle { color: "transparent" }
    padding: 20
    topPadding: 16
    spacing: 12
}
