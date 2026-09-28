// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.ui

// The catalogue task: adding a folder as a source (and why it cannot be added, merging the sources
// inside it), scanning and rescanning it, removing it with the numbers at stake and getting it back,
// and the question about photos removed earlier. Each test is a new workspace on a machine of its
// own; the harness left folders of generated photos to copy from: Template2 (2), Template3 (3), One (1).
AppTestCase {
    name: "Catalogue"

    // A new workspace (it opens on the catalogue task) and a "Card" folder of the template's photos.
    function begin(template) {
        const machine = freshMachine()
        launch(machine)
        createWorkspace("Main")
        compare(app.currentTask, "catalogue", "a new workspace opens on the catalogue")
        compare(app.sources.count, 0)
        const card = machinePath(machine) + "/Card"
        files.copyDir(home + "/" + template, card)
        return card
    }

    function test_adding_a_folder_scans_it_and_lists_it_without_copying() {
        const card = begin("Template3")
        verify(app.catalogue.addButton.enabled)
        addSource(card)
        verify(!app.flow.addDialog.visible, "the dialog closes and the scan runs")
        waitForTheScan()
        compare(app.flow.status, "Done: 3 added, 0 restored, 0 already known, 0 not readable.")
        compare(app.sources.count, 1)
        const row = app.catalogue.list.itemAtIndex(0)
        compare(row.name, "Card")
        compare(row.photos, 3)
        verify(row.online)
        // Nothing was copied: the workspace's own folder holds no photo.
        verify(!files.exists(machinePath("") + "Pictures/Auroraw/Main/IMG_0000.jpg"))
        // The photos are in the grid.
        app.currentTask = "cull"
        tryCompare(app.photos, "count", 3)
        compare(app.library.status, "3 photos")
    }

    function test_the_add_source_dialog_remembers_the_last_folder_added() {
        // Issue #10: the folder field used to always start empty.
        const card = begin("Template2")
        addSource(card)
        waitForTheScan()

        click(app.catalogue.addButton)
        wait(150)
        compare(files.canonical(app.flow.addDialog.folderField.text), files.canonical(card))
        app.flow.addDialog.close()
    }

    function test_the_dialog_says_why_a_folder_cannot_be_added() {
        const card = begin("Template2")
        addSource(card)
        waitForTheScan()

        // Inside a source: already covered.
        files.mkdir(card + "/September")
        click(app.catalogue.addButton)
        wait(150)
        app.flow.addDialog.folderField.text = card + "/September"
        click(app.flow.addDialog.addButton)
        verify(app.flow.addDialog.error.indexOf("already part of") >= 0, app.flow.addDialog.error)
        verify(app.flow.addDialog.visible, "the dialog stays")

        // Not absolute: refused with the reason.
        app.flow.addDialog.folderField.text = "photos/relative"
        click(app.flow.addDialog.addButton)
        verify(app.flow.addDialog.error.indexOf("absolute") >= 0, app.flow.addDialog.error)
        compare(app.sources.count, 1, "nothing else was registered")
    }

    function test_a_folder_that_contains_a_source_asks_before_merging_it() {
        const machine = freshMachine()
        launch(machine)
        createWorkspace("Main")
        const parent = machinePath(machine) + "/Photos"
        files.copyDir(home + "/Template2", parent + "/Card")
        files.copyFile(home + "/One/NEW_0000.jpg", parent + "/top.jpg")
        addSource(parent + "/Card")
        waitForTheScan()

        click(app.catalogue.addButton)
        wait(150)
        app.flow.addDialog.folderField.text = parent
        click(app.flow.addDialog.addButton)
        verify(app.flow.addDialog.mergeQuestion.indexOf("\"Card\"") >= 0, app.flow.addDialog.mergeQuestion)
        compare(app.sources.count, 1, "nothing changed yet")
        verify(app.flow.addDialog.mergeButton.visible && !app.flow.addDialog.addButton.visible)

        click(app.flow.addDialog.mergeButton)
        waitForTheScan()
        compare(app.sources.count, 1, "the inner source was merged into the new one")
        const row = app.catalogue.list.itemAtIndex(0)
        compare(row.name, "Photos")
        compare(row.photos, 3)
    }

    // Removes the only source ("Card", of `photos` photos) through the dialog.
    function removeCard() {
        click(app.catalogue.list.itemAtIndex(0).removeButton)
        wait(150)
        verify(app.flow.removeDialog.visible)
        click(app.flow.removeDialog.removeButton)
        waitForTheScan()
    }

    function test_removing_a_source_confirms_with_the_numbers_and_adding_it_again_brings_the_photos_back() {
        const card = begin("Template2")
        addSource(card)
        waitForTheScan()

        // Rate one photo, so that there is work to lose.
        app.currentTask = "cull"
        tryCompare(app.photos, "count", 2)
        mouseClick(app.library.grid.itemAtIndex(0))
        wait(60)
        keyClick(Qt.Key_4)
        tryVerify(() => { app.library.reload(); return app.library.grid.itemAtIndex(0).rating === 4 }, 5000)
        wait(300)
        app.currentTask = "catalogue"

        click(app.catalogue.list.itemAtIndex(0).removeButton)
        wait(150)
        verify(app.flow.removeDialog.visible)
        compare(app.flow.removeDialog.photos, 2)
        compare(app.flow.removeDialog.workedOn, 1)
        compare(app.flow.removeDialog.title, "Remove the source \"Card\"?")
        snapshot("remove-source-dialog")
        click(app.flow.removeDialog.removeButton)
        waitForTheScan()
        compare(app.flow.status, "Source \"Card\" removed; 2 photos left the catalogue.")
        compare(app.sources.count, 0)
        verify(files.exists(card + "/IMG_0000.jpg"), "the originals are untouched")

        // Adding it again offers to bring the photos back, with their rating.
        addSource(card)
        tryVerify(() => app.flow.restoreDialog.visible, 20000, "the question")
        compare(app.flow.restoreDialog.restorable, 2)
        compare(app.flow.restoreDialog.total, 2)
        snapshot("restore-dialog")
        click(app.flow.restoreDialog.restoreButton)
        waitForTheScan()
        compare(app.flow.status, "Done: 0 added, 2 restored, 0 already known, 0 not readable.")
        compare(app.catalogue.list.itemAtIndex(0).photos, 2)
        app.currentTask = "cull"
        app.library.filterBy(4)
        tryCompare(app.photos, "count", 1)
    }

    function test_the_question_about_removed_photos_waits_for_the_dialog_that_is_open() {
        const card = begin("Template2")
        addSource(card)
        waitForTheScan()
        removeCard()

        // Add the folder again and, before any time passes, open Settings: the scan then asks whether
        // to restore, and that question does not replace the dialog that is open.
        click(app.catalogue.addButton)
        wait(150)
        app.flow.addDialog.folderField.text = card
        app.flow.tryAdd(false)
        app.showSettings()
        tryCompare(app.flow.restoreDialog, "restorable", 2)
        wait(300)
        verify(app.settingsDialog.visible)
        verify(!app.flow.restoreDialog.visible, "the question waits")
        app.settingsDialog.close()
        tryVerify(() => app.flow.restoreDialog.visible, 5000, "the question is put once the dialog is closed")
        click(app.flow.restoreDialog.restoreButton)
        waitForTheScan()
        compare(app.flow.status, "Done: 0 added, 2 restored, 0 already known, 0 not readable.")
    }

    function test_closing_the_question_with_escape_stops_the_scan() {
        const card = begin("Template2")
        addSource(card)
        waitForTheScan()
        removeCard()
        addSource(card)
        tryVerify(() => app.flow.restoreDialog.visible, 20000)
        keyClick(Qt.Key_Escape)
        waitForTheScan()
        compare(app.flow.status, "Scan cancelled. Rescanning the source picks up where it stopped.")
        compare(app.sources.count, 1, "the source stays; a rescan picks up")
    }

    function test_a_source_can_be_rescanned_to_pick_up_new_photos() {
        const card = begin("One")
        addSource(card)
        waitForTheScan()
        compare(app.catalogue.list.itemAtIndex(0).photos, 1)
        files.copyFile(home + "/Template2/IMG_0001.jpg", card + "/IMG_new.jpg")

        click(app.catalogue.list.itemAtIndex(0).rescanButton)
        waitForTheScan()
        compare(app.flow.status, "Done: 1 added, 0 restored, 1 already known, 0 not readable.")
        compare(app.catalogue.list.itemAtIndex(0).photos, 2)
    }

    function test_a_scan_in_progress_disables_the_buttons_and_the_tabs_wait_for_a_dialog() {
        const card = begin("Template3")
        click(app.catalogue.addButton)
        wait(150)
        verify(app.flow.dialogOpen)
        verify(!app.tabs.itemAt(1).enabled, "the task bar waits for the dialog")
        mouseClick(app.tabs.itemAt(1))
        compare(app.currentTask, "catalogue")
        snapshot("add-source-dialog")
        app.flow.addDialog.close()
        wait(200)
        verify(app.tabs.itemAt(1).enabled)
        snapshot("catalogue-empty")
        addSource(card)
        waitForTheScan()
        snapshot("catalogue-with-a-source")
    }

    function test_a_photo_with_no_raw_file_is_marked_in_the_grid() {
        // Issue #5: only the extension decides this, so a plain RAW file (its bytes are never read for it)
        // paired with a JPEG shows no badge, and a JPEG on its own does.
        const card = begin("One")
        files.write(card + "/IMG_pair.CR2", "raw bytes")
        files.copyFile(home + "/One/NEW_0000.jpg", card + "/IMG_pair.JPG")
        addSource(card)
        waitForTheScan()
        app.currentTask = "cull"
        tryCompare(app.photos, "count", 2)
        const flags = [app.library.grid.itemAtIndex(0), app.library.grid.itemAtIndex(1)]
            .map(item => item.isRawOriginal)
        verify(flags.includes(true), "the RAW+JPEG pair has no badge")
        verify(flags.includes(false), "the JPEG-only photo is marked")
    }

    function test_a_photo_whose_file_is_deleted_is_marked_missing_after_a_rescan() {
        // Issue #7: rescanning a source used not to detect a photo removed from it. It does now
        // (the engine's own tests cover the catalogue side); this is the grid saying so.
        const card = begin("Template3")
        addSource(card)
        waitForTheScan()
        files.remove(card + "/IMG_0001.jpg")

        click(app.catalogue.list.itemAtIndex(0).rescanButton)
        waitForTheScan()
        app.currentTask = "cull"
        tryCompare(app.photos, "count", 3)
        const missing = [0, 1, 2].map(i => app.library.grid.itemAtIndex(i).isMissing)
        compare(missing.filter(m => m).length, 1, "exactly the deleted photo is marked missing")
    }
}
