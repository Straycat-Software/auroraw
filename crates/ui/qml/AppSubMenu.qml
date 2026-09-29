// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import org.auroraw.ui

// A menu as wide as its widest row: Qt sizes a menu when its rows are made, before the application's
// typeface, a change of language or the shortcuts written in the rows have reached them, and a row is then
// cut short. The width follows the rows (every `implicitWidth` read here is a dependency of the binding).
//
// D-127 (the visual refresh): the popup sits on the `raised` tier — lighter than the window behind
// it, not Fusion's own default (`base`, the same sunken surface as a text field) — with the larger
// radius tier, a hairline border and a soft shadow (`AppShadow.qml`, Patrick's own second review) to
// sell it floating above whatever is behind it. Every cascading submenu is one of these too.
Menu {
    id: control
    topPadding: 4
    bottomPadding: 4
    width: {
        let widest = 0
        for (let i = 0; i < count; i++) {
            const row = itemAt(i)
            if (row)
                widest = Math.max(widest, row.implicitWidth)
        }
        return widest + leftPadding + rightPadding
    }
    background: AppShadow {
        shadowRadius: Theme.radiusContainer
        Rectangle {
            anchors.fill: parent
            radius: Theme.radiusContainer
            color: Theme.surface.raised
            border.width: 1
            border.color: Theme.surface.border
        }
    }
}
