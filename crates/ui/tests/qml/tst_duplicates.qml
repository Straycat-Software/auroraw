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

    function test_the_dialog_is_a_resizable_window() {
        // Issue #12: a real secondary window (D-113) instead of a hand-rolled drag handle on a Popup — its own
        // native border does the resizing; what is ours to check is that it is allowed to, down to a sane floor.
        keyClick(Qt.Key_D, Qt.ControlModifier)
        tryVerify(() => app.duplicatesDialog.visible)
        verify(app.duplicatesDialog.minimumWidth > 0)
        verify(app.duplicatesDialog.minimumHeight > 0)
        // What dragging the real border does: `width`/`height` simply follow.
        app.duplicatesDialog.width = 900
        app.duplicatesDialog.height = 700
        compare(app.duplicatesDialog.width, 900)
        compare(app.duplicatesDialog.height, 700)
        keyClick(Qt.Key_Escape)
    }

    function test_export_writes_the_same_report_the_cli_prints() {
        // Issue #13.
        keyClick(Qt.Key_D, Qt.ControlModifier)
        tryVerify(() => app.duplicatesDialog.visible)
        verify(app.duplicatesDialog.exportButton.enabled)
        // Not "duplicates.txt": `run_suite` (qml.rs) already writes this suite's own QtTest report to
        // `home/duplicates.txt` (its report file is named after the suite), so that exact name would
        // collide with a file the harness itself keeps open for the whole run — the true cause of a
        // Windows-only "os error 32" (ERROR_SHARING_VIOLATION) chased at length before this was found.
        const path = home + "/exported-duplicates.txt"
        verify(app.duplicatesDialog.duplicates.exportTo(path))
        const out = files.read(path)
        verify(out.indexOf("Card — IMG_0000.jpg") >= 0, out)
        verify(out.indexOf("Backup — IMG_0000_copy.jpg") >= 0, out)
        compare(out.trim().split("\n").pop(), "1 duplicate photo(s)")
        keyClick(Qt.Key_Escape)
    }

    function test_exporting_the_list_remembers_the_last_folder_chosen() {
        // Issue #18.
        keyClick(Qt.Key_D, Qt.ControlModifier)
        tryVerify(() => app.duplicatesDialog.visible)
        compare(Folders.last("export-duplicates"), "", "nothing remembered yet")
        const folder = home + "/Reports"
        files.mkdir(folder)
        app.duplicatesDialog.exportDialog.choose(folder + "/exported-duplicates.txt")
        compare(Folders.last("export-duplicates"), folder)
        keyClick(Qt.Key_Escape)
    }

    function test_the_dialog_speaks_french() {
        useLanguage("fr")
        keyClick(Qt.Key_D, Qt.ControlModifier)
        tryVerify(() => app.duplicatesDialog.visible)
        snapshot("duplicates-fr")
        keyClick(Qt.Key_Escape)
    }
}
