// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.ui

// The Info panel, the inspector's third tab: the grid's active photo's own technical metadata, read-only,
// always one photo (never the selection). The machine's own photos are generated JPEGs with no EXIF at
// all (`support::write_photos`): capture time, camera, lens, orientation and GPS are all unknown and left
// out, and dimensions (read from the decoded image itself, not EXIF) is the one field that always shows
// -- exactly the "left out when unknown" behaviour this suite exists to prove, plus the panel reaching the
// catalogue at all. `crates/engine/src/technical_details.rs`'s own unit tests cover every field's exact
// value with a controlled fixture; nothing here needs to. Moving the cursor reloads it; the tab and its
// labels speak French. The 40-photo machine.
AppTestCase {
    name: "Info"

    property var grid: null

    function init() {
        launchWithPhotos(40)
        grid = app.library.grid
        app.keywordPanel.tabs.currentIndex = 2
    }

    function cleanup() {
        if (app)
            app.launcher.chooseLanguage("en")
        quit()
    }

    function cell(index) {
        grid.positionViewAtIndex(index, GridView.Contain)
        wait(30)
        const item = grid.itemAtIndex(index)
        verify(item, "the cell " + index + " exists")
        return item
    }

    function click(index) {
        const item = cell(index)
        mouseClick(item, item.width / 2, item.height / 2)
        wait(40)
    }

    // The row for `key` in the panel's own current rows, scrolled into view first (the list is
    // virtualised, a row off screen does not exist yet); `null` when the photo does not carry it.
    function rowFor(key) {
        const panel = app.keywordPanel.infoPanel
        for (let i = 0; i < panel.rows.length; i++) {
            if (panel.rows[i].key === key) {
                panel.list.positionViewAtIndex(i, ListView.Contain)
                wait(30)
                return panel.list.itemAtIndex(i)
            }
        }
        return null
    }

    function test_nothing_selected_shows_a_placeholder() {
        grid.currentIndex = -1
        tryVerify(() => app.keywordPanel.infoPanel.rows.length === 0, 5000)
    }

    function test_a_photos_dimensions_show() {
        click(0)
        tryVerify(() => app.keywordPanel.infoPanel.rows.length > 0, 5000)
        const dimensions = rowFor("dimensions")
        verify(dimensions !== null, "every photo has pixel dimensions")
        compare(dimensions.value, "160 × 120", "write_photos's own fixed size")
        snapshot("info-en")
    }

    function test_fields_the_photo_does_not_carry_are_left_out() {
        click(0)
        tryVerify(() => app.keywordPanel.infoPanel.rows.length > 0, 5000)
        for (const key of ["capture-time", "camera", "lens", "shutter", "aperture", "iso",
                            "focal-length", "orientation", "gps-latitude", "gps-longitude",
                            "gps-altitude", "serial"])
            verify(rowFor(key) === null, key + " is not known for a photo with no EXIF")
    }

    function test_the_panel_reloads_when_the_grid_cursor_moves() {
        click(0)
        tryVerify(() => app.keywordPanel.infoPanel.rows.length > 0, 5000)
        compare(app.keywordPanel.infoPanel.row, 0)
        click(5)
        tryVerify(() => app.keywordPanel.infoPanel.row === 5, 5000)
        tryVerify(() => rowFor("dimensions") !== null, 5000, "the new active photo's own row loaded")
    }

    function test_the_tab_and_the_rows_speak_french() {
        useLanguage("fr")
        click(0)
        tryVerify(() => app.keywordPanel.infoPanel.rows.length > 0, 5000)
        compare(app.keywordPanel.tabs.itemAt(2).text, "Infos")
        const dimensions = rowFor("dimensions")
        verify(dimensions !== null)
        compare(dimensions.label, "Dimensions")
    }
}
