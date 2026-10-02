// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.ui

// The collections tab (WP10, slice 3): a collection made from the field with the selection in it, the tri-state
// check (none, some, all) and a click putting photos in or taking them out, undo and redo with the tab following,
// collections inside collections and folding them, the list filtered by a collection and by the ones inside it,
// rename, the Move dialog, drag and drop, deleting a branch with the numbers said and taking it back with
// Ctrl+Z, what is refused, and French. The 40-photo machine, shared by the tests: each one uses names of its
// own, and empties what it filled.
AppTestCase {
    name: "Collections"

    property var grid: null

    function init() {
        launchWithPhotos(40)
        grid = app.library.grid
        app.keywordPanel.tabs.currentIndex = 3
    }

    // Every photo is taken out of every collection, the list is whole again and the language put back (the
    // collections themselves stay).
    function cleanup() {
        if (app) {
            app.launcher.chooseLanguage("en")
            app.library.filterCollection("", "")
            app.keywordPanel.collectionPanel.filterField.text = ""
            app.library.selectAll()
            for (let i = 0; i < collections().count; i++)
                app.photos.collectionSelection(collections().idAt(i), false)
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

    // The first `count` photos, selected.
    function selectFirst(count) {
        click(0)
        if (count > 1)
            click(count - 1, Qt.ShiftModifier)
        compare(app.photos.selectedCount, count)
    }

    function collections() { return app.library.collections }
    function panel() { return app.keywordPanel.collectionPanel }

    // Types in the tab's field and presses Enter (Shift when asked).
    function typeCollection(text, modifiers) {
        panel().filterField.forceActiveFocus()
        tryVerify(() => panel().filterField.activeFocus)
        panel().filterField.text = text
        keyClick(Qt.Key_Return, modifiers === undefined ? Qt.NoModifier : modifiers)
        wait(150)
    }

    function rowOf(name) {
        for (let i = 0; i < collections().count; i++)
            if (collections().nameAt(i) === name)
                return i
        return -1
    }

    function idOf(name) {
        const row = rowOf(name)
        verify(row >= 0, name + " is listed")
        return collections().idAt(row)
    }

    function itemAtRow(row) {
        const tree = panel().tree
        tree.positionViewAtIndex(row, ListView.Contain)
        wait(30)
        const it = tree.itemAtIndex(row)
        verify(it, "the row " + row + " exists")
        return it
    }

    function item(name) {
        const row = rowOf(name)
        verify(row >= 0, "the collection " + name + " is listed")
        return itemAtRow(row)
    }

    // Waits until a row of the tree has `value` for `property`. (Asks again every time: the list rebuilds its
    // rows when a collection moves, comes back or goes, so a row found earlier may be gone.)
    function tryItem(name, property, value) {
        tryVerify(() => {
            const row = rowOf(name)
            if (row < 0)
                return false
            panel().tree.positionViewAtIndex(row, ListView.Contain)
            const it = panel().tree.itemAtIndex(row)
            return it !== null && it !== undefined && it[property] === value
        }, 5000, name + "." + property + " never became " + value)
    }

    // Makes a collection (no photo needed) and lists it.
    function make(name, parent) {
        const id = collections().create(name, parent === undefined ? "" : parent)
        verify(id.indexOf("error:") !== 0, id)
        return id
    }

    function undoNow() {
        wait(300)
        grid.forceActiveFocus()
        keyClick(Qt.Key_Z, Qt.ControlModifier)
    }

    // Drags the row `from` onto `target`, an item with coordinates.
    function drag(from, target, tx, ty) {
        const x = from.depth * 14 + 60
        mousePress(from, x, from.height / 2)
        mouseMove(from, x + 12, from.height / 2 + 2)
        mouseMove(from, x + 30, from.height / 2 + 4)
        wait(50)
        mouseMove(target, tx, ty)
        wait(50)
        mouseMove(target, tx + 1, ty)
        wait(50)
        mouseRelease(target, tx + 1, ty)
        wait(150)
    }

    // Every tab is whole inside the bar and none overlaps the next; `cut` says whether a label may be elided.
    function checkTabs(cut) {
        const tabs = app.keywordPanel.tabs
        let right = 0
        for (let i = 0; i < 4; i++) {
            const tab = tabs.itemAt(i)
            verify(tab.x >= right - 1, tab.text + " starts at " + tab.x + " after the last ended at " + right)
            right = tab.x + tab.width
            if (!cut)
                verify(!tab.contentItem.truncated, tab.text + " is cut short at " + tab.width)
        }
        verify(right <= tabs.width + 1, "the last tab ends at " + right + " of " + tabs.width)
    }

    function test_the_four_tabs_fit_in_the_panel_in_both_languages_and_when_it_is_narrowed() {
        const tabs = app.keywordPanel.tabs
        compare(tabs.itemAt(3).text, "Collections")
        checkTabs(false)
        useLanguage("fr")
        compare(tabs.itemAt(1).text, "Métadonnées")
        checkTabs(false)
        app.keywordPanel.panelWidth = app.keywordPanel.minimumWidth
        wait(100)
        checkTabs(true)
        app.keywordPanel.panelWidth = app.keywordPanel.defaultWidth
    }

    function test_a_collection_typed_in_the_field_is_made_with_the_selection_in_it() {
        selectFirst(3)
        typeCollection("Portfolio")
        tryItem("Portfolio", "held", 2)
        compare(item("Portfolio").photos, 3)
        compare(panel().filterField.text, "", "the field is emptied")
        tryVerify(() => app.actions.undo.text === "Undo creating the collection", 5000, app.actions.undo.text)
    }

    function test_typing_a_name_that_exists_puts_the_selection_in_it_and_does_not_make_another() {
        selectFirst(2)
        typeCollection("Trip")
        const before = collections().count
        click(5)
        typeCollection("trip")
        compare(collections().count, before, "the same collection")
        tryItem("Trip", "photos", 3)
        tryVerify(() => app.actions.undo.text === "Undo adding 1 photo to a collection", 5000, app.actions.undo.text)
    }

    function test_the_check_says_none_some_or_all_and_a_click_puts_in_or_takes_out() {
        selectFirst(3)
        typeCollection("Tri")
        tryItem("Tri", "held", 2)
        selectFirst(5)
        tryItem("Tri", "held", 1) // some of the five
        compare(item("Tri").photos, 3)
        const check = item("Tri")
        mouseClick(check, check.depth * 14 + 16 + 2 + 8, check.height / 2)
        tryItem("Tri", "held", 2)
        compare(item("Tri").photos, 5)
        const again = item("Tri")
        mouseClick(again, again.depth * 14 + 16 + 2 + 8, again.height / 2)
        tryItem("Tri", "held", 0)
        compare(item("Tri").photos, 0)
        tryVerify(() => app.actions.undo.text === "Undo taking 5 photos out of a collection", 5000, app.actions.undo.text)
    }

    function test_undoing_the_making_of_a_collection_takes_it_and_its_photos_back() {
        selectFirst(3)
        typeCollection("Undoable")
        tryItem("Undoable", "held", 2)
        wait(300)
        keyClick(Qt.Key_Escape)
        verify(app.library.grid.activeFocus, "Escape gives the keyboard back to the grid")
        keyClick(Qt.Key_Z, Qt.ControlModifier)
        tryVerify(() => rowOf("Undoable") === -1, 5000, "the collection is gone from the tab")
        compare(app.photos.selectedCount, 3, "the selection stays")
        tryVerify(() => app.actions.redo.text === "Redo creating the collection", 5000, app.actions.redo.text)
        keyClick(Qt.Key_Y, Qt.ControlModifier)
        tryVerify(() => rowOf("Undoable") >= 0, 5000)
        tryItem("Undoable", "held", 2)
        compare(item("Undoable").photos, 3)
    }

    function test_a_collection_made_after_a_click_goes_inside_that_collection_and_can_be_folded() {
        selectFirst(2)
        typeCollection("Albums")
        const albums = item("Albums")
        mouseClick(albums, 100, albums.height / 2)
        compare(panel().createUnderName, "Albums")
        typeCollection("Summer")
        tryVerify(() => rowOf("Summer") === rowOf("Albums") + 1)
        compare(item("Summer").depth, 1)
        compare(item("Albums").hasChildren, true)
        collections().toggleExpanded(rowOf("Albums"))
        compare(rowOf("Summer"), -1, "the child is out of sight when its parent is folded")
        collections().toggleExpanded(rowOf("Albums"))
        verify(rowOf("Summer") > 0)
        panel().createUnder = ""
    }

    function test_the_field_filters_the_tree_and_keeps_the_ancestors() {
        const europe = make("Europe")
        make("Paris", europe)
        make("Rome", europe)
        make("Asia")
        panel().filterField.text = "par"
        tryVerify(() => rowOf("Paris") >= 0 && rowOf("Europe") >= 0, 5000)
        compare(rowOf("Rome"), -1)
        compare(rowOf("Asia"), -1)
        panel().filterField.text = ""
        tryVerify(() => rowOf("Rome") >= 0 && rowOf("Asia") >= 0, 5000)
    }

    function test_the_list_can_be_filtered_by_a_collection_and_by_the_ones_inside_it() {
        selectFirst(4)
        typeCollection("Shots")
        tryItem("Shots", "photos", 4)
        const shots = idOf("Shots")
        const inner = make("Inner shots", shots)
        click(9)
        app.photos.collectionSelection(inner, true)
        tryItem("Inner shots", "photos", 1)

        app.library.filterCollection(shots, "Shots")
        tryCompare(app.photos, "count", 5, 20000, "the collection's own four and the one inside it")
        compare(app.library.collectionFilterName, "Shots")
        compare(app.photos.selectedCount, 0, "a change of list lets go of the selection")
        app.library.filterCollection(inner, "Inner shots")
        tryCompare(app.photos, "count", 1, 20000)
        app.library.filterCollection("", "")
        tryCompare(app.photos, "count", 40, 20000)
    }

    function test_a_collection_is_renamed_and_the_filter_label_follows() {
        selectFirst(2)
        typeCollection("Old name")
        const id = idOf("Old name")
        tryItem("Old name", "photos", 2)
        app.library.filterCollection(id, "Old name")
        tryCompare(app.photos, "count", 2)
        const dialog = panel().renameDialog
        dialog.openFor(rowOf("Old name"), "Old name")
        tryVerify(() => dialog.visible)
        verify(app.dialogOpen, "the window's commands wait")
        dialog.nameField.text = "New name"
        dialog.tryRename()
        tryVerify(() => rowOf("New name") >= 0, 5000)
        tryVerify(() => app.library.collectionFilterName === "New name", 5000, app.library.collectionFilterName)
        compare(app.photos.count, 2, "the list is the same")
        tryVerify(() => app.actions.undo.text === "Undo renaming the collection", 5000, app.actions.undo.text)
        undoNow()
        tryVerify(() => rowOf("Old name") >= 0 && rowOf("New name") === -1, 5000)
    }

    function test_a_name_a_sibling_has_and_a_move_under_itself_are_refused_and_say_why() {
        const a = make("Alpha")
        make("Beta", a)
        make("Gamma")
        const dialog = panel().renameDialog
        dialog.openFor(rowOf("Gamma"), "Gamma")
        dialog.nameField.text = "alpha"
        dialog.tryRename()
        compare(dialog.error, "There is already a collection named “alpha” there.")
        verify(dialog.visible, "the dialog stays open")
        dialog.nameField.text = "  "
        dialog.tryRename()
        compare(dialog.error, "A collection needs a name.")
        dialog.close()
        const target = item("Beta")
        drag(item("Alpha"), target, 100, target.height / 2)
        compare(item("Alpha").depth, 0, "a collection does not go inside its own")
        compare(panel().note, "A collection cannot be moved under itself or under one of its own collections.")
    }

    function test_a_collection_dragged_onto_another_becomes_its_child_and_undo_puts_it_back() {
        make("Places")
        make("Peru")
        compare(item("Peru").depth, 0)
        const places = item("Places")
        drag(item("Peru"), places, 100, places.height / 2)
        tryItem("Peru", "depth", 1)
        compare(rowOf("Peru"), rowOf("Places") + 1)
        tryVerify(() => app.actions.undo.text === "Undo moving the collection", 5000, app.actions.undo.text)
        undoNow()
        tryItem("Peru", "depth", 0)
        tryVerify(() => app.actions.redo.text === "Redo moving the collection")
    }

    function test_a_collection_dropped_where_there_is_no_collection_goes_to_the_top_level() {
        const continent = make("Continent")
        make("Chile", continent)
        compare(item("Chile").depth, 1)
        const from = item("Chile")
        const x = from.depth * 14 + 60
        mousePress(from, x, from.height / 2)
        mouseMove(from, x + 12, from.height / 2)
        mouseMove(from, x + 30, from.height / 2 + 4)
        tryVerify(() => panel().dragging)
        // Under the last row of the list: no collection there.
        const tab = panel()
        mouseMove(tab, tab.width / 2, tab.height - 8)
        wait(50)
        mouseMove(tab, tab.width / 2 + 1, tab.height - 8)
        tryVerify(() => tab.topLevelDrop.containsDrag && tab.topLevelDrop.allowed)
        mouseRelease(tab, tab.width / 2 + 1, tab.height - 8)
        tryItem("Chile", "depth", 0)
    }

    function test_the_move_dialog_lists_where_a_collection_can_go() {
        make("Holder")
        make("Movable")
        make("Inside", idOf("Movable"))
        const dialog = panel().moveDialog
        dialog.openFor(idOf("Movable"), "Movable")
        tryVerify(() => dialog.visible)
        verify(app.dialogOpen, "the window's commands wait")
        const paths = dialog.targets.map(t => t.path)
        verify(paths.indexOf("Holder") >= 0)
        compare(paths.indexOf("Movable"), -1, "not inside itself")
        compare(paths.indexOf("Movable › Inside"), -1, "nor inside its own")
        dialog.targetBox.currentIndex = paths.indexOf("Holder")
        dialog.tryMove()
        tryVerify(() => !dialog.visible)
        tryItem("Movable", "depth", 1)
        compare(rowOf("Movable"), rowOf("Holder") + 1)
        compare(item("Inside").depth, 2, "with what is inside it")
    }

    function test_deleting_a_branch_says_what_it_takes_and_ctrl_z_brings_everything_back() {
        selectFirst(4)
        const folder = make("Folder")
        const child = make("Child", folder)
        app.photos.collectionSelection(child, true)
        click(9)
        app.photos.collectionSelection(folder, true)
        tryVerify(() => item("Child").photos === 4 && item("Folder").photos === 1, 5000)

        const dialog = panel().deleteDialog
        dialog.openFor(folder)
        tryVerify(() => dialog.visible)
        compare(dialog.branchCollections, 2)
        compare(dialog.branchPhotos, 5)
        verify(app.dialogOpen)
        snapshot("collections-delete-en")
        dialog.confirm()
        tryVerify(() => !dialog.visible)
        tryVerify(() => rowOf("Folder") === -1 && rowOf("Child") === -1, 5000)
        tryVerify(() => app.actions.undo.text === "Undo deleting 2 collections", 5000, app.actions.undo.text)
        compare(app.photos.count, 40, "the photos themselves are all still there")

        undoNow()
        tryVerify(() => rowOf("Child") >= 0 && rowOf("Folder") >= 0, 5000)
        compare(item("Child").depth, 1)
        tryItem("Child", "photos", 4)
        tryItem("Folder", "photos", 1)
        tryVerify(() => app.actions.redo.text === "Redo deleting 2 collections")
        wait(300)
        keyClick(Qt.Key_Y, Qt.ControlModifier)
        tryVerify(() => rowOf("Folder") === -1, 5000)
    }

    function test_the_list_filtered_by_a_deleted_collection_shows_everything_again() {
        selectFirst(3)
        const gone = make("Gone")
        app.photos.collectionSelection(gone, true)
        tryVerify(() => item("Gone").photos === 3, 5000)
        app.library.filterCollection(gone, "Gone")
        tryCompare(app.photos, "count", 3)
        const dialog = panel().deleteDialog
        dialog.openFor(gone)
        dialog.confirm()
        tryCompare(app.photos, "count", 40, 20000)
        compare(app.photos.collectionFilter, "")
    }

    // What the manual shows: a folder with two collections in it, and one beside it, the selection in one of them.
    // (Names of their own, as every test here has: the other tests' collections stay in the machine.)
    function test_the_tab_as_the_manual_shows_it() {
        selectFirst(6)
        const weddings = make("2026 weddings")
        const marie = make("Marie and Paul", weddings)
        make("Anna and Léo", weddings)
        const travel = make("Peru, July 2026")
        app.photos.collectionSelection(marie, true)
        tryItem("Marie and Paul", "photos", 6)
        selectFirst(3)
        app.photos.collectionSelection(travel, true)
        tryItem("Peru, July 2026", "photos", 3)
        app.photos.collectionSelection(weddings, true)
        tryItem("2026 weddings", "photos", 3)
        selectFirst(4)
        tryItem("Marie and Paul", "held", 2)
        tryItem("Peru, July 2026", "held", 1)
        tryItem("2026 weddings", "held", 1)
        snapshot("collections-en")
    }

    function test_the_tab_and_the_dialogs_speak_french() {
        selectFirst(2)
        const a = make("Faune")
        make("Oiseaux", a)
        useLanguage("fr")
        compare(app.keywordPanel.tabs.itemAt(3).text, "Collections")
        const dialog = panel().deleteDialog
        dialog.openFor(a)
        tryVerify(() => dialog.visible)
        snapshot("collections-delete-fr")
        dialog.confirm()
        tryVerify(() => rowOf("Faune") === -1, 5000)
        tryVerify(() => app.actions.undo.text === "Annuler la suppression de 2 collections", 5000, app.actions.undo.text)
        compare(panel().explain("name"), "Une collection doit avoir un nom.")
    }
}
