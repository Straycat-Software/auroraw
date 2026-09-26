// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.ui

// Series (WP9, D-101): a burst of five and a bracket of three among 32 loners, shown as one thumbnail each with a
// count, opened in place, acted on as one unit while collapsed (X rejects all five, one step), resolved with R
// (the kept picked, the others rejected, one step that Ctrl+Z takes back), made by hand with Ctrl+G, filtered, and
// opened by the image view. The window is 1680 wide; the grid lists the newest first, so the loners come first,
// then the bracket (row 32, cover IMG_0005), then the burst (row 33, cover IMG_0000).
AppTestCase {
    name: "Series"

    property var grid: null
    readonly property int burstRow: 33
    readonly property int bracketRow: 32

    function init() {
        launchWithPhotos(34)
        grid = app.library.grid
        compare(app.photos.total, 40)
    }

    // Every series is taken apart and formed again by detection, every flag and rating cleared, the gap put back.
    function cleanup() {
        if (app) {
            app.launcher.chooseLanguage("en")
            app.launcher.setSeriesGap(2)
            app.library.filterSeries(0)
            app.library.filterFlags(1)
            wait(200)
            app.library.expandAll(false)
            app.library.selectAll()
            app.photos.rateSelection(0)
            app.photos.flagSelection("clear")
            app.photos.ungroupSelection()
            wait(400)
            app.launcher.regroupSeries()
            wait(600)
        }
        quit()
    }

    function cell(index) {
        grid.positionViewAtIndex(index, GridView.Contain)
        wait(30)
        const item = grid.itemAtIndex(index)
        verify(item, "the cell " + index + " exists")
        return item
    }

    function click(index, modifiers) {
        const item = cell(index)
        mouseClick(item, item.width / 2, item.height / 2 - 10, Qt.LeftButton,
                   modifiers === undefined ? Qt.NoModifier : modifiers)
        wait(40)
    }

    // Waits until a cell has `value` for `property` (asks for the cell again each time: the list is rebuilt when a series
    // changes, and a cell found earlier may be gone).
    function tryCell(index, property, value) {
        tryVerify(() => {
            const item = grid.itemAtIndex(index)
            return item !== null && item !== undefined && item[property] === value
        }, 10000, "cell " + index + "." + property + " never became " + value)
    }

    function flagsOf(rows) { return rows.map(r => app.photos.flagAt(r)) }

    function test_a_burst_and_a_bracket_are_one_thumbnail_each_with_their_count() {
        compare(app.photos.count, 34)
        compare(app.library.status, "40 photos")
        compare(app.photos.seriesCount, 2)
        const burst = cell(burstRow), bracket = cell(bracketRow)
        verify(burst.collapsed && bracket.collapsed)
        compare(burst.seriesSize, 5)
        compare(bracket.seriesSize, 3)
        compare(cell(0).seriesId, "", "a loner is in no series")
        snapshot("series-collapsed-en")
    }

    function test_a_series_opens_in_place_from_its_badge_or_the_key_and_closes_again() {
        // The badge, bottom right of the picture.
        const item = cell(burstRow)
        mouseClick(item, 140, 100)
        tryCompare(app.photos, "count", 38)
        compare(cell(burstRow).seriesOpen, true)
        compare([burstRow, burstRow + 1, burstRow + 2, burstRow + 3, burstRow + 4].map(r => cell(r).seriesEdge), [1, 2, 2, 2, 3])
        snapshot("series-open-en")
        // The key closes the series the cursor is in.
        click(burstRow + 2)
        keyClick(Qt.Key_E)
        tryCompare(app.photos, "count", 34)
        // Open all and close all.
        app.library.expandAll(true)
        compare(app.photos.count, 40)
        app.library.expandAll(false)
        compare(app.photos.count, 34)
    }

    function test_x_on_a_collapsed_series_rejects_all_its_photos_and_ctrl_z_gives_them_back() {
        click(burstRow)
        compare(app.photos.selectedCount, 5, "a collapsed series is selected with all its photos")
        keyClick(Qt.Key_X)
        tryCell(burstRow, "flag", 2)
        tryVerify(() => app.actions.undo.text === "Undo 5 flags", 5000, app.actions.undo.text)
        // Every member is rejected: see them with the rejected shown and the series open.
        app.library.filterFlags(1)
        app.library.toggleSeries(burstRow)
        tryVerify(() => flagsOf([33, 34, 35, 36, 37]).every(f => f === 2), 5000, flagsOf([33, 34, 35, 36, 37]).join())
        wait(300)
        grid.forceActiveFocus()
        keyClick(Qt.Key_Z, Qt.ControlModifier)
        tryVerify(() => flagsOf([33, 34, 35, 36, 37]).every(f => f === 0), 5000)
    }

    function test_r_resolves_a_series_keeping_the_selected_and_ctrl_z_gives_back_every_flag_and_rating() {
        // Stars for the five frames first (an open series lists them in the order they were taken: rows 33 to 37).
        app.library.toggleSeries(burstRow)
        for (let n = 0; n < 5; n++) {
            click(burstRow + n)
            keyClick(Qt.Key_1 + n)
        }
        wait(400)
        const ratingsBefore = [0, 1, 2, 3, 4].map(n => app.photos.ratingAt(burstRow + n))
        compare(ratingsBefore, [1, 2, 3, 4, 5])
        // Keep the second and the fourth frame.
        click(burstRow + 1)
        click(burstRow + 3, Qt.ControlModifier)
        compare(app.photos.selectedCount, 2)
        keyClick(Qt.Key_R)
        // The rejected ones leave the default list; the series is marked resolved.
        tryCompare(app.photos, "count", 35, 10000)
        tryCell(burstRow, "seriesResolved", true)
        app.library.filterFlags(1)
        tryVerify(() => app.photos.count === 38, 5000)
        tryVerify(() => flagsOf([33, 34, 35, 36, 37]).join() === "2,1,2,1,2", 5000, flagsOf([33, 34, 35, 36, 37]).join())
        tryVerify(() => app.actions.undo.text === "Undo resolving the series", 5000, app.actions.undo.text)
        // One step gives everything back: the flags, the ratings, the series.
        wait(300)
        grid.forceActiveFocus()
        keyClick(Qt.Key_Z, Qt.ControlModifier)
        tryVerify(() => flagsOf([33, 34, 35, 36, 37]).every(f => f === 0), 5000)
        compare([0, 1, 2, 3, 4].map(n => app.photos.ratingAt(burstRow + n)), ratingsBefore)
        tryCell(burstRow, "seriesResolved", false)
        keyClick(Qt.Key_Y, Qt.ControlModifier)
        tryVerify(() => flagsOf([33, 34, 35, 36, 37]).join() === "2,1,2,1,2", 5000)
    }

    function test_the_series_menu_of_the_filter_bar_lists_photos_by_series() {
        app.library.filterSeries(1)
        tryCompare(app.photos, "count", 2)
        app.library.filterSeries(2)
        tryCompare(app.photos, "count", 2)
        app.library.filterSeries(3)
        tryCompare(app.photos, "count", 0)
        app.library.filterSeries(0)
        tryCompare(app.photos, "count", 34)
        // Resolved: only the resolved one.
        app.library.toggleSeries(bracketRow)
        click(bracketRow)
        keyClick(Qt.Key_R)
        wait(400)
        app.library.filterSeries(3)
        tryVerify(() => app.photos.count >= 1, 5000)
        verify(app.photos.count <= 3)
    }

    function test_ctrl_g_makes_a_series_and_ctrl_shift_g_and_ctrl_z_take_it_apart() {
        click(0)
        click(1, Qt.ControlModifier)
        keyClick(Qt.Key_G, Qt.ControlModifier)
        tryCompare(app.photos, "count", 33, 10000)
        tryVerify(() => app.actions.undo.text === "Undo grouping 2 photos", 5000, app.actions.undo.text)
        wait(300)
        grid.forceActiveFocus()
        keyClick(Qt.Key_Z, Qt.ControlModifier)
        tryCompare(app.photos, "count", 34, 10000)
        keyClick(Qt.Key_Y, Qt.ControlModifier)
        tryCompare(app.photos, "count", 33, 10000)
        click(0)
        keyClick(Qt.Key_G, Qt.ControlModifier | Qt.ShiftModifier)
        tryCompare(app.photos, "count", 34, 10000)
    }

    function test_the_image_view_opens_a_collapsed_series_and_walks_its_members() {
        click(burstRow)
        keyClick(Qt.Key_Return)
        tryVerify(() => app.library.viewing)
        compare(app.photos.count, 38, "the series opened")
        compare(app.library.viewer.row, burstRow, "at its cover, its first frame")
        keyClick(Qt.Key_Left)
        compare(app.library.viewer.row, burstRow - 1)
        keyClick(Qt.Key_Escape)
    }

    function test_the_gap_is_a_setting_and_regrouping_uses_it() {
        const dialog = app.settingsDialog
        dialog.open()
        tryVerify(() => dialog.visible)
        compare(dialog.gapBox.value, 2)
        dialog.gapBox.value = 0
        dialog.gapBox.valueModified()
        compare(app.launcher.seriesGap(), 0)
        mouseClick(dialog.regroupButton)
        // Photos a second apart are no longer one series.
        tryCompare(app.photos, "count", 40, 10000)
        dialog.close()
        app.launcher.setSeriesGap(2)
        app.launcher.regroupSeries()
        tryCompare(app.photos, "count", 34, 10000)
    }

    function test_the_series_speak_french() {
        app.launcher.chooseLanguage("fr")
        wait(250)
        compare(app.library.seriesName(2), "Séries non résolues")
        click(burstRow)
        keyClick(Qt.Key_X)
        tryVerify(() => app.actions.undo.text === "Annuler 5 drapeaux", 5000, app.actions.undo.text)
        snapshot("series-fr")
    }
}
