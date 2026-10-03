// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls

// A command of the View section that is the choice of a filter (D-153): "Show photos rated 3 stars or more", "Show the
// picked photos". `marked` says whether it is the choice in force, and the caller **reads it from the grid** (the
// filters live in `PhotoGrid`, nothing is kept here), so that the menu and the filter bar cannot disagree; the menu
// draws a check mark at the front of the row, and a screen reader is told the row is checked.
Action {
    property string commandId: ""
    property bool marked: false
}
