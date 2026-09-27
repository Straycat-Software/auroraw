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
            app.library.sizeSlider.value = 160
            app.library.sizeSlider.moved()
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

    // Opening or closing a series leaves the view where it is, with a cursor far away (it was scrolled back to it) or
    // none (it was scrolled to put the series at the bottom). Large thumbnails make the grid longer than the window.
    function test_opening_or_closing_a_series_does_not_scroll_the_grid() {
        app.library.sizeSlider.value = 256
        app.library.sizeSlider.moved()
        tryVerify(() => grid.contentHeight > grid.height + 200, 5000, "the grid can be scrolled")
        wait(300)
        app.library.select(0)
        wait(100)
        grid.contentY = 500
        wait(100)
        compare(grid.contentY, 500)
        app.library.toggleSeries(bracketRow)
        wait(300)
        compare(grid.contentY, 500, "opening with the cursor on the first photo")
        compare(grid.currentIndex, 0)
        app.library.toggleSeries(bracketRow)
        wait(300)
        compare(grid.contentY, 500, "closing again")
        app.library.select(-1)
        wait(100)
        grid.contentY = 300
        wait(100)
        app.library.toggleSeries(bracketRow)
        wait(300)
        compare(grid.contentY, 300, "opening with no cursor")
        app.library.toggleSeries(bracketRow)
        wait(300)
        compare(grid.contentY, 300)
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

    // The flags of a closed series changed as one show on its photos when it opens, with no refresh needed.
    function test_flags_cleared_on_a_closed_series_show_on_its_photos_once_it_opens() {
        app.library.filterFlags(1)
        wait(200)
        app.library.toggleSeries(burstRow)
        click(burstRow + 1)
        tryVerify(() => app.library.canResolve, 5000)
        keyClick(Qt.Key_R)
        tryVerify(() => flagsOf([33, 34, 35, 36, 37]).join() === "2,1,2,2,2", 5000, flagsOf([33, 34, 35, 36, 37]).join())
        app.library.toggleSeries(burstRow)
        click(burstRow)
        app.library.reopen()
        wait(200)
        click(burstRow)
        compare(app.photos.selectedCount, 5, "the closed series is all its photos")
        tryVerify(() => app.library.canFlag, 5000, "reopened: its flags can be changed")
        keyClick(Qt.Key_U)
        wait(400)
        app.library.toggleSeries(burstRow)
        tryVerify(() => flagsOf([33, 34, 35, 36, 37]).every(f => f === 0), 5000, flagsOf([33, 34, 35, 36, 37]).join())
        for (let n = 0; n < 5; n++)
            tryCell(burstRow + n, "flag", 0)
    }

    // What a series lets be done with it: a photo in none has no series commands but Group; a resolved series takes no
    // flags, keep marks, grouping, taking out or resolving, and only it can be reopened.
    function test_commands_are_off_for_a_resolved_series_and_reopening_needs_one() {
        const lib = app.library
        lib.filterFlags(1)
        wait(300)
        click(0)
        verify(!lib.inSeries && lib.canFlag && lib.canGroup)
        verify(!lib.canKeep && !lib.canResolve && !lib.canUngroup && !lib.canReopen, "a loner has no series command")
        keyClick(Qt.Key_K)
        verify(!app.photos.isMarked(app.photos.idAt(0)), "and no keep mark")
        lib.toggleSeries(burstRow)
        click(burstRow + 1)
        verify(lib.inSeries && lib.canKeep && lib.canResolve && lib.canUngroup && lib.canFlag && lib.canGroup)
        verify(!lib.canReopen, "an unresolved series cannot be reopened")
        keyClick(Qt.Key_R)
        tryCell(burstRow, "seriesResolved", true)
        click(burstRow + 1)
        tryVerify(() => lib.canReopen, 5000)
        verify(!lib.canFlag && !lib.canKeep && !lib.canResolve && !lib.canGroup && !lib.canUngroup)
        const flags = flagsOf([33, 34, 35, 36, 37]).join()
        keyClick(Qt.Key_X)
        keyClick(Qt.Key_P)
        keyClick(Qt.Key_U)
        keyClick(Qt.Key_K)
        keyClick(Qt.Key_G, Qt.ControlModifier)
        keyClick(Qt.Key_G, Qt.ControlModifier | Qt.ShiftModifier)
        wait(300)
        compare(flagsOf([33, 34, 35, 36, 37]).join(), flags, "no flag changed")
        verify(!app.photos.isMarked(app.photos.idAt(burstRow + 1)), "no keep mark")
        compare(app.photos.seriesCount, 2, "still the same series")
        // The image view: the flag button is off, Keep is off and Compare is there.
        lib.openView(burstRow + 1)
        tryVerify(() => lib.viewing)
        verify(lib.viewer.keepButton.visible && !lib.viewer.keepButton.enabled && lib.viewer.compareButton.visible)
        verify(!lib.viewer.flagButton.enabled)
        keyClick(Qt.Key_Escape)
        // Reopened, everything is back and Reopen is off.
        click(burstRow + 1)
        lib.reopen()
        tryVerify(() => !lib.canReopen && lib.canFlag && lib.canKeep && lib.canResolve, 5000)
    }

    // The filters hide the rejected frames of a resolved series: the badge says how many are listed, and of how many, and
    // the view's Compare (nothing to compare) and Keep (resolved) look off.
    function test_a_series_with_hidden_frames_says_so_and_the_view_shows_what_is_off() {
        const lib = app.library
        lib.toggleSeries(burstRow)
        click(burstRow + 1)
        tryVerify(() => lib.canResolve, 5000)
        keyClick(Qt.Key_R)
        tryCompare(app.photos, "count", 34, 10000, "the four rejected frames are not listed")
        const one = cell(burstRow)
        tryCompare(one, "badgeLabel", "✓ ▣ 1/5", 5000)
        compare(one.seriesSize, 1)
        compare(one.seriesTotal, 5)
        lib.openView(burstRow)
        tryVerify(() => lib.viewing)
        verify(lib.viewer.compareButton.visible && !lib.viewer.compareButton.enabled && lib.viewer.compareButton.opacity < 1,
               "one frame listed: nothing to compare")
        verify(lib.viewer.keepButton.visible && !lib.viewer.keepButton.enabled && lib.viewer.keepButton.opacity < 1)
        keyClick(Qt.Key_Escape)
        // All photos: the five frames are listed again and the series compares.
        lib.filterFlags(1)
        wait(300)
        lib.openView(burstRow)
        tryVerify(() => lib.viewing)
        verify(lib.viewer.compareButton.enabled && lib.viewer.compareButton.opacity === 1)
        verify(!lib.viewer.keepButton.enabled)
        keyClick(Qt.Key_Escape)
        tryCell(burstRow, "badgeLabel", "✓ ▾ 5")
    }

    function test_the_view_offers_keep_and_compare_only_for_a_photo_in_a_series() {
        const lib = app.library
        click(0)
        lib.openView(0)
        tryVerify(() => lib.viewing)
        verify(!lib.viewer.keepButton.visible && !lib.viewer.compareButton.visible, "a loner")
        keyClick(Qt.Key_Escape)
        lib.toggleSeries(burstRow)
        lib.openView(burstRow + 1)
        tryVerify(() => lib.viewing)
        verify(lib.viewer.keepButton.visible && lib.viewer.keepButton.enabled && lib.viewer.compareButton.visible, "a frame")
        keyClick(Qt.Key_Escape)
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
        // The filmstrip has the frame in its middle (as near as the end of the list allows), though the list was just rebuilt.
        const strip = app.library.viewer.strip
        tryVerify(() => {
            const frame = strip.itemAtIndex(burstRow)
            if (!frame)
                return false
            const middle = frame.x + frame.width / 2 - strip.width / 2
            const wanted = Math.max(0, Math.min(middle, strip.contentWidth - strip.width))
            return Math.abs(strip.contentX - wanted) < 3
        }, 3000, "the strip is at " + strip.contentX + " of " + strip.contentWidth)
        keyClick(Qt.Key_Left)
        compare(app.library.viewer.row, burstRow - 1)
        keyClick(Qt.Key_Escape)
        tryCompare(app.photos, "count", 34, 5000, "the series is closed again, as it was")
    }

    // The series that the view opened is closed again when it goes away, and the grid has not moved.
    function test_going_back_from_the_image_view_leaves_the_series_closed_and_the_grid_where_it_was() {
        app.library.sizeSlider.value = 256
        app.library.sizeSlider.moved()
        tryVerify(() => grid.contentHeight > grid.height + 200, 5000)
        wait(300)
        grid.positionViewAtEnd()
        wait(200)
        const scrolled = grid.contentY
        app.library.openView(burstRow)
        tryVerify(() => app.library.viewing)
        compare(app.photos.count, 38)
        compare(grid.contentY, scrolled, "opening the view did not scroll the grid")
        keyClick(Qt.Key_Right)
        keyClick(Qt.Key_Escape)
        tryCompare(app.photos, "count", 34, 5000)
        compare(grid.contentY, scrolled, "nor did going back")
        compare(grid.currentIndex, burstRow, "the cursor is on the series")
        // A series that was open stays open.
        app.library.toggleSeries(burstRow)
        app.library.openView(burstRow)
        keyClick(Qt.Key_Escape)
        wait(200)
        compare(app.photos.count, 38)
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
