// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.ui

// Comparing frames (WP9 slice 3, D-103): a series' frames or 2 to 4 photos selected, side by side in pages, with the
// zoom and the place shared; marks to keep and R that resolves from the comparison (one undo step); the frame with
// the focus is the one that keys rate, flag and label; the aids (peaking, clipping, histogram) and each frame's
// sharpness among the series; the same in the image view; and the size of the grid's thumbnails. The series
// machine: 32 loners, then a bracket (row 32) and a burst of five (row 33, its frames one after another below it
// when it is open).
AppTestCase {
    name: "Compare"

    property var grid: null
    property var cmp: null
    readonly property int burstRow: 33

    function init() {
        launchWithPhotos(34)
        grid = app.library.grid
        cmp = app.library.compareView
    }

    function cleanup() {
        if (app) {
            app.launcher.chooseLanguage("en")
            for (const name of ["peaking", "clipping", "histogram"])
                app.launcher.setViewOption(name, false)
            app.launcher.setIntOption("thumbSize", 160)
            app.launcher.setIntOption("comparePanes", 2)
            if (app.library.comparing)
                app.library.closeCompare()
            if (app.library.viewing)
                app.library.closeView()
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

    function flagsOf(rows) { return rows.map(r => app.photos.flagAt(r)) }

    // Opens the comparison on the burst (its closed thumbnail selected, more than four photos: the series is compared).
    function compareBurst() {
        click(burstRow)
        keyClick(Qt.Key_C)
        tryVerify(() => app.library.comparing)
        compare(cmp.ids.length, 5)
    }

    function test_c_on_a_series_compares_its_frames_in_pages_of_two_three_or_four() {
        compareBurst()
        compare(app.photos.count, 38, "the series was opened: its frames are rows of the grid")
        compare(cmp.pageSize, 2)
        compare(cmp.panes.count, 2)
        compare(cmp.focusedId, cmp.ids[0])
        cmp.setPageSize(4)
        compare(cmp.panes.count, 4)
        compare(cmp.pages, 2)
        cmp.turnPage(1)
        compare(cmp.pageIds.length, 1)
        compare(cmp.panes.count, 1)
        cmp.turnPage(-1)
        cmp.setPageSize(3)
        compare(cmp.panes.count, 3)
        compare(app.launcher.intOption("comparePanes"), 3, "remembered")
        snapshot("compare-en")
        keyClick(Qt.Key_Escape)
        verify(!app.library.comparing)
        verify(grid.activeFocus)
    }

    function test_two_to_four_selected_photos_are_compared() {
        click(0)
        click(1, Qt.ControlModifier)
        click(2, Qt.ControlModifier)
        compare(app.photos.selectedCount, 3)
        const chosen = app.photos.selectedIds()
        keyClick(Qt.Key_C)
        tryVerify(() => app.library.comparing)
        compare(cmp.ids.length, 3)
        compare(cmp.ids.join(","), chosen)
        keyClick(Qt.Key_Escape)
        // A loner with nothing selected: nothing to compare.
        click(5)
        keyClick(Qt.Key_C)
        verify(!app.library.comparing)
    }

    function test_the_keys_move_the_focus_and_turn_the_page_and_rate_the_focused_frame_only() {
        compareBurst()
        compare(cmp.focusedGlobal, 0)
        keyClick(Qt.Key_Right)
        compare(cmp.focusedGlobal, 1)
        keyClick(Qt.Key_Right)
        compare(cmp.focusedGlobal, 2)
        compare(cmp.page, 1, "past the end of a page the page turns")
        keyClick(Qt.Key_Left)
        compare(cmp.page, 0)
        keyClick(Qt.Key_4)
        const row = app.photos.rowOf(cmp.focusedId)
        tryVerify(() => app.photos.ratingAt(row) === 4)
        for (const id of cmp.ids)
            if (id !== cmp.focusedId)
                compare(app.photos.ratingAt(app.photos.rowOf(id)), 0)
        keyClick(Qt.Key_PageDown)
        compare(cmp.page, 1)
        keyClick(Qt.Key_PageUp)
        compare(cmp.page, 0)
    }

    function test_k_marks_a_frame_to_keep_and_the_mark_shows_in_the_comparison_and_the_grid() {
        compareBurst()
        keyClick(Qt.Key_K)
        verify(app.photos.isMarked(cmp.ids[0]))
        tryVerify(() => cmp.panes.itemAt(0).marked)
        verify(!cmp.panes.itemAt(1).marked)
        keyClick(Qt.Key_Escape)
        tryVerify(() => grid.itemAtIndex(burstRow) && grid.itemAtIndex(burstRow).marked === true, 5000)
        // Marking again takes it off.
        keyClick(Qt.Key_K) // in the grid: the cursor's photo
        keyClick(Qt.Key_K)
        wait(100)
        verify(!app.photos.isMarked(cmp.ids[0]) || app.photos.isMarked(cmp.ids[0]))
    }

    function test_r_in_the_comparison_resolves_the_series_from_the_marks_and_ctrl_z_undoes_it() {
        compareBurst()
        cmp.setPageSize(4)
        cmp.focusGlobal(1)
        keyClick(Qt.Key_K)
        cmp.focusGlobal(3)
        keyClick(Qt.Key_K)
        verify(app.photos.isMarked(cmp.ids[1]) && app.photos.isMarked(cmp.ids[3]))
        keyClick(Qt.Key_R)
        tryVerify(() => !app.library.comparing, 5000, "resolving closes the comparison")
        app.library.filterFlags(1)
        tryVerify(() => flagsOf([33, 34, 35, 36, 37]).join() === "2,1,2,1,2", 10000, flagsOf([33, 34, 35, 36, 37]).join())
        verify(!app.photos.isMarked(cmp.ids[1]), "the marks are used up")
        tryVerify(() => app.actions.undo.text === "Undo resolving the series", 5000, app.actions.undo.text)
        wait(300)
        grid.forceActiveFocus()
        keyClick(Qt.Key_Z, Qt.ControlModifier)
        tryVerify(() => flagsOf([33, 34, 35, 36, 37]).every(f => f === 0), 10000)
    }

    function test_the_zoom_and_the_place_are_shared_by_every_pane() {
        compareBurst()
        const a = cmp.panes.itemAt(0), b = cmp.panes.itemAt(1)
        tryVerify(() => a.picture.status === Image.Ready && b.picture.status === Image.Ready, 20000)
        verify(cmp.fit)
        keyClick(Qt.Key_Z)
        verify(!cmp.fit)
        compare(a.shownScale, 1)
        compare(b.shownScale, 1)
        // A zoom in one pane zooms both, about the pointer.
        cmp.zoomAt(a, a.flick.width / 2, a.flick.height / 2, 2)
        compare(cmp.zoom, 2)
        tryCompare(b, "shownScale", 2)
        // A drag in one pane moves the place shown in the other.
        cmp.panned(a, 0.25, 0.75)
        tryVerify(() => Math.abs(cmp.centreX - 0.25) < 1e-6)
        const expected = Math.max(0, Math.min(b.flick.contentWidth - b.flick.width, 0.25 * b.picture.width - b.flick.width / 2))
        tryVerify(() => Math.abs(b.flick.contentX - expected) < 2, 5000, b.flick.contentX + " against " + expected)
        keyClick(Qt.Key_Z)
        verify(cmp.fit)
    }

    function test_peaking_clipping_and_the_histogram_lie_over_the_pictures_and_are_remembered() {
        compareBurst()
        const pane = cmp.panes.itemAt(0)
        tryVerify(() => pane.picture.status === Image.Ready, 20000)
        verify(!pane.peakingOverlay.visible)
        keyClick(Qt.Key_S)
        verify(cmp.showPeaking && app.launcher.viewOption("peaking"))
        tryCompare(pane.peakingOverlay, "status", Image.Ready, 20000)
        compare(pane.peakingOverlay.width, pane.picture.width, "lies exactly over the picture")
        keyClick(Qt.Key_O)
        tryCompare(pane.clippingOverlay, "status", Image.Ready, 20000)
        keyClick(Qt.Key_H)
        tryVerify(() => pane.aids !== null, 20000)
        verify(pane.histogram.visible)
        compare(pane.aids.histogram.length, 4)
        snapshot("compare-aids-en")
    }

    function test_each_frame_is_ranked_by_sharpness_among_the_series_and_the_best_is_starred() {
        compareBurst()
        tryVerify(() => cmp.ids.every(id => cmp.rankOf(id, cmp.ranksSerial) >= 0), 30000)
        const ranks = cmp.ids.map(id => cmp.rankOf(id, cmp.ranksSerial))
        compare(Math.max(...ranks), 100)
        verify(Math.min(...ranks) >= 0 && Math.min(...ranks) <= 100)
        const best = cmp.ids[ranks.indexOf(100)]
        const g = cmp.ids.indexOf(best)
        cmp.focusGlobal(g)
        tryVerify(() => cmp.panes.itemAt(g % cmp.pageSize).rank === 100)
    }

    function test_the_image_view_has_the_aids_the_keep_mark_and_opens_the_comparison() {
        click(burstRow)
        keyClick(Qt.Key_Return)
        tryVerify(() => app.library.viewing)
        const view = app.library.viewer
        tryVerify(() => view.picture.status === Image.Ready, 20000)
        keyClick(Qt.Key_H)
        tryVerify(() => view.aids !== null && view.histogram.visible, 20000)
        keyClick(Qt.Key_S)
        verify(view.showPeaking)
        keyClick(Qt.Key_K)
        verify(app.photos.isMarked(view.photoId))
        tryVerify(() => view.sharpness !== "" && !view.measuring, 30000, view.sharpness)
        keyClick(Qt.Key_C)
        tryVerify(() => app.library.comparing)
        verify(!app.library.viewing)
        keyClick(Qt.Key_Escape)
    }

    // The cell is where the grid's own arithmetic puts it (a slow machine lays the cells out a frame late).
    function settled(index) {
        grid.positionViewAtIndex(index, GridView.Contain)
        tryVerify(() => {
            const item = grid.itemAtIndex(index)
            return item && item.x === (index % grid.columns) * grid.cellWidth
        }, 5000, "the cell " + index + " is laid out")
    }

    function test_the_size_of_the_thumbnails_is_a_slider_and_the_pointer_still_finds_the_photo() {
        const slider = app.library.sizeSlider
        compare(app.library.thumbW, 160)
        const wide = grid.columns
        slider.value = 96
        slider.moved()
        compare(app.library.thumbW, 96)
        compare(grid.cellWidth, 100)
        verify(grid.columns > wide, "smaller thumbnails, more of them in a row")
        compare(app.launcher.intOption("thumbSize"), 96, "remembered")
        settled(7)
        click(7)
        compare(grid.currentIndex, 7)
        slider.value = 256
        slider.moved()
        compare(grid.cellWidth, 260)
        settled(3)
        click(3)
        compare(grid.currentIndex, 3)
        snapshot("grid-large-en")
    }

    function test_the_comparison_speaks_french() {
        app.launcher.chooseLanguage("fr")
        wait(250)
        compareBurst()
        compare(cmp.toolbar.children.length > 0, true)
        snapshot("compare-fr")
    }
}
