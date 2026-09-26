// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls

// A menu as wide as its widest row: Qt sizes a menu when its rows are made, before the application's
// typeface, a change of language or the shortcuts written in the rows have reached them, and a row is then
// cut short. The width follows the rows (every `implicitWidth` read here is a dependency of the binding).
Menu {
    width: {
        let widest = 0
        for (let i = 0; i < count; i++) {
            const row = itemAt(i)
            if (row)
                widest = Math.max(widest, row.implicitWidth)
        }
        return widest + leftPadding + rightPadding
    }
}
