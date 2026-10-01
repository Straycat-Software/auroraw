// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.ui

// Offline place names (WP10, design note 008, D-147): Tools > Find place names… opens a dialog; the selected photos or a
// whole source get their city, region and country from where they were taken, a field a person typed stays, the whole run
// is one undoable step, the names are in the language of the interface, and a refresh follows the names Auroraw found
// earlier. The machine: 12 photos in "Card" with positions in their sidecars (`machine_with_places`), and a places file of
// "Aland" (two regions, a town in each) that `AURORAW_PLACES` points at. A test undoes what it did, since the workspace is
// shared by the tests.
AppTestCase {
    name: "Places"

    function init() {
        launchWithPhotos(12)
        app.currentTask = "cull"
    }

    function cleanup() {
        if (app) {
            app.launcher.chooseLanguage("en")
            if (app.placeNamesDialog.visible)
                app.placeNamesDialog.close()
        }
        quit()
    }

    function rowOf(name) {
        for (let row = 0; row < app.photos.count; row++)
            if (app.photos.summaryAt(row).indexOf(name) >= 0)
                return row
        return -1
    }

    // The value of a metadata field of the photo of `name`.
    function field(name, key) {
        return app.photos.metadataAt(rowOf(name), key)
    }

    // Selects the photos of `names` and only them.
    function selectOnly(...names) {
        app.photos.selectNone()
        for (const name of names)
            app.photos.toggle(rowOf(name))
        compare(app.photos.selectedCount, names.length)
    }

    function openDialog() {
        // (A dialog just closed is still on its way out: the command is not enabled until it has gone.)
        tryVerify(() => !app.placeNamesDialog.visible)
        app.actions.findPlaceNames.trigger()
        tryVerify(() => app.placeNamesDialog.opened)
        const dialog = app.placeNamesDialog
        drawn(dialog.contentItem)
        return dialog
    }

    // Clicks a button of the dialog once a frame with it in it has been drawn: Apply and Back have just appeared, and so has
    // Find after Back, and a click sent before the frame that shows them is lost (issues #27 and #63).
    function press(button) {
        drawn(button)
        click(button)
    }

    // Waits until the dialog is at `phase`, and says where it is, and what it reported, **as they are when the wait ends** (a
    // message handed to `tryVerify` is made before the wait, and says how things were at its start).
    function reach(dialog, phase) {
        const deadline = Date.now() + 30000
        while (dialog.phase !== phase && Date.now() < deadline)
            wait(20)
        compare(dialog.phase, phase)
    }

    // Presses Find and waits for the report.
    function runFind(dialog) {
        press(dialog.findButton)
        reach(dialog, "done")
    }

    // Asks for a refresh and waits for what it would do (nothing is written yet).
    function previewRefresh(dialog) {
        dialog.refreshBox.checked = true
        press(dialog.findButton)
        reach(dialog, "preview")
    }

    // Takes back what the runs of a test did.
    function undoAll(name, key) {
        for (let n = 0; n < 4 && field(name, key) !== ""; n++) {
            tryVerify(() => app.actions.undo.enabled)
            app.actions.undo.trigger()
            wait(150)
        }
    }

    function test_the_tools_menu_has_the_command_and_its_shortcut_opens_the_dialog() {
        verify(app.actions.findPlaceNames.enabled)
        compare(app.actions.findPlaceNames.shortcut.toString(), "Ctrl+Shift+L")
        // (A photo selected, as a person asking for place names has: the form the manual shows.)
        selectOnly("IMG_0000")
        keyClick(Qt.Key_L, Qt.ControlModifier | Qt.ShiftModifier)
        tryVerify(() => app.placeNamesDialog.visible)
        verify(!app.actions.findPlaceNames.enabled, "not over a dialog")
        verify(app.placeNamesDialog.phase === "form", "the places file is there: the form")
        snapshot("place-names")
    }

    function test_the_form_starts_on_the_selection_and_falls_back_to_the_source_without_one() {
        selectOnly("IMG_0000", "IMG_0001")
        let dialog = openDialog()
        verify(dialog.scopeSelection.checked && dialog.scopeSelection.enabled)
        compare(dialog.scopeSelection.text, "The 2 selected photos")
        verify(!dialog.refreshBox.checked, "a refresh is asked for each time, never remembered")
        verify(dialog.findButton.enabled)
        dialog.close()
        app.photos.selectNone()
        dialog = openDialog()
        verify(!dialog.scopeSelection.enabled)
        verify(dialog.scopeSource.checked)
        compare(dialog.sourceBox.currentText, "Card")
        verify(dialog.findButton.enabled, "a whole source can be asked for")
        dialog.close()
    }

    function test_the_places_of_the_selected_photos_are_filled_what_a_person_typed_stays_and_it_is_one_undo() {
        selectOnly("IMG_0000", "IMG_0001", "IMG_0002", "IMG_0003", "IMG_0004", "IMG_0005")
        const dialog = openDialog()
        runFind(dialog)
        // Six looked at: four get names (IMG_0004 has a city of its own and gets the rest), one is in open water, one has no
        // position.
        compare(dialog.report.photos, 6)
        compare(dialog.report.filled, 4)
        compare(dialog.report.noPosition, 1)
        compare(dialog.report.openWater, 1)
        compare(dialog.report.hadPlace, 0)
        compare(dialog.report.failed, 0)
        compare(dialog.lines.length, 3, "what was found, no position, open water: nothing went wrong")
        verify(dialog.lines.every(line => !line.attention))
        compare(field("IMG_0000", "city"), "Westville")
        compare(field("IMG_0000", "region"), "West")
        compare(field("IMG_0000", "country"), "Aland")
        compare(field("IMG_0000", "country-code"), "AA")
        compare(field("IMG_0002", "city"), "Eastburg")
        compare(field("IMG_0002", "region"), "East")
        compare(field("IMG_0004", "city"), "Mine", "a city a person typed is never changed")
        compare(field("IMG_0004", "region"), "West")
        compare(field("IMG_0003", "country"), "", "open water has no country")
        compare(field("IMG_0005", "country"), "", "no position, no place")
        snapshot("place-names-done")
        dialog.close()
        // One step: a single Undo takes the names back from every photo, and the typed city stays.
        tryVerify(() => app.actions.undo.enabled)
        verify(app.actions.undo.text.indexOf("place names") > 0, app.actions.undo.text)
        app.actions.undo.trigger()
        tryCompare(app.photos, "count", 12)
        tryVerify(() => field("IMG_0000", "country") === "", 5000, "undone for the first photo")
        compare(field("IMG_0002", "city"), "")
        compare(field("IMG_0004", "region"), "")
        compare(field("IMG_0004", "city"), "Mine")
    }

    function test_a_source_run_looks_at_every_photo_and_a_second_one_finds_them_already_done() {
        app.photos.selectNone()
        let dialog = openDialog()
        runFind(dialog)
        const r = dialog.report
        compare(r.photos, 12)
        compare(r.filled + r.hadPlace + r.noPosition + r.openWater + r.failed, 12, "each photo is counted once")
        verify(r.filled >= 7, "the seven photos with a position in Aland get names: " + JSON.stringify(r))
        compare(r.openWater, 1)
        dialog.close()
        dialog = openDialog()
        runFind(dialog)
        compare(dialog.report.filled, 0, "nothing is looked up again that is filled")
        verify(dialog.report.hadPlace >= 7, JSON.stringify(dialog.report))
        dialog.close()
        undoAll("IMG_0000", "country")
        compare(field("IMG_0000", "country"), "")
    }

    function test_the_names_are_in_the_interfaces_language_and_a_refresh_follows_them() {
        app.launcher.chooseLanguage("fr")
        wait(250)
        selectOnly("IMG_0006")
        let dialog = openDialog()
        runFind(dialog)
        compare(field("IMG_0006", "region"), "Ouest")
        compare(field("IMG_0006", "country"), "Alandie")
        compare(field("IMG_0006", "city"), "Westville", "the city is as the place spells it")
        dialog.close()
        // Without a refresh, what Auroraw found is left as it is, in whatever language it was found.
        app.launcher.chooseLanguage("en")
        wait(250)
        selectOnly("IMG_0006")
        dialog = openDialog()
        runFind(dialog)
        compare(field("IMG_0006", "region"), "Ouest")
        dialog.close()
        // With one it follows (the language is a reason: a name found by Auroraw is Auroraw's to update), once it has been
        // shown and applied.
        dialog = openDialog()
        previewRefresh(dialog)
        compare(field("IMG_0006", "region"), "Ouest", "shown, not done")
        press(dialog.applyButton)
        reach(dialog, "done")
        compare(field("IMG_0006", "region"), "West")
        compare(field("IMG_0006", "country"), "Aland")
        dialog.close()
        undoAll("IMG_0006", "country")
        compare(field("IMG_0006", "country"), "")
    }

    function test_a_refresh_is_shown_grouped_before_it_is_done_and_applied_on_request() {
        const six = ["IMG_0000", "IMG_0001", "IMG_0002", "IMG_0006", "IMG_0007", "IMG_0008"]
        app.launcher.chooseLanguage("fr")
        wait(250)
        selectOnly(...six)
        let dialog = openDialog()
        runFind(dialog)
        compare(dialog.report.filled, 6, "a first fill goes straight to the run: there is nothing to decide")
        dialog.close()
        app.launcher.chooseLanguage("en")
        wait(250)
        selectOnly(...six)
        dialog = openDialog()
        previewRefresh(dialog)
        // What it would do, grouped, the biggest first: the country's name in all six, the region's in four and two. The
        // cities and the code are the same words in both languages: not changes.
        compare(dialog.preview.report.filled, 6)
        compare(dialog.preview.groups.length, 3)
        compare(dialog.preview.groupsTotal, 3)
        compare(dialog.changeText(dialog.preview.groups[0]), "Country: Alandie → Aland, 6 photos")
        compare(dialog.changeText(dialog.preview.groups[1]), "Region: Ouest → West, 4 photos")
        compare(dialog.changeText(dialog.preview.groups[2]), "Region: Est → East, 2 photos")
        compare(dialog.preview.groups[0].examples.length, 3, "a few of the six, to say what it is about")
        verify(dialog.applyButton.visible && dialog.backButton.visible && dialog.closeButton.visible)
        verify(!dialog.findButton.visible && !dialog.stopButton.visible)
        compare(dialog.closeButton.text, "Cancel")
        compare(dialog.previewTitle.text, "The refresh would change 6 photos.")
        // Nothing was written, and nothing is a step of the history yet.
        compare(field("IMG_0000", "country"), "Alandie")
        compare(field("IMG_0002", "region"), "Est")
        drawn(dialog.contentItem)
        snapshot("place-names-preview")
        // Back leaves it undone, and the form is as it was.
        press(dialog.backButton)
        compare(dialog.phase, "form")
        verify(dialog.refreshBox.checked)
        compare(field("IMG_0000", "country"), "Alandie")
        // Apply makes the changes, and the usual report follows.
        previewRefresh(dialog)
        press(dialog.applyButton)
        reach(dialog, "done")
        compare(dialog.report.filled, 6)
        compare(field("IMG_0000", "country"), "Aland")
        compare(field("IMG_0002", "region"), "East")
        dialog.close()
        undoAll("IMG_0000", "country")
        compare(field("IMG_0000", "country"), "")
    }

    function test_a_refresh_that_would_change_nothing_says_so_and_has_nothing_to_apply() {
        selectOnly("IMG_0007")
        let dialog = openDialog()
        runFind(dialog)
        dialog.close()
        selectOnly("IMG_0007")
        dialog = openDialog()
        previewRefresh(dialog)
        compare(dialog.preview.report.filled, 0)
        compare(dialog.preview.groups.length, 0)
        compare(dialog.previewTitle.text, "The refresh would change nothing.")
        verify(!dialog.applyButton.visible, "nothing to apply")
        verify(dialog.backButton.visible)
        press(dialog.backButton)
        compare(dialog.phase, "form")
        dialog.close()
        undoAll("IMG_0007", "country")
        compare(field("IMG_0007", "country"), "")
    }

    function test_the_progress_says_how_far_it_is() {
        selectOnly("IMG_0007")
        const dialog = openDialog()
        // (A run on one photo is over before a test can look: the bus is told what a job would say.)
        dialog.phase = "running"
        dialog.job = "a-job"
        Bus.jobProgress("a-job", 3, 12)
        compare(dialog.done, 3)
        compare(dialog.total, 12)
        compare(dialog.share, 0.25)
        compare(dialog.progressBar.Accessible.name, "Place names progress")
        verify(dialog.stopButton.visible && !dialog.closeButton.visible, "only Stop while it runs")
        dialog.phase = "form"
        dialog.job = ""
    }

    function test_the_refresh_has_its_consequence_for_a_screen_reader() {
        selectOnly("IMG_0007")
        const dialog = openDialog()
        compare(dialog.refreshBox.Accessible.description, dialog.refreshHint)
        verify(dialog.refreshHint.indexOf("left as it is") > 0)
        dialog.close()
    }

    function test_the_about_window_credits_the_data() {
        app.actions.about.trigger()
        tryVerify(() => app.aboutDialog.visible)
        verify(app.aboutDialog.placesLabel.visible, "the places file is installed")
        verify(app.aboutDialog.placesLabel.text.indexOf("GeoNames") >= 0)
        verify(app.aboutDialog.placesLabel.text.indexOf("Creative Commons Attribution 4.0") >= 0)
        verify(app.aboutDialog.placesLabel.text.indexOf("https://creativecommons.org/licenses/by/4.0/") >= 0, "a link to the licence")
        verify(app.aboutDialog.placesLabel.text.indexOf("adapted from GeoNames") >= 0, "and that the data was modified")
        verify(app.aboutDialog.placesLabel.text.indexOf("Natural Earth") >= 0)
        verify(app.aboutDialog.placesLabel.text.indexOf("default worldview") >= 0)
        app.aboutDialog.close()
    }

    function test_the_dialog_speaks_french() {
        app.launcher.chooseLanguage("fr")
        wait(250)
        selectOnly("IMG_0008")
        const dialog = openDialog()
        compare(dialog.title, "Trouver les noms de lieux")
        compare(dialog.findButton.text, "Trouver")
        snapshot("place-names-fr")
        runFind(dialog)
        compare(field("IMG_0008", "country"), "Alandie")
        snapshot("place-names-fr-done")
        dialog.close()
        undoAll("IMG_0008", "country")
    }
}
