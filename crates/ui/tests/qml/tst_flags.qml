// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.ui

// Flags (spec §5.3, D-098): P, X and U on one photo and on a selection, the same key taking the flag off,
// a rejected photo dimmed and kept in place until the list is read again, the flag filter, undo as one
// step, and the names in both languages. The 40-photo machine.
AppTestCase {
    name: "Flags"

    property var grid: null

    function init() {
        launchWithPhotos(40)
        grid = app.library.grid
    }

    // Every flag a test set is cleared afterwards (the machine is shared), and the language put back.
    function cleanup() {
        if (app) {
            app.launcher.chooseLanguage("en")
            app.library.filterFlags(1)
            app.library.selectAll()
            app.photos.flagSelection("clear")
            wait(300)
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
        mouseClick(item, item.width / 2, item.height / 2, Qt.LeftButton, modifiers === undefined ? Qt.NoModifier : modifiers)
        wait(40)
    }

    function key(code) { keyClick(code) }

    function test_p_and_x_flag_a_photo_and_the_same_key_takes_the_flag_off() {
        click(2)
        key(Qt.Key_P)
        compare(cell(2).flag, 1, "picked")
        key(Qt.Key_P)
        compare(cell(2).flag, 0, "the same key again clears it")
        key(Qt.Key_X)
        compare(cell(2).flag, 2, "rejected")
        key(Qt.Key_P)
        compare(cell(2).flag, 1, "the other key replaces it")
        key(Qt.Key_U)
        compare(cell(2).flag, 0, "U clears")
        key(Qt.Key_U)
        compare(cell(2).flag, 0)
    }

    function test_a_selection_is_flagged_as_one_action_all_or_none() {
        click(2)
        key(Qt.Key_X)
        wait(300)
        click(2)
        click(5, Qt.ShiftModifier)
        key(Qt.Key_X)
        for (let i = 2; i <= 5; i++)
            compare(cell(i).flag, 2, "one of them was rejected already: X sets it on all")
        // (The step before it, "Undo flag", is still the label until the engine has recorded this one.)
        tryVerify(() => app.actions.undo.text === "Undo 3 flags", 5000,
                  "the photo that had the flag is not part of the step: " + app.actions.undo.text)
        key(Qt.Key_X)
        for (let i = 2; i <= 5; i++)
            compare(cell(i).flag, 0, "every photo had it: X takes it off all")
    }

    function test_undo_takes_a_flag_back_in_one_step() {
        click(3)
        key(Qt.Key_P)
        tryVerify(() => app.actions.undo.enabled, 5000)
        compare(app.actions.undo.text, "Undo flag")
        wait(300)
        keyClick(Qt.Key_Z, Qt.ControlModifier)
        tryRead(() => delegateValue(grid, 3, "flag"), 0)
        tryVerify(() => app.actions.redo.enabled)
        compare(app.actions.redo.text, "Redo flag")
        keyClick(Qt.Key_Y, Qt.ControlModifier)
        tryRead(() => delegateValue(grid, 3, "flag"), 1)
    }

    function test_a_rejected_photo_stays_dimmed_until_the_list_is_read_again() {
        click(4)
        key(Qt.Key_X)
        compare(cell(4).flag, 2)
        compare(app.photos.count, 40, "it did not leave under the cursor")
        snapshot("grid-flags-en")
        wait(2200) // what was asked is confirmed by the catalogue
        mouseClick(app.library.refreshButton)
        compare(app.photos.count, 39, "the Refresh button reads the list again and hides it")
        compare(app.library.status, "39 photos")
        app.library.filterFlags(3)
        tryCompare(app.photos, "count", 1)
        app.library.filterFlags(1)
        tryCompare(app.photos, "count", 40, 20000)
        app.library.filterFlags(0)
        tryCompare(app.photos, "count", 39)
    }

    function test_the_flag_filters_list_picked_and_rejected_photos() {
        click(1)
        key(Qt.Key_P)
        click(2)
        key(Qt.Key_P)
        click(6)
        key(Qt.Key_X)
        wait(600)
        app.library.filterFlags(2)
        tryCompare(app.photos, "count", 2)
        compare(app.photos.flagFilter, 2)
        app.library.filterFlags(3)
        tryCompare(app.photos, "count", 1)
        // Combined with a rating: nothing is rated, so nothing is listed.
        app.library.filterBy(3)
        tryCompare(app.photos, "count", 0)
        compare(app.photos.flagFilter, 3, "the flag filter stayed")
        app.library.filterBy(0)
        app.library.filterFlags(0)
        tryCompare(app.photos, "count", 39)
    }

    function test_the_strip_shows_the_flag_of_one_photo() {
        click(7)
        key(Qt.Key_P)
        tryVerify(() => app.library.summary.indexOf("✔") >= 0, 5000, app.library.summary)
        key(Qt.Key_X)
        tryVerify(() => app.library.summary.indexOf("✖") >= 0, 5000, app.library.summary)
    }

    function test_the_flags_are_named_in_the_language() {
        compare(app.library.flagName(0), "Not rejected")
        app.launcher.chooseLanguage("fr")
        wait(250)
        compare(app.library.flagName(0), "Non refusées")
        compare(app.library.flagName(2), "Retenues")
        compare(app.library.flagName(3), "Refusées")
        click(3)
        key(Qt.Key_X)
        tryVerify(() => app.actions.undo.enabled, 5000)
        compare(app.actions.undo.text, "Annuler le drapeau")
        snapshot("grid-flags-fr")
    }
}
