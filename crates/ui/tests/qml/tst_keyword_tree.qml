// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.ui

// Reorganising the vocabulary (D-099): a keyword dragged onto another, or onto the strip for the top level,
// the Move dialog, a drop that cannot be done, deleting a branch with the numbers said and taking it back
// with Ctrl+Z, renaming undone, its synonyms and export flag set as one step (WP10 slice 2), the keyword
// filter that goes when its keyword does, and French.
// The 40-photo machine; each test uses names of its own.
AppTestCase {
    name: "KeywordTree"

    property var grid: null

    function init() {
        launchWithPhotos(40)
        grid = app.library.grid
    }

    // Every keyword is taken off every photo, and the language put back (the vocabulary stays).
    function cleanup() {
        if (app) {
            app.launcher.chooseLanguage("en")
            app.library.filterKeyword("", "")
            app.keywordPanel.filterField.text = ""
            app.library.selectAll()
            for (let i = 0; i < app.library.keywords.count; i++)
                app.photos.keywordSelection(app.library.keywords.idAt(i), false)
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

    function selectFirst(count) {
        click(0)
        if (count > 1)
            click(count - 1, Qt.ShiftModifier)
        compare(app.photos.selectedCount, count)
    }

    function keywords() { return app.library.keywords }

    function rowOf(name) {
        for (let i = 0; i < keywords().count; i++)
            if (keywords().nameAt(i) === name)
                return i
        return -1
    }

    function idOf(name) {
        const row = rowOf(name)
        verify(row >= 0, name + " is listed")
        return keywords().idAt(row)
    }

    function itemAtRow(row) {
        const tree = app.keywordPanel.tree
        tree.positionViewAtIndex(row, ListView.Contain)
        wait(30)
        const it = tree.itemAtIndex(row)
        verify(it, "the row " + row + " exists")
        return it
    }

    function item(name) {
        const row = rowOf(name)
        verify(row >= 0, "the keyword " + name + " is listed")
        return itemAtRow(row)
    }

    function lastRowOf(name) {
        for (let i = keywords().count - 1; i >= 0; i--)
            if (keywords().nameAt(i) === name)
                return i
        return -1
    }

    // Makes a keyword (no photo needed) and lists it.
    function make(name, parent) {
        const id = keywords().create(name, parent === undefined ? "" : parent)
        verify(id.indexOf("error:") !== 0, id)
        return id
    }

    // Drags the row `from` (by its name) onto `target`, an item with coordinates; `hold` keeps the button
    // down at the end, for a picture.
    function drag(from, target, tx, ty, hold) {
        const x = from.depth * 14 + 60
        mousePress(from, x, from.height / 2)
        mouseMove(from, x + 12, from.height / 2 + 2)
        mouseMove(from, x + 30, from.height / 2 + 4)
        wait(50)
        mouseMove(target, tx, ty)
        wait(50)
        mouseMove(target, tx + 1, ty)
        wait(50)
        if (!hold) {
            mouseRelease(target, tx + 1, ty)
            wait(150)
        }
    }

    function undoNow() {
        wait(300)
        grid.forceActiveFocus()
        keyClick(Qt.Key_Z, Qt.ControlModifier)
    }

    // Waits until a row of the tree has `value` for `property`. (Asks again every time: the list rebuilds its
    // rows when a keyword moves, comes back or goes, so a row found earlier may be gone.)
    function tryItem(name, property, value) {
        tryVerify(() => {
            const row = rowOf(name)
            if (row < 0)
                return false
            app.keywordPanel.tree.positionViewAtIndex(row, ListView.Contain)
            const it = app.keywordPanel.tree.itemAtIndex(row)
            return it !== null && it !== undefined && it[property] === value
        }, 5000, name + "." + property + " never became " + value)
    }

    function test_a_keyword_dragged_onto_another_becomes_its_child_and_undo_puts_it_back() {
        make("Places")
        make("Peru")
        compare(item("Peru").depth, 0)
        const places = item("Places")
        drag(item("Peru"), places, 100, places.height / 2, true)
        verify(app.keywordPanel.dragging, "a drag is under way")
        verify(app.keywordPanel.ghost.visible)
        snapshot("keywords-drag-en")
        mouseRelease(places, 101, places.height / 2)
        wait(200)
        verify(!app.keywordPanel.dragging)
        tryItem("Peru", "depth", 1)
        compare(rowOf("Peru"), rowOf("Places") + 1)
        tryVerify(() => app.actions.undo.text === "Undo moving the keyword", 5000, app.actions.undo.text)
        undoNow()
        tryItem("Peru", "depth", 0)
        tryVerify(() => app.actions.redo.text === "Redo moving the keyword")
    }

    function test_a_keyword_dropped_where_there_is_no_keyword_goes_to_the_top_level() {
        const places = make("Continent")
        make("Chile", places)
        compare(item("Chile").depth, 1)
        const from = item("Chile")
        const x = from.depth * 14 + 60
        mousePress(from, x, from.height / 2)
        mouseMove(from, x + 12, from.height / 2)
        mouseMove(from, x + 30, from.height / 2 + 4)
        tryVerify(() => app.keywordPanel.dragging)
        // Above the list, among the field and the line that says where new keywords go: no keyword there.
        const panel = app.keywordPanel
        mouseMove(panel, panel.width / 2, 100)
        wait(50)
        mouseMove(panel, panel.width / 2 + 1, 100)
        tryVerify(() => panel.topLevelDrop.containsDrag && panel.topLevelDrop.allowed)
        mouseRelease(panel, panel.width / 2 + 1, 100)
        tryItem("Chile", "depth", 0)
    }

    function test_a_drop_that_cannot_be_done_changes_nothing_and_says_why() {
        const a = make("Alpha")
        make("Beta", a)
        const target = item("Beta")
        drag(item("Alpha"), target, 100, target.height / 2)
        compare(item("Alpha").depth, 0, "a keyword does not go under its own")
        compare(item("Beta").depth, 1)
        compare(app.keywordPanel.note, "A keyword cannot be moved under itself or under one of its own keywords.")
        // A name that is already there is refused too.
        app.keywordPanel.note = ""
        make("Gamma")
        const g2 = make("Delta")
        make("Gamma", g2)
        const gamma = item("Delta")
        drag(itemAtRow(lastRowOf("Gamma")), gamma, 100, gamma.height / 2) // the top-level Gamma onto Delta, which has a Gamma
        compare(item("Delta").depth, 0)
        compare(itemAtRow(lastRowOf("Gamma")).depth, 0, "the top-level Gamma did not move")
        compare(app.keywordPanel.note, "There is already a keyword named “Gamma” there.")
    }

    function test_the_move_dialog_lists_where_a_keyword_can_go() {
        const holder = make("Holder")
        make("Movable")
        make("Inside", idOf("Movable"))
        const dialog = app.keywordPanel.moveDialog
        dialog.openFor(idOf("Movable"), "Movable")
        tryVerify(() => dialog.visible)
        verify(app.dialogOpen, "the window's commands wait")
        const paths = dialog.targets.map(t => t.path)
        verify(paths.indexOf("Holder") >= 0)
        compare(paths.indexOf("Movable"), -1, "not under itself")
        compare(paths.indexOf("Movable › Inside"), -1, "nor under its own")
        dialog.targetBox.currentIndex = paths.indexOf("Holder")
        dialog.tryMove()
        tryVerify(() => !dialog.visible)
        tryItem("Movable", "depth", 1)
        compare(rowOf("Movable"), rowOf("Holder") + 1)
        compare(item("Inside").depth, 2, "with its branch")
    }

    function test_deleting_a_branch_says_what_it_takes_and_ctrl_z_brings_everything_back() {
        selectFirst(4)
        const animals = make("Animals")
        const birds = make("Birds", animals)
        app.photos.keywordSelection(birds, true)
        click(9)
        app.photos.keywordSelection(animals, true)
        tryVerify(() => item("Birds").photos === 4 && item("Animals").photos === 1, 5000)

        const dialog = app.keywordPanel.deleteDialog
        dialog.openFor(animals)
        tryVerify(() => dialog.visible)
        compare(dialog.branchKeywords, 2)
        compare(dialog.branchPhotos, 5)
        verify(app.dialogOpen)
        snapshot("keywords-delete-en")
        dialog.confirm()
        tryVerify(() => !dialog.visible)
        tryVerify(() => rowOf("Animals") === -1 && rowOf("Birds") === -1, 5000)
        tryVerify(() => app.actions.undo.text === "Undo deleting 2 keywords", 5000, app.actions.undo.text)

        undoNow()
        tryVerify(() => rowOf("Birds") >= 0 && rowOf("Animals") >= 0, 5000)
        compare(item("Birds").depth, 1)
        tryItem("Birds", "photos", 4)
        tryItem("Animals", "photos", 1)
        tryVerify(() => app.actions.redo.text === "Redo deleting 2 keywords")
        wait(300)
        keyClick(Qt.Key_Y, Qt.ControlModifier)
        tryVerify(() => rowOf("Animals") === -1, 5000)
    }

    function test_the_list_filtered_by_a_deleted_keyword_shows_everything_again() {
        selectFirst(3)
        const gone = make("Gone")
        app.photos.keywordSelection(gone, true)
        tryVerify(() => item("Gone").photos === 3, 5000)
        app.library.filterKeyword(gone, "Gone")
        tryCompare(app.photos, "count", 3)
        const dialog = app.keywordPanel.deleteDialog
        dialog.openFor(gone)
        dialog.confirm()
        tryCompare(app.photos, "count", 40, 20000)
        compare(app.photos.keywordFilter, "")
    }

    function test_a_rename_is_undone() {
        selectFirst(2)
        make("Before")
        const id = idOf("Before")
        app.photos.keywordSelection(id, true)
        const dialog = app.keywordPanel.renameDialog
        dialog.openFor(rowOf("Before"), "Before")
        dialog.nameField.text = "After"
        dialog.tryRename()
        tryVerify(() => rowOf("After") >= 0, 5000)
        tryVerify(() => app.actions.undo.text === "Undo renaming the keyword", 5000, app.actions.undo.text)
        undoNow()
        tryVerify(() => rowOf("Before") >= 0 && rowOf("After") === -1, 5000)
        compare(item("Before").photos, 2)
    }

    function test_the_properties_dialog_opens_with_the_current_values_and_saves_as_one_step() {
        make("Redwood")
        const dialog = app.keywordPanel.propertiesDialog
        dialog.openFor(rowOf("Redwood"), "Redwood")
        tryVerify(() => dialog.visible)
        verify(app.dialogOpen, "the window's commands wait")
        compare(dialog.synonymsField.text, "", "no synonym yet")
        compare(dialog.doNotExportField.checked, false, "exported by default")

        dialog.synonymsField.text = "Sequoia\nCoast redwood"
        dialog.doNotExportField.checked = true
        dialog.trySave()
        tryVerify(() => !dialog.visible)
        tryVerify(() => app.actions.undo.text === "Undo keyword properties", 5000, app.actions.undo.text)

        dialog.openFor(rowOf("Redwood"), "Redwood")
        tryVerify(() => dialog.visible)
        compare(dialog.synonymsField.text, "Sequoia\nCoast redwood", "reopening shows what was saved")
        compare(dialog.doNotExportField.checked, true)
        dialog.close()

        undoNow()
        // The model refreshes on the history event asynchronously; wait for it before reading it back,
        // the same way tryItem does for any other property read right after an undo.
        tryVerify(() => JSON.parse(app.library.keywords.properties(rowOf("Redwood"))).synonyms === "", 5000)
        dialog.openFor(rowOf("Redwood"), "Redwood")
        tryVerify(() => dialog.visible)
        compare(dialog.synonymsField.text, "", "undo brings back the old ones")
        compare(dialog.doNotExportField.checked, false)
        dialog.close()
    }

    function test_a_synonym_containing_a_bar_is_refused() {
        make("Sea")
        const dialog = app.keywordPanel.propertiesDialog
        dialog.openFor(rowOf("Sea"), "Sea")
        dialog.synonymsField.text = "Ocean|Deep"
        dialog.trySave()
        compare(dialog.error, "A synonym cannot contain |.")
        verify(dialog.visible, "the dialog stays open")
    }

    function test_the_filter_label_follows_a_rename() {
        selectFirst(2)
        make("Old name")
        const id = idOf("Old name")
        app.photos.keywordSelection(id, true)
        tryVerify(() => item("Old name").photos === 2, 5000)
        app.library.filterKeyword(id, "Old name")
        tryCompare(app.photos, "count", 2)
        compare(app.library.keywordFilterName, "Old name")
        const dialog = app.keywordPanel.renameDialog
        dialog.openFor(rowOf("Old name"), "Old name")
        dialog.nameField.text = "New name"
        dialog.tryRename()
        tryVerify(() => app.library.keywordFilterName === "New name", 5000, app.library.keywordFilterName)
        compare(app.photos.count, 2, "the list is the same")
    }

    function test_the_list_does_not_move_when_the_line_about_new_keywords_appears() {
        make("Steady")
        const tree = app.keywordPanel.tree
        const before = tree.mapToItem(null, 0, 0).y
        const row = item("Steady")
        mouseClick(row, row.depth * 14 + 60, row.height / 2)
        tryVerify(() => app.keywordPanel.createUnderName === "Steady")
        wait(100)
        compare(tree.mapToItem(null, 0, 0).y, before, "the line has its place already")
        app.keywordPanel.createUnder = ""
        wait(100)
        compare(tree.mapToItem(null, 0, 0).y, before)
    }

    function test_the_context_menu_is_wide_enough_for_its_rows_the_first_time_it_opens() {
        make("Menu row")
        const menu = app.keywordPanel.contextMenu
        menu.row = rowOf("Menu row")
        menu.keywordId = idOf("Menu row")
        menu.keywordName = "Menu row"
        menu.popup()
        tryVerify(() => menu.visible)
        wait(100)
        for (let i = 0; i < menu.count; i++) {
            const it = menu.itemAt(i)
            verify(it.implicitWidth + menu.leftPadding + menu.rightPadding <= menu.width,
                   "row " + i + " (" + it.text + ") fits: " + it.implicitWidth + " in " + menu.width)
        }
        menu.close()
    }

    function test_the_labels_and_dialogs_speak_french() {
        selectFirst(2)
        const a = make("Faune")
        make("Oiseaux", a)
        make("Séquoia")
        app.launcher.chooseLanguage("fr")
        wait(250)

        const properties = app.keywordPanel.propertiesDialog
        properties.openFor(rowOf("Séquoia"), "Séquoia")
        tryVerify(() => properties.visible)
        compare(properties.title, "Propriétés du mot-clé")
        properties.synonymsField.text = "Coast redwood|x"
        properties.trySave()
        compare(properties.error, "Un synonyme ne peut pas contenir de |.")
        properties.close()

        const dialog = app.keywordPanel.deleteDialog
        dialog.openFor(a)
        tryVerify(() => dialog.visible)
        snapshot("keywords-delete-fr")
        dialog.confirm()
        tryVerify(() => rowOf("Faune") === -1, 5000)
        tryVerify(() => app.actions.undo.text === "Annuler la suppression de 2 mots-clés", 5000, app.actions.undo.text)
    }
}
