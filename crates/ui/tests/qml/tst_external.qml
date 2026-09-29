// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.ui

// The changes other applications make to the XMP files beside originals (WP10, spec §5.7, D-047): a rescan that
// finds one shows a banner ("N photos have metadata changed by another application"); Review… lists the photos;
// Accept all applies them as one undoable step, Ignore declines them, and neither is reported again. The machine:
// 20 photos in "Card"; every test uses photos of its own and answers what it raised, since the machine is shared.
AppTestCase {
    name: "External"

    function init() {
        launchWithPhotos(20)
    }

    function cleanup() {
        if (app) {
            app.launcher.chooseLanguage("en")
            if (app.externalDialog.visible)
                app.externalDialog.close()
            app.externalChanges.ignoreAll()
            tryVerify(() => app.externalBanner.count === 0)
        }
        quit()
    }

    // An XMP file as another application would write it: a title, and a rating when there is one.
    function xmp(title, rating) {
        const stars = rating === undefined ? "" : ' xmp:Rating="' + rating + '"'
        return '<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">'
            + '<rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmlns:dc="http://purl.org/dc/elements/1.1/"'
            + stars + '><dc:title><rdf:Alt><rdf:li xml:lang="x-default">' + title + '</rdf:li></rdf:Alt></dc:title>'
            + '</rdf:Description></rdf:RDF></x:xmpmeta>'
    }

    function rescan() {
        app.currentTask = "catalogue"
        click(app.catalogue.list.itemAtIndex(0).rescanButton)
        waitForTheScan()
    }

    function rowOfPhoto(name) {
        app.currentTask = "cull"
        const count = app.photos.count
        for (let row = 0; row < count; row++) {
            if (app.photos.summaryAt(row).indexOf(name) >= 0)
                return row
        }
        return -1
    }

    // Writes `name`.xmp beside its original and lets a rescan see it, as another application's edit.
    function edit(name, title, rating) {
        files.write(home + "/Card/" + name + ".xmp", xmp(title, rating))
        rescan()
    }

    // (Diagnostic, Windows CI: what the review button and the window look like around the click.)
    function diag(label) {
        const b = app.externalBanner.reviewButton
        const c = b.mapToItem(null, b.width / 2, b.height / 2)
        return ("DIAG " + label + ": review enabled=" + b.enabled + " visible=" + b.visible
            + " size=" + b.width + "x" + b.height + " centre=" + Math.round(c.x) + "," + Math.round(c.y)
            + " window=" + app.width + "x" + app.height + " banner y=" + app.externalBanner.y
            + " h=" + app.externalBanner.height + " dialogOpen=" + app.dialogOpen
            + " nativeDialogOpen=" + app.nativeDialogOpen
            + " externalDialog.visible=" + app.externalDialog.visible + " visibility=" + app.externalDialog.visibility
            + " appState=" + Qt.application.state + " active=" + app.active)
    }

    function test_a_change_shows_the_banner_then_review_and_accept_all_applies_it_as_one_step() {
        // First sight: remembered, nothing said, nothing applied.
        edit("IMG_0003", "One")
        verify(!app.externalBanner.visible)
        // Then another application rates the photo.
        edit("IMG_0003", "One", 4)
        tryVerify(() => app.externalBanner.visible)
        compare(app.externalBanner.count, 1)

        const t0 = Date.now()
        const lines = []
        const say = text => lines.push((Date.now() - t0) + " ms: " + text)
        say(diag("before the click"))
        app.externalBanner.reviewRequested.connect(() => say("reviewRequested emitted"))
        app.externalDialog.visibleChanged.connect(() => say("externalDialog.visible -> " + app.externalDialog.visible))
        click(app.externalBanner.reviewButton)
        say("click returned")
        // Up to 30 s instead of 5: how long does it take, when the machine is busy?
        tryVerify(() => app.externalDialog.visible, 30000)
        say("waited: visible=" + app.externalDialog.visible)
        say(diag("after the wait"))
        // (The diagnostic is the failure message: a passing suite prints nothing.)
        fail(lines.join("\n    "))
        tryCompare(app.externalDialog.entries, "length", 1)
        const entry = app.externalDialog.entries[0]
        compare(entry.filename, "IMG_0003.jpg")
        compare(entry.changes.length, 1)
        compare(entry.changes[0].field, "rating")
        compare(entry.changes[0].file, "4")
        compare(entry.changes[0].conflict, false)
        const row = rowOfPhoto("IMG_0003")
        compare(app.photos.ratingAt(row), 0, "nothing applied before the answer")

        click(app.externalDialog.acceptAllButton)
        tryCompare(app.photos, "count", 20)
        tryVerify(() => app.photos.ratingAt(rowOfPhoto("IMG_0003")) === 4)
        tryVerify(() => !app.externalBanner.visible)
        tryCompare(app.externalDialog.entries, "length", 0)
        // (Undo waits for the window to be put away: no command while a dialog is open.)
        app.externalDialog.close()
        tryVerify(() => app.actions.undo.enabled)
        compare(app.actions.undo.text, "Undo accepting external changes to 1 photo")

        // Not reported again.
        rescan()
        verify(!app.externalBanner.visible)
        // One undo takes it back.
        app.actions.undo.trigger()
        tryVerify(() => app.photos.ratingAt(rowOfPhoto("IMG_0003")) === 0)
    }

    function test_a_field_changed_on_both_sides_is_a_choice_that_keeps_the_photos_value_by_default() {
        edit("IMG_0005", "Base", 2)
        // The photo is rated here, and rated differently in the file.
        app.currentTask = "cull"
        app.photos.rateSelection(0)
        const row = rowOfPhoto("IMG_0005")
        app.photos.setRating(row, 5)
        tryVerify(() => app.photos.ratingAt(rowOfPhoto("IMG_0005")) === 5)
        edit("IMG_0005", "Base", 3)
        tryVerify(() => app.externalBanner.visible)

        click(app.externalBanner.reviewButton)
        tryVerify(() => app.externalDialog.visible)
        tryCompare(app.externalDialog.entries, "length", 1)
        compare(app.externalDialog.entries[0].changes[0].conflict, true)
        // Kept by default: accepting leaves the rating as it is.
        click(app.externalDialog.acceptAllButton)
        tryVerify(() => !app.externalBanner.visible)
        compare(app.photos.ratingAt(rowOfPhoto("IMG_0005")), 5)
        // (The review is a modal window: put away before the main window is clicked again, or a platform
        // that enforces modality — Windows — never delivers the click that rescans.)
        app.externalDialog.close()
        tryVerify(() => !app.externalDialog.visible)

        // Another change, and this time the file's value is chosen.
        edit("IMG_0005", "Base", 1)
        tryVerify(() => app.externalBanner.visible)
        click(app.externalBanner.reviewButton)
        tryVerify(() => app.externalDialog.visible)
        tryCompare(app.externalDialog.entries, "length", 1)
        app.externalDialog.choose(app.externalDialog.entries[0].id, "rating", true)
        click(app.externalDialog.acceptAllButton)
        tryVerify(() => app.photos.ratingAt(rowOfPhoto("IMG_0005")) === 1)
    }

    function test_ignore_declines_the_change_and_it_is_not_reported_until_the_file_changes_again() {
        edit("IMG_0007", "Base")
        edit("IMG_0007", "Base", 3)
        tryVerify(() => app.externalBanner.visible)
        click(app.externalBanner.ignoreButton)
        tryVerify(() => !app.externalBanner.visible)
        compare(app.photos.ratingAt(rowOfPhoto("IMG_0007")), 0)
        rescan()
        verify(!app.externalBanner.visible)
        edit("IMG_0007", "Base", 5)
        tryVerify(() => app.externalBanner.visible)
    }

    function test_a_change_still_waiting_is_announced_again_when_the_workspace_opens() {
        edit("IMG_0009", "Base")
        edit("IMG_0009", "Base", 2)
        tryVerify(() => app.externalBanner.visible)
        launchWithPhotos(20)
        tryVerify(() => app.externalBanner.visible)
        compare(app.externalBanner.count, 1)
    }

    function test_the_review_speaks_french() {
        edit("IMG_0011", "Base")
        edit("IMG_0011", "Base", 4)
        tryVerify(() => app.externalBanner.visible)
        app.launcher.chooseLanguage("fr")
        wait(250)
        click(app.externalBanner.reviewButton)
        tryVerify(() => app.externalDialog.visible)
        tryCompare(app.externalDialog.entries, "length", 1)
        compare(app.externalDialog.title, "Métadonnées modifiées par une autre application")
        verify(app.externalDialog.acceptAllButton.text !== "Accept all")
        app.externalDialog.close()
    }
}
