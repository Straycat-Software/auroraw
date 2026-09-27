// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.ui

// The duplicates report (WP9, D-036, D-108): Ctrl+D (or Edit ▸ Duplicate photos…) opens a read-only list of every
// photo Auroraw found at more than one confirmed location, each with "Show in file manager". The machine: 20 photos
// in "Card", one of them (IMG_0000.jpg) also copied into "Backup" — one photo, two locations, still 20 photos in
// all (the copy joined the existing one, it was not added as its own).
AppTestCase {
    name: "Duplicates"

    function init() {
        launchWithPhotos(20)
    }

    function cleanup() {
        if (app) {
            app.launcher.chooseLanguage("en")
            if (app.duplicatesDialog.visible)
                app.duplicatesDialog.close()
        }
        quit()
    }

    function test_ctrl_d_lists_the_duplicate_with_both_its_locations() {
        verify(!app.duplicatesDialog.visible)
        keyClick(Qt.Key_D, Qt.ControlModifier)
        tryVerify(() => app.duplicatesDialog.visible)
        compare(app.duplicatesDialog.entries.length, 1)
        const entry = app.duplicatesDialog.entries[0]
        compare(entry.filename, "IMG_0000.jpg")
        compare(entry.primary.sourceName, "Card")
        compare(entry.primary.path, "IMG_0000.jpg")
        compare(entry.extra.length, 1)
        compare(entry.extra[0].sourceName, "Backup")
        compare(entry.extra[0].path, "IMG_0000_copy.jpg")
        snapshot("duplicates-en")
        keyClick(Qt.Key_Escape)
        verify(!app.duplicatesDialog.visible)
    }

    function test_show_in_file_manager_records_each_locations_own_file() {
        keyClick(Qt.Key_D, Qt.ControlModifier)
        tryVerify(() => app.duplicatesDialog.visible)
        const entry = app.duplicatesDialog.entries[0]
        verify(app.duplicatesDialog.duplicates.revealLocation(entry.primary.sourceId, entry.primary.path))
        compare(tail(app.photos.lastRevealedPath(), 2), "Card/IMG_0000.jpg")
        verify(app.duplicatesDialog.duplicates.revealLocation(entry.extra[0].sourceId, entry.extra[0].path))
        compare(tail(app.photos.lastRevealedPath(), 2), "Backup/IMG_0000_copy.jpg")
        keyClick(Qt.Key_Escape)
    }

    // The last two segments of a path, slashes and backslashes both understood (D-106's own convention: the engine
    // may resolve the test home to a different real path, symlinks included).
    function tail(path, n) {
        const parts = path.split(/[\\/]/)
        return parts.slice(-n).join("/")
    }

    function test_the_dialog_speaks_french() {
        app.launcher.chooseLanguage("fr")
        wait(250)
        keyClick(Qt.Key_D, Qt.ControlModifier)
        tryVerify(() => app.duplicatesDialog.visible)
        snapshot("duplicates-fr")
        keyClick(Qt.Key_Escape)
    }
}
