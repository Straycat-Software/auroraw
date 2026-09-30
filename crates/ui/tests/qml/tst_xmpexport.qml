// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.ui

// The XMP export to the source folders (WP10, spec §5.7, D-024, D-138, D-139): Tools > Export XMP files… opens a
// dialog; the selected photos or a whole source get an XMP file beside the original, a file another application
// changed is held back for the review, Replace asks first, and the choices are remembered. The machine: 20 photos
// in "Card"; every test uses photos of its own, since the machine is shared.
AppTestCase {
    name: "XmpExport"

    function init() {
        launchWithPhotos(20)
        app.currentTask = "cull"
    }

    function cleanup() {
        if (app) {
            app.launcher.chooseLanguage("en")
            if (app.xmpExportDialog.visible)
                app.xmpExportDialog.close()
            if (app.externalDialog.visible)
                app.externalDialog.close()
            if (app.externalBanner.count > 0) {
                app.externalChanges.ignoreAll()
                tryVerify(() => app.externalBanner.count === 0)
            }
        }
        quit()
    }

    function card(name) { return home + "/Card/" + name }

    // Selects the photo of `name` and only it.
    function selectOnly(...names) {
        app.photos.selectNone()
        for (const name of names) {
            for (let row = 0; row < app.photos.count; row++) {
                if (app.photos.summaryAt(row).indexOf(name) >= 0) {
                    app.photos.toggle(row)
                    break
                }
            }
        }
        compare(app.photos.selectedCount, names.length)
    }

    function openDialog() {
        // (A dialog just closed is still on its way out: the command is not enabled until it has gone.)
        tryVerify(() => !app.xmpExportDialog.visible)
        app.actions.exportXmp.trigger()
        // A popup is `visible` at once but its contents are put in place at the first frame drawn with them: a click
        // sent before that (whether the popup has `opened` or not) lands where its buttons are not yet.
        tryVerify(() => app.xmpExportDialog.opened)
        const dialog = app.xmpExportDialog
        // (A frame drawn after this call, hence the update: one drawn before it would leave the wait to time out.)
        dialog.contentItem.update()
        verify(waitForRendering(dialog.contentItem), "a frame was drawn with the dialog in it")
        return dialog
    }

    // The defaults of the form, whatever an earlier test of this suite chose (the choices are remembered).
    function defaults(dialog) {
        dialog.stemNaming.checked = true
        dialog.mergeExisting.checked = true
        dialog.minusOneBox.checked = true
    }

    // Presses Export and waits for the report.
    function runExport(dialog) {
        click(dialog.exportButton)
        if (dialog.phase === "confirm")
            click(dialog.exportButton)
        for (let waited = 0; waited < 30000 && dialog.phase !== "done"; waited += 100)
            wait(100)
        compare(dialog.phase, "done", "the export ended: job " + dialog.job + ", can export " + dialog.canExport
                + ", " + JSON.stringify(dialog.report))
    }

    function test_the_tools_menu_has_the_command_and_its_shortcut_opens_the_dialog() {
        verify(app.actions.exportXmp.enabled)
        compare(app.actions.exportXmp.shortcut.toString(), "Ctrl+Shift+E")
        keyClick(Qt.Key_E, Qt.ControlModifier | Qt.ShiftModifier)
        tryVerify(() => app.xmpExportDialog.visible)
        verify(!app.actions.exportXmp.enabled, "not over a dialog")
        snapshot("xmp-export")
    }

    function test_the_form_starts_on_the_selection_and_falls_back_to_the_source_without_one() {
        selectOnly("IMG_0001", "IMG_0002")
        let dialog = openDialog()
        verify(dialog.scopeSelection.checked && !dialog.scopeSource.checked)
        verify(dialog.scopeSelection.text.indexOf("2") >= 0, dialog.scopeSelection.text)
        dialog.close()
        app.photos.selectNone()
        dialog = openDialog()
        verify(!dialog.scopeSelection.enabled, "no selection to export")
        verify(dialog.scopeSource.checked)
        compare(dialog.sourceBox.count, 1)
        compare(dialog.sourceBox.currentText, "Card")
        verify(dialog.exportButton.enabled)
    }

    function test_each_question_takes_one_answer() {
        const dialog = openDialog()
        dialog.fullNaming.checked = true
        verify(!dialog.stemNaming.checked, "one name or the other")
        dialog.replaceExisting.checked = true
        verify(!dialog.mergeExisting.checked && !dialog.skipExisting.checked)
        dialog.skipExisting.checked = true
        verify(!dialog.replaceExisting.checked)
        dialog.scopeSource.checked = true
        verify(!dialog.scopeSelection.checked, "the scope's two radios are one question although they are not siblings")
    }

    function test_the_selected_photos_get_a_file_beside_their_original_and_only_they_do() {
        selectOnly("IMG_0001", "IMG_0002")
        verify(!files.exists(card("IMG_0001.xmp")))
        const dialog = openDialog()
        defaults(dialog)
        runExport(dialog)
        verify(files.exists(card("IMG_0001.xmp")))
        verify(files.exists(card("IMG_0002.xmp")))
        verify(!files.exists(card("IMG_0003.xmp")), "the others are not touched")
        compare(dialog.report.written, 2)
        verify(dialog.lines[0].indexOf("2 file") === 0, dialog.lines[0])
        verify(files.read(card("IMG_0001.xmp")).indexOf("aur:Export") >= 0, "it carries the marker")
        // Exporting again writes nothing: the files already say it.
        dialog.close()
        selectOnly("IMG_0001", "IMG_0002")
        const again = openDialog()
        runExport(again)
        compare(again.report.written || 0, 0)
        compare(again.report.upToDate, 2)
    }

    // (The last of the suite, by its name: it writes a file for every photo of the machine, which the others share.)
    function test_zz_a_whole_source_is_exported_and_an_existing_file_of_another_application_is_merged() {
        // (A development setting of another application in a file that Auroraw has never seen: kept.)
        files.write(card("IMG_0010.xmp"), '<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">'
            + '<rdf:Description rdf:about="" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" crs:Exposure2012="+0.50"/></rdf:RDF></x:xmpmeta>')
        app.photos.selectNone()
        const dialog = openDialog()
        verify(dialog.scopeSource.checked)
        defaults(dialog)
        runExport(dialog)
        compare(dialog.report.written + dialog.report.heldBack + dialog.report.upToDate, 20, JSON.stringify(dialog.report))
        verify(files.exists(card("IMG_0011.xmp")), "a photo with no file got one")
        const merged = files.read(card("IMG_0010.xmp"))
        // Either merged (the develop setting is still there) or held back for the review (untouched): never lost.
        verify(merged.indexOf("Exposure2012") >= 0, merged)
    }

    function test_a_file_another_application_changed_is_held_back_and_the_banner_says_so() {
        // The first export writes the file and remembers it; another application then rates the photo.
        selectOnly("IMG_0004")
        let dialog = openDialog()
        defaults(dialog)
        runExport(dialog)
        dialog.close()
        const before = files.read(card("IMG_0004.xmp"))
        files.write(card("IMG_0004.xmp"), before.replace("</rdf:Description>", "<xmp:Rating>4</xmp:Rating></rdf:Description>")
                    .replace("xmlns:aur=", 'xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmlns:aur='))
        selectOnly("IMG_0004")
        dialog = openDialog()
        defaults(dialog)
        runExport(dialog)
        compare(dialog.report.heldBack, 1, JSON.stringify(dialog.report))
        verify(dialog.lines.join("\n").indexOf("held back") >= 0)
        tryVerify(() => app.externalBanner.visible, 5000, "the review of external changes is offered")
        verify(files.read(card("IMG_0004.xmp")).indexOf("<xmp:Rating>4") >= 0, "the file is untouched")
    }

    function test_replace_asks_first_and_keeps_the_old_file() {
        selectOnly("IMG_0006")
        let dialog = openDialog()
        defaults(dialog)
        runExport(dialog)
        dialog.close()
        selectOnly("IMG_0006")
        dialog = openDialog()
        dialog.replaceExisting.checked = true
        click(dialog.exportButton)
        compare(dialog.phase, "confirm", "Replace asks before it writes")
        verify(dialog.exportButton.text.indexOf("Replace") === 0)
        click(dialog.backButton)
        compare(dialog.phase, "form")
        click(dialog.exportButton)
        click(dialog.exportButton)
        tryVerify(() => dialog.phase === "done", 30000)
        compare(dialog.report.written, 1, JSON.stringify(dialog.report))
    }

    function test_the_choices_are_remembered_except_replace() {
        selectOnly("IMG_0007")
        let dialog = openDialog()
        dialog.fullNaming.checked = true
        dialog.skipExisting.checked = true
        dialog.minusOneBox.checked = false
        runExport(dialog)
        compare(dialog.report.written, 1)
        verify(files.exists(card("IMG_0007.jpg.xmp")), "named after the whole file as asked")
        dialog.close()
        dialog = openDialog()
        verify(dialog.fullNaming.checked)
        verify(dialog.skipExisting.checked)
        verify(!dialog.minusOneBox.checked)
        dialog.replaceExisting.checked = true
        selectOnly("IMG_0008")
        click(dialog.exportButton)
        click(dialog.exportButton)
        tryVerify(() => dialog.phase === "done", 30000)
        dialog.close()
        const next = openDialog()
        verify(!next.replaceExisting.checked, "Replace is asked for each time")
        verify(next.skipExisting.checked || next.mergeExisting.checked)
    }

    function test_the_dialog_speaks_french() {
        app.launcher.chooseLanguage("fr")
        wait(250)
        selectOnly("IMG_0009")
        const dialog = openDialog()
        compare(dialog.title, "Exporter les fichiers XMP")
        snapshot("xmp-export-fr")
        runExport(dialog)
        snapshot("xmp-export-fr-done")
    }
}
