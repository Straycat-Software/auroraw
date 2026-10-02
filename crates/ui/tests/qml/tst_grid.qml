// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.ui

// The library grid on the fixture workspace (a machine made by the harness, 80 photos): selecting,
// paging, rating, the filter bar, the layout and the sentences.
// The last workspace opens on the grid.
AppTestCase {
    name: "Grid"

    function init() {
        launch("")
        app.width = 1400 + panelWidth
        compare(app.launcher.screen, "workspace")
        compare(app.currentTask, "cull", "a workspace with photos opens on the grid")
        tryCompare(app.photos, "count", 80)
        grid = app.library.grid
    }

    // The grid of the window `init` made.
    property var grid: null

    function cell(index) {
        grid.positionViewAtIndex(index, GridView.Contain)
        wait(30)
        const item = grid.itemAtIndex(index)
        verify(item, "the cell " + index + " exists")
        return item
    }

    function clickCell(index) {
        mouseClick(cell(index))
        wait(60)
    }

    // The keyword panel takes 320 px of the window; the grid has the rest.
    readonly property int panelWidth: 320
    function shownWidth(columns) { return columns * 164 + panelWidth }

    // The tests share the machine, and a rating is kept in the catalogue: what a test rated is
    // cleared afterwards (and the language put back), so that the next one starts from the same photos.
    property var rated: []

    function rateSelected(stars) {
        rated.push(app.photos.idAt(grid.currentIndex))
        keyClick(Qt.Key_0 + stars)
    }

    function cleanup() {
        if (app) {
            app.launcher.chooseLanguage("en")
            app.library.filterBy(0)
            app.library.sizeSlider.value = 160
            app.library.sizeSlider.moved()
            for (const id of rated) {
                const row = app.photos.rowOf(id)
                if (row >= 0)
                    app.photos.setRating(row, 0)
            }
            wait(200)
        }
        rated = []
        quit()
    }

    function test_a_click_selects_a_photo_and_the_strip_describes_it() {
        wait(500)
        snapshot("grid-unselected")
        compare(grid.currentIndex, -1, "nothing is selected at first")
        compare(app.library.summary, "")
        clickCell(3)
        compare(grid.currentIndex, 3)
        verify(app.library.summary.indexOf("IMG_") === 0, "the strip names the file: " + app.library.summary)
        clickCell(5)
        compare(grid.currentIndex, 5, "only one photo is selected")
        rateSelected(4)
        wait(300)
        snapshot("grid-selected")
    }

    function test_arrows_step_and_stop_at_the_ends_of_the_list() {
        grid.forceActiveFocus()
        keyClick(Qt.Key_Right)
        compare(grid.currentIndex, 0, "with nothing selected a move selects the first photo")
        keyClick(Qt.Key_Left)
        compare(grid.currentIndex, 0, "not before the first photo")
        keyClick(Qt.Key_Right)
        compare(grid.currentIndex, 1)
        keyClick(Qt.Key_Down)
        compare(grid.currentIndex, 1 + grid.columns)
        keyClick(Qt.Key_Up)
        compare(grid.currentIndex, 1)
        keyClick(Qt.Key_End)
        keyClick(Qt.Key_Right)
        compare(grid.currentIndex, 79, "not beyond the last photo")
    }

    function test_a_step_down_onto_a_short_last_row_lands_on_its_last_photo() {
        app.width = shownWidth(7) + 20
        wait(300)
        compare(grid.columns, 7, "80 photos in rows of 7: the last row has 3")
        app.library.select(75)
        grid.forceActiveFocus()
        keyClick(Qt.Key_Down)
        compare(grid.currentIndex, 79, "the column has no photo below: the last one")
    }

    function test_paging_keys_home_and_end_move_the_selection_and_keep_it_in_view() {
        verify(grid.count >= 60, "the fixture's photos are listed")
        clickCell(1)
        const page = grid.visibleRows * grid.columns
        keyClick(Qt.Key_PageDown)
        compare(grid.currentIndex, 1 + page, "a page down keeps the column")
        keyClick(Qt.Key_End)
        compare(grid.currentIndex, grid.count - 1)
        verify(grid.contentY > 0, "the last row was scrolled into view")
        const last = grid.itemAtIndex(grid.count - 1)
        verify(last && last.y + last.height <= grid.contentY + grid.height + 1, "the last photo is in view")
        keyClick(Qt.Key_PageUp)
        compare(grid.currentIndex, grid.count - 1 - page)
        keyClick(Qt.Key_Home)
        compare(grid.currentIndex, 0)
        compare(grid.contentY, 0, "back at the top")
        verify(app.library.summary !== "", "the strip follows")
    }

    function test_a_rating_key_reaches_the_cell_the_strip_and_the_catalogue() {
        clickCell(2)
        rateSelected(4)
        compare(grid.itemAtIndex(2).rating, 4, "the cell shows its new rating at once")
        tryVerify(() => app.library.summary.indexOf("★★★★") >= 0, 5000)
        // The catalogue holds it: read the list again, and list only the photos rated 4 or more.
        app.library.reload()
        compare(grid.itemAtIndex(2).rating, 4)
        compare(grid.currentIndex, 2, "the selection stays on its photo when the list is read again")
        app.library.filterBy(4)
        tryCompare(app.photos, "count", 1)
        compare(grid.itemAtIndex(0).rating, 4)
        // 0 clears it.
        clickCell(0)
        rateSelected(0)
        compare(grid.itemAtIndex(0).rating, 0)
    }

    // The Edit menu's Undo and Redo, for what was done to the photos (D-096).
    function undo() { keyClick(Qt.Key_Z, Qt.ControlModifier) }
    function redo() { keyClick(Qt.Key_Y, Qt.ControlModifier) }

    function test_undo_and_redo_take_a_rating_back_and_forward() {
        verify(!app.actions.undo.enabled, "nothing to undo yet")
        compare(app.actions.undo.text, "Undo")
        clickCell(2)
        rateSelected(4)
        tryVerify(() => app.actions.undo.enabled, 5000, "the engine reported the step")
        compare(app.actions.undo.text, "Undo rating")
        verify(!app.actions.redo.enabled)

        undo()
        tryRead(() => delegateValue(grid, 2, "rating"), 0)
        tryVerify(() => app.actions.redo.enabled)
        compare(app.actions.redo.text, "Redo rating")
        verify(!app.actions.undo.enabled, "back where it began")
        redo()
        tryRead(() => delegateValue(grid, 2, "rating"), 4)
        tryVerify(() => app.library.summary.indexOf("★★★★") >= 0)
        app.menu.openSection(1)
        wait(250)
        snapshot("edit-menu-undo")
        app.menu.close()
        wait(150)
    }

    function test_a_rating_pressed_twice_is_one_step_and_two_ratings_are_two() {
        clickCell(0)
        rateSelected(3)
        rateSelected(3)
        tryVerify(() => app.actions.undo.enabled)
        wait(300)
        undo()
        tryRead(() => delegateValue(grid, 0, "rating"), 0)
        tryVerify(() => !app.actions.undo.enabled, 5000, "the second press was not a step")

        rateSelected(1)
        rateSelected(2)
        tryRead(() => delegateValue(grid, 0, "rating"), 2)
        wait(300) // the engine's word that both steps are in the history reaches the menu
        undo()
        tryRead(() => delegateValue(grid, 0, "rating"), 1)
        wait(300)
        undo()
        tryRead(() => delegateValue(grid, 0, "rating"), 0)
    }

    function test_undo_shows_the_photo_it_undid() {
        clickCell(5)
        rateSelected(3)
        tryVerify(() => app.actions.undo.enabled)
        clickCell(1)
        compare(grid.currentIndex, 1)
        undo()
        tryCompare(grid, "currentIndex", 5)
        compare(grid.itemAtIndex(5).rating, 0)
        verify(app.library.summary !== "", "the strip shows it")
    }

    function test_undo_waits_for_a_dialog_and_a_text_field_keeps_its_own() {
        clickCell(0)
        rateSelected(2)
        tryVerify(() => app.actions.undo.enabled)
        app.showSettings()
        wait(200)
        verify(!app.actions.undo.enabled, "the photos are behind a dialog")
        undo()
        wait(200)
        compare(grid.itemAtIndex(0).rating, 2, "and Ctrl+Z did not reach them")
        app.settingsDialog.close()
        wait(300)
        verify(app.actions.undo.enabled)
    }

    function test_undo_and_redo_are_named_in_the_language() {
        app.launcher.chooseLanguage("fr")
        wait(200)
        clickCell(0)
        rateSelected(2)
        tryVerify(() => app.actions.undo.enabled)
        compare(app.actions.undo.text, "Annuler la note")
        undo()
        tryVerify(() => app.actions.redo.enabled)
        compare(app.actions.redo.text, "Rétablir la note")
    }

    function test_the_filter_bar_lists_photos_by_rating_and_says_how_many() {
        compare(app.library.status, "80 photos")
        clickCell(0)
        rateSelected(3)
        clickCell(1)
        rateSelected(5)
        clickCell(2)
        rateSelected(1)
        // The ratings reach the catalogue on the engine's own time (slower on a busy runner): wait until it lists them.
        tryVerify(() => {
            app.library.filterBy(3)
            return app.photos.count === 2
        }, 15000)
        app.library.filterBy(0)
        tryCompare(app.photos, "count", 80)
        const buttons = app.library.filterButtons
        compare(buttons.count, 6)
        compare(buttons.itemAt(0).text, "All")
        verify(buttons.itemAt(0).highlighted, "All is the filter to begin with")

        mouseClick(buttons.itemAt(3))
        tryCompare(app.photos, "count", 2)
        compare(app.photos.minRating, 3)
        compare(app.library.status, "2 photos")
        verify(buttons.itemAt(3).highlighted && !buttons.itemAt(0).highlighted)
        compare(grid.currentIndex, -1, "a new filter drops the selection")
        mouseClick(buttons.itemAt(5))
        tryCompare(app.photos, "count", 1)
        compare(app.library.status, "1 photo")
        mouseClick(buttons.itemAt(0))
        tryCompare(app.photos, "count", 80)
    }

    function test_the_sentences_follow_the_language_with_their_plural_forms() {
        app.launcher.chooseLanguage("fr")
        wait(200)
        compare(app.library.filterButtons.itemAt(0).text, "Tout")
        compare(app.library.status, "80 photos")
        app.library.filterBy(5)
        compare(app.library.status, "0 photo", "French counts zero as one")
        app.library.filterBy(0)
        clickCell(0)
        rateSelected(1)
        compare(grid.itemAtIndex(0).Accessible.name, "Photo, 1 étoile")
        rateSelected(2)
        compare(grid.itemAtIndex(0).Accessible.name, "Photo, 2 étoiles")
        app.launcher.chooseLanguage("en")
        wait(200)
        compare(grid.itemAtIndex(0).Accessible.name, "Photo, 2 stars")
        grid.forceActiveFocus()
        rateSelected(1)
        compare(grid.itemAtIndex(0).Accessible.name, "Photo, 1 star")
    }

    // "in view" when the cell at `index` is made and inside the visible part of the grid, else where it is and where the view is.
    function whereIs(index) {
        const item = grid.itemAtIndex(index)
        if (!item)
            return "no cell is made at " + index
        const top = item.y
        const bottom = item.y + item.height
        return bottom > grid.contentY && top < grid.contentY + grid.height
            ? "in view"
            : "the cell is at " + top + " to " + bottom + ", the view at " + grid.contentY + " for " + grid.height
    }

    // (Each step waits for what it asserts, reading it again at each poll: a fixed pause and one read failed once on a slow
    // macOS runner, when the layout and then the scroll took longer than the pause, issue #72.)
    function test_a_resized_window_shows_as_many_columns_as_fit_and_keeps_the_selected_photo() {
        app.width = 1400 + panelWidth
        tryCompare(grid, "columns", 8)
        clickCell(3)
        // 1100 wide holds six cells.
        app.width = 1100 + panelWidth
        tryCompare(grid, "columns", 6)
        tryCompare(grid, "currentIndex", 3, 5000, "the selection stays on its photo")
        // Narrower than one cell would still show one column; the window's minimum shows three.
        app.width = 640 + panelWidth
        tryCompare(grid, "columns", 3)
        app.width = 1900 + panelWidth
        tryCompare(grid, "columns", 11)
        tryCompare(grid, "currentIndex", 3)
        // A selection that a resize pushed out of view is brought back.
        app.library.select(60)
        app.width = 1400 + panelWidth
        // The rows re-flow first (the content is as high as eight columns make it); before that the photo is still where the
        // eleven columns of the step before put it, `select(60)` has just brought it into view there, and "in view" would be
        // said of the old layout: a check that is true before the thing it checks has happened (Django's review of #93).
        tryCompare(grid, "contentHeight", Math.ceil(grid.count / 8) * grid.cellHeight)
        tryRead(() => whereIs(60), "in view", 5000, "the selected photo is in view after the columns changed")
    }

    // The grid sits in the middle of its panel (D-144): what the columns leave over is shared by the two sides.
    function test_the_grid_is_centred_in_its_panel() {
        // 1400 holds eight cells of 164 and leaves 88: 44 on each side.
        app.width = 1400 + panelWidth
        wait(300)
        compare(grid.columns, 8)
        const left = grid.leftMargin
        const right = grid.width - left - grid.columns * grid.cellWidth
        verify(left > 0, "there is room to share: " + left)
        verify(Math.abs(left - right) <= 1, "as much on the left (" + left + ") as on the right (" + right + ")")
        compare(grid.itemAtIndex(0).mapToItem(grid, 0, 0).x, left, "the first cell starts after the margin, as drawn")
        // The margin changes under the view when the window is resized, and the cells follow it (Qt would leave them where the
        // last margin put them).
        app.width = 1450 + panelWidth
        wait(300)
        compare(grid.columns, 8)
        compare(grid.itemAtIndex(0).mapToItem(grid, 0, 0).x, grid.leftMargin, "still after the margin, once the margin changed")
        compare(grid.contentX, grid.originX - grid.leftMargin)
        app.width = 1400 + panelWidth
        wait(300)
        // Exactly full: nothing to share, the grid is where it was.
        app.width = shownWidth(8)
        wait(300)
        compare(grid.leftMargin, 0)
        // Narrower than one cell: still no negative margin.
        app.width = 100 + panelWidth
        wait(300)
        compare(grid.leftMargin >= 0, true)
    }

    // A press in the margin is a press on empty space: it clears the selection, as it did when the leftover lay on the right.
    // A photo is still where it shows, and the rubber band takes the columns from the content's own left.
    function test_with_the_grid_centred_a_click_in_the_margin_clears_and_a_photo_and_a_rubber_band_are_where_they_show() {
        app.width = 1400 + panelWidth
        wait(300)
        verify(grid.leftMargin > 10)
        clickWhereItIs(1, "centred")
        compare(app.library.selectedCount, 1)
        mouseClick(grid, 4, 60)
        wait(60)
        compare(app.library.selectedCount, 0, "a click in the left margin clears the selection")
        clickWhereItIs(2, "centred")
        // (Not on the scrollbar's own 12px along the right edge.)
        mouseClick(grid, grid.width - 24, 60)
        wait(60)
        compare(app.library.selectedCount, 0, "and in the right margin")
        // A rubber band from the margin over the first two cells of the first row selects those two, and no more.
        const first = cell(0).mapToItem(grid, 0, 0)
        const y = first.y + 40
        mousePress(grid, 4, y)
        mouseMove(grid, first.x + grid.cellWidth + 60, y + 4)
        mouseMove(grid, first.x + grid.cellWidth + 64, y + 6)
        mouseRelease(grid, first.x + grid.cellWidth + 64, y + 6)
        wait(100)
        compare(app.library.selectedCount, 2, "the first two photos of the row")
        verify(app.photos.isSelected(0) && app.photos.isSelected(1))
    }

    // Clicks the middle of what shows of the cell that is on screen (no scrolling to it first), and says which photo is
    // selected: it must be that cell's.
    function clickWhereItIs(index, note) {
        const item = grid.itemAtIndex(index)
        verify(item, "the cell " + index + " is on screen")
        const top = Math.max(0, item.mapToItem(grid, 0, 0).y + 6)
        const bottom = Math.min(grid.height, item.mapToItem(grid, 0, 0).y + app.library.thumbH - 6)
        if (bottom - top < 8)
            return
        const x = item.mapToItem(grid, item.width / 2, 0).x, y = (top + bottom) / 2
        mouseClick(grid, x, y)
        wait(40)
        compare(grid.currentIndex, index, note + ": clicked the cell " + index + " at " + x + "," + y
                + " (contentY " + grid.contentY + ", cell " + grid.cellWidth + "x" + grid.cellHeight + ")")
    }

    function test_after_scrolling_and_resizing_the_thumbnails_a_click_selects_the_cell_under_the_pointer() {
        const slider = app.library.sizeSlider
        for (const [scroll, sizes] of [[0.6, [232, 208, 184, 160, 128]], [0.5, [160, 200, 240]], [0.9, [200, 96]], [0.3, [128, 256]]]) {
            // Scrolled first, then the thumbnails resized in steps, as a drag of the slider does.
            slider.value = 256
            slider.moved()
            wait(300)
            grid.contentY = Math.max(0, (grid.contentHeight - grid.height) * scroll)
            wait(200)
            for (const size of sizes) {
                slider.value = size
                slider.moved()
                wait(20)
            }
            wait(400)
            let odd = []
            for (let index = 0; index < grid.count; index++) {
                const it = grid.itemAtIndex(index)
                if (it) {
                    const wantX = grid.originX + (index % grid.columns) * grid.cellWidth, wantY = grid.originY + Math.floor(index / grid.columns) * grid.cellHeight
                    if (Math.abs(it.x - wantX) > 1 || Math.abs(it.y - wantY) > 1)
                        odd.push(index + ":" + it.x + "," + it.y + " want " + wantX + "," + wantY)
                }
            }
            verify(odd.length === 0, "misplaced (origin " + grid.originY + ", cols " + grid.columns + ", cell " + grid.cellWidth + "x" + grid.cellHeight + ", contentY " + grid.contentY + "): " + odd.slice(0, 6).join(" | "))
            for (let index = 0; index < grid.count; index++)
                if (grid.itemAtIndex(index))
                    clickWhereItIs(index, "scroll " + scroll + ", sizes " + sizes)
        }
    }

    // The last `n` segments of a path, slashes and backslashes both understood (the engine may have resolved the
    // home the tests were given to a different real path, symlinks and short Windows names included: what the
    // reveal or the export names is compared to the fixture's own naming, not to the raw `AURORAW_TEST_HOME`).
    function tail(path, n) {
        const parts = path.split(/[\\\/]/)
        return parts.slice(-n).join("/")
    }

    function test_show_in_file_manager_records_the_photos_file_from_the_cell_menu_and_the_view_menu() {
        const name = app.photos.infoAt(0).split(" ")[0]
        app.library.revealCell(0)
        compare(tail(app.photos.lastRevealedPath(), 2), "Card/" + name)
        // The image view's menu acts on the photo it shows, the same way.
        app.library.openView(1)
        tryVerify(() => app.library.viewing)
        const shown = app.photos.infoAt(1).split(" ")[0]
        app.library.revealCell(-1)
        compare(tail(app.photos.lastRevealedPath(), 2), "Card/" + shown)
        keyClick(Qt.Key_Escape)
    }

    function test_export_writes_every_listed_photos_file_one_a_line_and_follows_the_filter() {
        const path = home + "/list.txt"
        verify(app.library.exportButton.enabled)
        verify(app.photos.exportListedTo(path))
        let lines = files.read(path).trim().split("\n")
        compare(lines.length, 80)
        verify(lines.every(line => tail(line, 2).indexOf("Card/IMG_") === 0), lines[0])
        // A stricter filter (nothing is rated yet) exports fewer, and none once it is empty.
        app.library.filterBy(5)
        tryCompare(app.photos, "count", 0)
        verify(!app.library.exportButton.enabled, "nothing to export")
        verify(app.photos.exportListedTo(path))
        compare(files.read(path), "")
        app.library.filterBy(0)
        tryCompare(app.photos, "count", 80)
    }

    function test_exporting_the_list_remembers_the_last_folder_chosen() {
        // Issue #18.
        compare(Folders.last("export-photos"), "", "nothing remembered yet")
        const folder = home + "/Reports"
        files.mkdir(folder)
        app.library.exportDialog.choose(folder + "/list.txt")
        compare(Folders.last("export-photos"), folder)
    }
}
