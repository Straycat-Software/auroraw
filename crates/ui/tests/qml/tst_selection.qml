// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.ui

// Selecting several photos in the grid (D-097): click, Ctrl and Shift with the mouse and the keyboard, the
// cursor and the anchor, Space, Escape, Select all, none and invert, the rubber band, rating what is
// selected as one step, and what survives a reload. The 80-photo machine; the window is 1400 wide, 8 columns.
AppTestCase {
    name: "Selection"

    property var grid: null
    property var rated: []

    function init() {
        launch("")
        // The keyword panel takes 320 px: this width leaves the grid 1360, still 8 columns.
        app.width = 1680
        wait(300)
        tryCompare(app.photos, "count", 80)
        grid = app.library.grid
        compare(grid.columns, 8)
    }

    // What a test rated is cleared afterwards, and the language put back (the machine is shared).
    function cleanup() {
        if (app) {
            app.launcher.chooseLanguage("en")
            app.library.filterBy(0)
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

    function cell(index) {
        grid.positionViewAtIndex(index, GridView.Contain)
        wait(30)
        const item = grid.itemAtIndex(index)
        verify(item, "the cell " + index + " exists")
        return item
    }

    function click(index, modifiers) {
        const item = cell(index)
        mouseClick(item, item.width / 2, item.height / 2, Qt.LeftButton, modifiers === undefined ? Qt.NoModifier : modifiers)
        wait(40)
    }

    function selectedRows() {
        const rows = []
        for (let i = 0; i < app.photos.count; i++)
            if (app.photos.isSelected(i))
                rows.push(i)
        return rows
    }

    function range(from, to) {
        const rows = []
        for (let i = from; i <= to; i++)
            rows.push(i)
        return rows
    }

    function key(code, modifiers) { keyClick(code, modifiers === undefined ? Qt.NoModifier : modifiers) }

    function test_a_click_selects_only_that_photo() {
        click(3)
        compare(selectedRows(), [3])
        click(5)
        compare(selectedRows(), [5])
        compare(grid.currentIndex, 5)
    }

    function test_ctrl_click_adds_and_removes_one_photo() {
        click(1)
        click(4, Qt.ControlModifier)
        click(9, Qt.ControlModifier)
        compare(selectedRows(), [1, 4, 9])
        click(4, Qt.ControlModifier)
        compare(selectedRows(), [1, 9], "Ctrl+click on a selected photo removes it")
        compare(grid.currentIndex, 4, "the cursor goes where the click was")
        compare(app.library.summary, "2 photos selected")
    }

    function test_shift_click_selects_the_range_from_the_anchor_in_either_direction() {
        click(10)
        click(13, Qt.ShiftModifier)
        compare(selectedRows(), range(10, 13))
        click(8, Qt.ShiftModifier)
        compare(selectedRows(), range(8, 10), "the anchor stayed on 10, and the new range replaced the old")
        click(20, Qt.ShiftModifier | Qt.ControlModifier)
        compare(selectedRows(), range(8, 20), "Ctrl+Shift adds the range to what was selected")
        click(30)
        compare(selectedRows(), [30])
    }

    function test_shift_and_the_keys_extend_the_range() {
        click(10)
        key(Qt.Key_Right, Qt.ShiftModifier)
        key(Qt.Key_Right, Qt.ShiftModifier)
        compare(selectedRows(), [10, 11, 12])
        key(Qt.Key_Down, Qt.ShiftModifier)
        compare(selectedRows(), range(10, 20), "a row down is a range to the photo below")
        key(Qt.Key_Left, Qt.ShiftModifier)
        compare(selectedRows(), range(10, 19))
        key(Qt.Key_Home, Qt.ShiftModifier)
        compare(selectedRows(), range(0, 10))
        key(Qt.Key_End, Qt.ShiftModifier)
        compare(selectedRows(), range(10, 79))
        key(Qt.Key_PageUp, Qt.ShiftModifier)
        const page = grid.visibleRows * grid.columns
        compare(grid.currentIndex, 79 - page, "the cursor moved a page up")
        compare(selectedRows(), range(Math.min(10, 79 - page), Math.max(10, 79 - page)), "and the range follows")
    }

    function test_a_plain_move_after_a_range_collapses_to_the_photo_landed_on() {
        click(10)
        key(Qt.Key_Right, Qt.ShiftModifier)
        key(Qt.Key_Right, Qt.ShiftModifier)
        compare(selectedRows().length, 3)
        key(Qt.Key_Right)
        compare(selectedRows(), [13])
        key(Qt.Key_Down)
        compare(selectedRows(), [21])
        key(Qt.Key_End)
        compare(selectedRows(), [79])
        key(Qt.Key_Home)
        compare(selectedRows(), [0])
    }

    function test_ctrl_and_the_keys_move_the_cursor_and_space_toggles_under_it() {
        click(10)
        key(Qt.Key_Right, Qt.ControlModifier)
        key(Qt.Key_Right, Qt.ControlModifier)
        compare(grid.currentIndex, 12)
        compare(selectedRows(), [10], "the selection did not move")
        key(Qt.Key_Space)
        compare(selectedRows(), [10, 12])
        key(Qt.Key_Space)
        compare(selectedRows(), [10])
        key(Qt.Key_Home, Qt.ControlModifier)
        compare(grid.currentIndex, 0)
        compare(selectedRows(), [10])
        key(Qt.Key_Space)
        compare(selectedRows(), [0, 10])
        // A range from there, with Shift, starts at the photo Space toggled.
        key(Qt.Key_Right, Qt.ShiftModifier)
        compare(selectedRows(), [0, 1])
    }

    function test_escape_clears_the_selection_and_the_cursor_stays() {
        click(5)
        click(9, Qt.ShiftModifier)
        compare(selectedRows().length, 5)
        key(Qt.Key_Escape)
        compare(selectedRows(), [])
        compare(grid.currentIndex, 9)
        compare(app.library.summary, "")
    }

    function test_select_all_none_and_invert_from_the_keyboard_and_the_menu() {
        verify(app.actions.selectAll.enabled)
        verify(!app.actions.selectNone.enabled, "nothing to deselect")
        grid.forceActiveFocus()
        key(Qt.Key_A, Qt.ControlModifier)
        compare(selectedRows().length, 80)
        compare(app.library.summary, "80 photos selected")
        verify(app.actions.selectNone.enabled)
        key(Qt.Key_A, Qt.ControlModifier | Qt.ShiftModifier)
        compare(selectedRows().length, 0)

        click(1)
        click(2, Qt.ControlModifier)
        key(Qt.Key_I, Qt.ControlModifier | Qt.ShiftModifier)
        const rows = selectedRows()
        compare(rows.length, 78)
        verify(rows.indexOf(1) < 0 && rows.indexOf(2) < 0)
        app.actions.selectNone.trigger()
        compare(selectedRows().length, 0)
    }

    function test_a_text_field_keeps_select_all_for_itself() {
        click(3)
        app.newDialog.openWith()
        wait(200)
        app.newDialog.nameField.forceActiveFocus()
        key(Qt.Key_A, Qt.ControlModifier)
        verify(app.newDialog.nameField.selectedText.length > 0, "the field selected its text")
        compare(selectedRows(), [3], "and the grid's selection did not change")
        verify(!app.actions.selectNone.enabled, "the grid is behind a dialog")
        app.newDialog.close()
        wait(300)
    }

    function test_a_rubber_band_selects_what_it_touches_and_ctrl_adds_to_the_selection() {
        // From the gap left of the first photo, over the first four columns of three rows.
        mouseDrag(grid, 2, 2, 164 * 3 + 100, 124 * 2 + 50)
        wait(60)
        compare(selectedRows(), [0, 1, 2, 3, 8, 9, 10, 11, 16, 17, 18, 19])
        // A smaller band is a smaller selection, and Ctrl keeps what was there.
        click(60)
        grid.positionViewAtBeginning()
        wait(60)
        mouseDrag(grid, 2, 2, 164 + 100, 60, Qt.LeftButton, Qt.ControlModifier)
        wait(60)
        compare(selectedRows(), [0, 1, 60])
        // A click on empty space, in a gap, clears the selection.
        mouseClick(grid, 2, 2)
        wait(60)
        compare(selectedRows(), [])
    }

    function rateSelection(stars) {
        for (const row of selectedRows())
            rated.push(app.photos.idAt(row))
        key(Qt.Key_0 + stars)
    }

    function test_rating_a_selection_is_one_step_and_undo_selects_the_photos_again() {
        click(2)
        click(5, Qt.ShiftModifier)
        compare(selectedRows(), range(2, 5))
        rateSelection(4)
        for (let i = 2; i <= 5; i++)
            compare(grid.itemAtIndex(i).rating, 4)
        tryVerify(() => app.actions.undo.enabled, 5000)
        compare(app.actions.undo.text, "Undo 4 ratings")
        wait(300)
        click(30) // the selection is somewhere else when the undo comes
        key(Qt.Key_Z, Qt.ControlModifier)
        tryCompare(grid.itemAtIndex(2), "rating", 0)
        for (let i = 2; i <= 5; i++)
            compare(app.photos.rowOf(app.photos.idAt(i)), i)
        tryCompare(app.photos, "selectedCount", 4)
        compare(selectedRows(), range(2, 5), "what the undo touched is what is selected")
        tryVerify(() => app.actions.undo.enabled === false, 5000, "one step undid all four")
        key(Qt.Key_Y, Qt.ControlModifier)
        tryCompare(grid.itemAtIndex(4), "rating", 4)
    }

    function test_the_selection_survives_a_reload_and_a_new_filter_clears_it() {
        click(2)
        click(4, Qt.ControlModifier)
        app.library.reload()
        compare(selectedRows(), [2, 4])
        compare(grid.currentIndex, 4)
        app.library.filterBy(3)
        compare(app.photos.selectedCount, 0)
        app.library.filterBy(0)
        compare(selectedRows(), [])
    }

    function test_a_selection_is_named_in_the_language_and_drawn() {
        click(6)
        click(12, Qt.ShiftModifier)
        click(30, Qt.ControlModifier)
        snapshot("grid-selection-en")
        app.launcher.chooseLanguage("fr")
        wait(250)
        click(31, Qt.ControlModifier)
        compare(app.library.summary, "9 photos sélectionnées")
        compare(app.actions.selectNone.text, "Ne rien sélectionner")
        compare(app.actions.invertSelection.text, "Inverser la sélection")
        snapshot("grid-selection-fr")
    }
}
