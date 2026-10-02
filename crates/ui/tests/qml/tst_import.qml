// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.ui

// Import (spec §5.2): copying the photos of a card or folder to another, verified, with what the
// destination is to the catalogue, the camera folders, the form remembered, cards that are inserted, and
// the folder pickers. Each test is a new workspace on a machine of its own; the harness left folders
// of generated photos to copy from: Template2 (2 photos), Template3 (3), Big (40), Cam100 and Cam101 (1).
AppTestCase {
    name: "Import"

    // The Import dialog of the window that is up.
    property var d: null

    function launchApp(machine) {
        launch(machine)
        d = app.importDialog
    }

    // The application on a machine whose workspace opens again, once it has: the screen is waited for, not read after a fixed
    // pause (issue #100).
    function reopenApp(machine) {
        launchOn(machine, "workspace")
        d = app.importDialog
    }

    // A new workspace, and a "Card" folder of the template's photos; `Archive` is where they go.
    function begin(template) {
        const machine = freshMachine()
        launchApp(machine)
        createWorkspace("Main")
        const card = machinePath(machine) + "/Card"
        files.copyDir(home + "/" + template, card)
        return { machine: machine, card: card, archive: machinePath(machine) + "/Archive" }
    }

    function openImport() {
        app.actions.importPhotos.trigger()
        tryVerify(() => d.visible, 3000, "the Import dialog opened")
    }

    function fill(m) {
        d.sourceField.text = m.card
        d.destinationField.text = m.archive
        d.fieldsChanged()
    }

    function waitForTheImport() {
        tryVerify(() => d.finished, 30000, "the import ended")
    }

    function test_filling_the_form_and_clicking_import_copies_verifies_and_shows_the_photos() {
        const m = begin("Template3")
        openImport()
        fill(m)
        snapshot("import-dialog")
        click(d.importButton)
        waitForTheImport()
        verify(!d.importing)
        compare(d.status, "All 3 files copied and verified.")
        for (let n = 0; n < 3; n++)
            verify(files.exists(m.archive + "/IMG_000" + n + ".jpg"), "IMG_000" + n + ".jpg reached the archive")

        // "Show photos" switches to the grid with what was just imported.
        verify(d.showPhotosButton.visible)
        click(d.showPhotosButton)
        verify(!d.visible)
        compare(app.currentTask, "cull")
        tryCompare(app.photos, "count", 3)
        compare(app.library.status, "3 photos")
        snapshot("import-done")
    }

    function test_a_workspace_that_is_empty_opens_on_what_fills_it_and_one_with_photos_on_the_grid() {
        const m = begin("Template2")
        compare(app.currentTask, "catalogue", "an empty workspace opens on what fills it")
        openImport()
        fill(m)
        click(d.importButton)
        waitForTheImport()
        waitForTheScan()
        // The next launch reopens it, on the grid, because it now has photos.
        reopenApp(m.machine)
        compare(app.currentTask, "cull")
        tryCompare(app.photos, "count", 2)
        compare(app.library.status, "2 photos")
    }

    function test_an_import_can_find_the_place_names_of_the_photos_it_registers() {
        const m = begin("TemplateGps")
        openImport()
        fill(m)
        verify(d.placesInstalled, "the places file is there")
        verify(d.findPlacesBox.enabled, "so the option can be turned on")
        verify(!d.findPlaces && !d.findPlacesBox.checked, "off until the person asks")
        click(d.findPlacesBox)
        verify(d.findPlaces)
        click(d.importButton)
        waitForTheImport()
        // The import said what it did, then the run of place names said what it found, for the two photos it registered.
        tryVerify(() => d.status.indexOf("Place names found for 2 photos.") >= 0, 30000, d.status)
        verify(d.status.indexOf("All 2 files copied and verified.") === 0, d.status)
        click(d.showPhotosButton)
        tryVerify(() => app.photos.count === 2, 10000)
        compare(app.photos.metadataAt(0, "city"), "Westville")
        compare(app.photos.metadataAt(0, "country"), "Aland")
    }

    function test_the_dialog_cannot_be_closed_while_an_import_runs() {
        const m = begin("Big")
        openImport()
        verify(d.closeButton.enabled)
        verify(!d.cancelButton.enabled, "there is nothing to cancel yet")
        verify(d.cancelButton.opacity < 1 && d.importButton.opacity === 1, "and it looks it: a disabled button is dimmed")
        snapshot("import-dialog-idle")
        fill(m)
        // Nothing here lets the event loop turn between the click and the checks, so the import is
        // still running.
        mouseClick(d.importButton)
        verify(d.importing)
        verify(!d.closeButton.enabled)
        // Issue #3: the fields cannot be changed while the import runs.
        verify(!d.sourceField.enabled)
        verify(!d.destinationField.enabled)
        verify(!d.backupField.enabled)
        keyClick(Qt.Key_Escape)
        verify(d.visible, "Escape does not close it either")
        verify(d.cancelButton.enabled, "the import can still be cancelled")
        waitForTheImport()
        verify(d.closeButton.enabled)
        verify(!d.cancelButton.enabled, "nor is there once it ended")
        click(d.closeButton)
        wait(150)
        verify(!d.visible)
    }

    function test_a_second_import_of_the_same_card_says_so_and_copies_nothing() {
        const m = begin("Template2")
        openImport()
        fill(m)
        click(d.importButton)
        waitForTheImport()
        click(d.importButton)
        tryVerify(() => d.finished && !d.importing && d.status.indexOf("0 copied") === 0, 30000)
        compare(d.status, "0 copied, 2 already in the library, 0 failed. Run it again to retry.")
    }

    function test_an_import_that_cannot_start_says_why_and_starts_nothing() {
        const m = begin("Template2")
        openImport()
        click(d.importButton)
        verify(!d.importing)
        verify(d.status.indexOf("Cannot start the import:") === 0, d.status)

        d.sourceField.text = m.card + "/nowhere-at-all"
        d.destinationField.text = m.archive
        click(d.importButton)
        verify(!d.importing)
        verify(d.status.indexOf("is not a folder") >= 0, d.status)
        verify(!files.exists(m.archive), "nothing was created for a refused import")
    }

    function test_the_form_is_remembered_the_next_time_the_workspace_opens() {
        const m = begin("Template2")
        openImport()
        fill(m)
        click(d.importButton)
        waitForTheImport()

        reopenApp(m.machine)
        compare(d.destinationField.text, files.canonical(m.archive))
        compare(d.sourceField.text, files.canonical(m.card))
    }

    function test_the_dialog_says_what_the_destination_is_to_the_catalogue() {
        const m = begin("Template2")
        openImport()
        d.sourceField.text = m.card
        // A folder that is not in the catalogue: a copy, unless it is added as a source (the default).
        d.destinationField.text = m.archive
        d.fieldsChanged()
        compare(d.kind, "not-covered")
        verify(d.addDestination)
        verify(d.registering)
        verify(d.destinationNote.indexOf("becomes a source") >= 0, d.destinationNote)
        verify(d.addDestinationBox.visible)

        d.addDestination = false
        d.fieldsChanged()
        verify(!d.registering)
        verify(d.destinationNote.indexOf("only copied") >= 0, d.destinationNote)

        // A folder inside a source of the catalogue: the photos enter it, whatever the box says.
        d.close()
        wait(150)
        const library = machinePath(m.machine) + "/Library"
        files.mkdir(library)
        addSource(library)
        waitForTheScan()
        openImport()
        d.destinationField.text = library + "/2026"
        d.fieldsChanged()
        compare(d.kind, "covered")
        verify(d.registering)
        verify(d.destinationNote.indexOf("part of the source") >= 0, d.destinationNote)
        verify(!d.addDestinationBox.visible)
    }

    function test_a_plain_copy_registers_nothing_and_shows_no_photos_to_go_to() {
        const m = begin("Template3")
        openImport()
        fill(m)
        d.addDestination = false
        d.fieldsChanged()
        click(d.importButton)
        waitForTheImport()
        compare(d.status, "All 3 files copied and verified.")
        verify(files.exists(m.archive + "/IMG_0002.jpg"))
        compare(app.sources.count, 0)
        verify(!d.showPhotosButton.visible, "a plain copy has no photos to show")
        app.library.reload()
        compare(app.photos.count, 0, "nothing entered the catalogue")
    }

    function test_importing_into_a_new_folder_can_add_it_to_the_catalogue_in_the_same_gesture() {
        const m = begin("Template2")
        openImport()
        fill(m)
        click(d.importButton)
        waitForTheImport()
        waitForTheScan()
        compare(app.sources.count, 1, "the destination became a source")
        app.currentTask = "catalogue"
        const row = app.catalogue.list.itemAtIndex(0)
        compare(row.name, "Archive")
        compare(row.photos, 2)
    }

    // A card's camera folders: DCIM/100CANON and DCIM/101CANON, one photo each (the case is kept).
    function cameraCard(m) {
        files.copyFile(home + "/Cam100/IMG_0000.jpg", m.card + "/DCIM/100CANON/IMG_0001.JPG")
        files.copyFile(home + "/Cam101/CAM_0000.jpg", m.card + "/DCIM/101CANON/IMG_0002.JPG")
    }

    function test_a_card_with_camera_folders_offers_to_keep_them() {
        const m = begin("Cam100")
        files.remove(m.card + "/IMG_0000.jpg")
        cameraCard(m)
        openImport()
        fill(m)
        d.addDestination = false
        d.fieldsChanged()
        verify(d.hasFolders)
        compare(d.folders, "100CANON, 101CANON")

        // The template is proposed; the card's folders are one click away.
        compare(d.layoutChoice, "template")
        click(d.foldersButton)
        compare(d.layoutChoice, "folders")
        click(d.importButton)
        waitForTheImport()
        verify(files.exists(m.archive + "/100CANON/IMG_0001.JPG"), "folders and case kept")
        verify(files.exists(m.archive + "/101CANON/IMG_0002.JPG"))
    }

    function test_the_layout_choice_is_always_offered_and_dcim_itself_counts_as_a_camera_card() {
        const m = begin("Cam100")
        files.remove(m.card + "/IMG_0000.jpg")
        cameraCard(m)
        openImport()
        // Nothing chosen yet: the choice is there all the same.
        verify(d.templateButton.visible && d.foldersButton.visible)
        // The card's root, its DCIM folder: the camera folders are noticed.
        for (const source of [m.card, m.card + "/DCIM"]) {
            d.sourceField.text = source
            d.fieldsChanged()
            verify(d.hasFolders, source)
            compare(d.folders, "100CANON, 101CANON")
        }
        // A folder with no camera layout still offers the choice, and no note.
        d.sourceField.text = m.card + "/DCIM/100CANON"
        d.fieldsChanged()
        verify(!d.hasFolders)
        verify(d.foldersButton.visible)
    }

    function test_keeping_the_folders_of_a_chosen_dcim_folder_copies_the_camera_folders() {
        const m = begin("Cam100")
        files.remove(m.card + "/IMG_0000.jpg")
        cameraCard(m)
        openImport()
        d.sourceField.text = m.card + "/DCIM"
        d.destinationField.text = m.archive
        d.addDestination = false
        d.fieldsChanged()
        click(d.foldersButton)
        click(d.importButton)
        waitForTheImport()
        verify(files.exists(m.archive + "/100CANON/IMG_0001.JPG"))
    }

    function test_the_labels_get_the_room_their_translation_needs() {
        begin("Template2")
        openImport()
        const left = field => field.mapToItem(null, 0, 0).x
        const en = left(d.backupField)
        compare(left(d.sourceField), en, "every field starts in the same column")
        compare(left(d.destinationField), en)
        useLanguage("fr")
        const fr = left(d.backupField)
        verify(fr > en, "the label column follows the widest label: " + fr + " in French, " + en + " in English")
        compare(left(d.sourceField), fr)
        compare(left(d.destinationField), fr)
    }

    // The volumes a person has plugged in, as the tests say them.
    function plug(cards) { app.importForm.volumesOverride = JSON.stringify(cards) }
    function camera(name, path) { return { name: name, path: path, hasDcim: true } }

    function test_a_card_inserted_while_the_application_runs_is_offered_for_import() {
        const m = begin("Template2")
        verify(!app.cardBanner.visible)
        plug([camera("EOS_DIGITAL", m.card)])
        tryVerify(() => app.cardBanner.visible, 8000, "the card was noticed")
        compare(app.cardBanner.cardName, "EOS_DIGITAL")
        snapshot("card-banner")

        // Import… opens the dialog on that card.
        wait(200) // the banner has taken its place in the layout
        click(app.cardBanner.importButton)
        tryVerify(() => d.visible, 3000)
        compare(d.sourceField.text, m.card)
        verify(!app.cardBanner.visible)
        d.close()
        wait(150)

        // The same card does not come back; another inserted later does, and Ignore dismisses it.
        wait(4500)
        verify(!app.cardBanner.visible)
        plug([camera("EOS_DIGITAL", m.card), camera("NIKON_D850", machinePath(m.machine) + "/Second")])
        tryVerify(() => app.cardBanner.visible, 8000, "the second card")
        compare(app.cardBanner.cardName, "NIKON_D850")
        click(app.cardBanner.ignoreButton)
        verify(!app.cardBanner.visible)
    }

    function test_a_card_that_was_already_in_when_the_workspace_opened_is_not_announced() {
        const machine = freshMachine()
        launchApp(machine)
        const card = machinePath(machine) + "/Card"
        plug([camera("EOS_DIGITAL", card)])
        createWorkspace("Main")
        wait(4500)
        verify(!app.cardBanner.visible)
        // It is listed in the dialog all the same.
        openImport()
        compare(d.volumes.length, 1)
        compare(d.sourceField.text, card, "a single card is offered")
    }

    function test_a_cards_banner_cannot_open_the_dialog_over_another_dialog() {
        const m = begin("Template2")
        app.showSettings()
        wait(150)
        plug([camera("EOS_DIGITAL", m.card)])
        tryVerify(() => app.cardBanner.visible, 8000)
        wait(200)
        verify(!app.cardBanner.importButton.enabled, "not over Settings")
        app.settingsDialog.close()
        wait(150)
        verify(app.cardBanner.importButton.enabled)
        wait(200)
        click(app.cardBanner.importButton)
        tryVerify(() => d.visible, 3000)
        compare(d.sourceField.text, m.card)
    }

    function test_the_folder_pickers_fill_their_field_and_open_where_it_points() {
        const m = begin("Template2")
        openImport()
        const existing = machinePath(m.machine) + "/Photos"
        files.mkdir(existing)

        // An empty field opens the system's default place; the answer fills the field.
        compare(d.destinationPicker.startPath(), "")
        d.destinationPicker.choose(m.archive)
        compare(d.destinationField.text, m.archive)
        compare(d.destinationPicker.title, "Choose the destination folder")

        // A field that already names a folder opens the dialog there; each field has its own title.
        d.sourceField.text = existing + "/2026/September/half-typed"
        compare(files.canonical(d.sourcePicker.startPath()), files.canonical(existing),
                "the folders that do not exist yet are skipped")
        compare(d.sourcePicker.title, "Choose the card or folder to import from")
        d.sourcePicker.choose(m.card)
        compare(d.sourceField.text, m.card)
        d.backupPicker.choose(existing)
        compare(d.backupField.text, existing)
        compare(d.backupPicker.title, "Choose the backup folder")

        // A dialog that is cancelled leaves the field alone.
        d.backupPicker.rejected()
        compare(d.backupField.text, existing)

        // Paths and URLs, the way a dialog gives and takes them.
        compare(d.sourcePicker.pathOf("file:///home/me/My%20Photos"), "/home/me/My Photos")
        compare(d.sourcePicker.pathOf("file:///C:/Users/me/Pictures"), "C:/Users/me/Pictures")
        compare(d.sourcePicker.urlFor("C:\\Users\\me\\My Photos"), "file:///C:/Users/me/My%20Photos")
        compare(d.sourcePicker.urlFor("/home/me/x"), "file:///home/me/x")
    }
}
