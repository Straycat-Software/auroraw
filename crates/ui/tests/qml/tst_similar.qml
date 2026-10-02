// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.ui

// Similar photos (WP9 slice 4, D-105): M shows a panel of the photos that look like the one under the cursor and were
// taken near it, nearest first, made from a hash of each thumbnail that the workers make in the background; a click
// goes to one, the buttons group them with the photo as a series (one undo step) or compare them, and the two settings
// (how many bits, how many minutes) change what is offered. The similar machine: 30 photos, the first four the same
// scene at 10:00, 10:10, 10:20 and 10:55 (IMG_0000 to IMG_0003), the others unrelated scenes from 11:00. The grid
// lists the newest first: the 26 others, then IMG_0003 (row 26), IMG_0002, IMG_0001 and IMG_0000 (row 29).
AppTestCase {
    name: "Similar"

    property var grid: null
    property var panel: null

    function init() {
        launchWithPhotos(30)
        grid = app.library.grid
        panel = app.library.similarPanel
    }

    function cleanup() {
        if (app) {
            app.launcher.chooseLanguage("en")
            app.launcher.setIntOption("similarDistance", 10)
            app.launcher.setIntOption("similarMinutes", 30)
            if (app.library.comparing)
                app.library.closeCompare()
            app.library.showSimilar(false)
            app.library.selectAll()
            app.photos.ungroupSelection()
            wait(400)
        }
        quit()
    }

    function rowOf(name) {
        for (let row = 0; row < app.photos.count; row++)
            if (app.photos.infoAt(row).indexOf(name) >= 0)
                return row
        return -1
    }

    function nameOf(id) { return app.photos.infoAt(app.photos.rowOf(id)).split(" ")[0] }

    function cell(index) {
        grid.positionViewAtIndex(index, GridView.Contain)
        wait(30)
        const item = grid.itemAtIndex(index)
        verify(item, "the cell " + index + " exists")
        return item
    }

    function click(index, modifiers) {
        const item = cell(index)
        mouseClick(item, item.width / 2, item.height / 2 - 10, Qt.LeftButton,
                   modifiers === undefined ? Qt.NoModifier : modifiers)
        wait(60)
    }

    // The names of the photos the panel suggests, in its order.
    function suggested() { return panel.similar.map(s => nameOf(s.id)) }

    // Puts the cursor on `name` and waits until the panel has the suggestions for it (the hashes are made in the
    // background, a few at a time, when the application has just opened).
    function look(name, count) {
        click(rowOf(name))
        tryVerify(() => panel.reference === app.photos.idAt(grid.currentIndex), 5000)
        tryVerify(() => panel.pending === 0 && panel.count === count, 30000,
                  name + ": " + panel.count + " suggested, " + panel.pending + " to hash")
    }

    function test_m_shows_the_photos_that_look_alike_nearest_first_and_m_again_hides_them() {
        verify(!panel.visible)
        compare(rowOf("IMG_0000"), 29)
        click(29)
        keyClick(Qt.Key_M)
        tryVerify(() => panel.visible)
        look("IMG_0000", 2)
        compare(suggested().slice().sort().join(), "IMG_0001.jpg,IMG_0002.jpg", "the same scene within half an hour")
        verify(panel.similar.every(s => s.distance <= 10))
        verify(panel.similar[0].distance <= panel.similar[1].distance, "nearest first")
        // The unrelated scenes and the photo of 10:55 are not offered.
        verify(suggested().indexOf("IMG_0003.jpg") < 0)
        snapshot("similar-en")
        keyClick(Qt.Key_M)
        verify(!panel.visible)
    }

    function test_a_click_on_a_suggestion_goes_to_that_photo_and_the_panel_follows() {
        click(rowOf("IMG_0000"))
        keyClick(Qt.Key_M)
        look("IMG_0000", 2)
        tryVerify(() => panel.thumbs.itemAtIndex(0) !== null)
        const first = panel.similar[0]
        mouseClick(panel.thumbs.itemAtIndex(0), 20, 20)
        tryCompare(grid, "currentIndex", app.photos.rowOf(first.id))
        tryVerify(() => panel.reference === first.id, 5000)
        tryVerify(() => panel.count === 2, 5000)
        verify(suggested().indexOf(nameOf(first.id)) < 0, "not itself")
    }

    function test_the_two_settings_change_what_is_offered() {
        click(rowOf("IMG_0000"))
        keyClick(Qt.Key_M)
        look("IMG_0000", 2)
        // The photo of 10:55 is 55 minutes away: an hour and a half brings it in.
        app.launcher.setIntOption("similarMinutes", 90)
        panel.refresh()
        tryVerify(() => panel.count === 3, 5000)
        verify(suggested().indexOf("IMG_0003.jpg") >= 0)
        // No bit may differ: the same scene lit a little more still hashes the same, so they stay; but a tighter
        // window than ten minutes drops the two photos that are further than that.
        app.launcher.setIntOption("similarMinutes", 12)
        panel.refresh()
        tryVerify(() => panel.count === 1, 5000)
        compare(suggested()[0], "IMG_0001.jpg")
        // The settings dialog has them.
        compare(app.launcher.intOption("similarMinutes"), 12)
    }

    function test_group_makes_one_series_of_the_photo_and_the_similar_ones_and_ctrl_z_undoes_it() {
        click(rowOf("IMG_0000"))
        keyClick(Qt.Key_M)
        look("IMG_0000", 2)
        verify(panel.groupButton.enabled)
        mouseClick(panel.groupButton)
        tryCompare(app.photos, "seriesCount", 1, 10000)
        compare(app.photos.count, 28, "three photos are one row")
        tryVerify(() => app.actions.undo.text === "Undo grouping 3 photos", 5000, app.actions.undo.text)
        // The series is made: nothing is left to suggest for its photos.
        tryVerify(() => panel.count === 0, 5000)
        wait(300)
        grid.forceActiveFocus()
        keyClick(Qt.Key_Z, Qt.ControlModifier)
        tryCompare(app.photos, "seriesCount", 0, 10000)
        compare(app.photos.count, 30)
    }

    function test_compare_opens_the_photo_with_the_nearest_ones() {
        click(rowOf("IMG_0000"))
        keyClick(Qt.Key_M)
        look("IMG_0000", 2)
        mouseClick(panel.compareButton)
        tryVerify(() => app.library.comparing)
        compare(app.library.compareView.ids.length, 3)
        keyClick(Qt.Key_Escape)
    }

    // A bug: when the panel's reference was itself the cover of a closed series, its row stood for every member of
    // that series (as it does for rating, flagging...), so Compare's "reference and its nearest" selection swept the
    // series mate in with it, and the fallback that "too many photos selected" triggers on then compared the series
    // alone, dropping the real suggestions. Compare must show exactly the reference and its own nearest, whatever
    // series the reference happens to be the cover of.
    function test_compare_from_a_closed_series_compares_the_reference_and_its_nearest_not_its_series_mate() {
        const mateId = app.photos.idAt(rowOf("IMG_0003"))
        click(rowOf("IMG_0000"))
        click(rowOf("IMG_0003"), Qt.ControlModifier)
        keyClick(Qt.Key_G, Qt.ControlModifier)
        tryCompare(app.photos, "seriesCount", 1, 5000)
        click(rowOf("IMG_0000"))
        keyClick(Qt.Key_M)
        tryVerify(() => panel.reference === app.photos.idAt(grid.currentIndex), 5000)
        tryVerify(() => panel.pending === 0, 30000)
        // IMG_0003 is grouped with it already, so only IMG_0001 and IMG_0002 are suggested.
        compare(suggested().slice().sort().join(), "IMG_0001.jpg,IMG_0002.jpg")
        mouseClick(panel.compareButton)
        tryVerify(() => app.library.comparing)
        const ids = app.library.compareView.ids
        compare(ids.length, 3, "the reference and its two nearest, not its whole series")
        verify(ids.indexOf(mateId) < 0, "the series mate must not appear just because the reference stands for it")
        keyClick(Qt.Key_Escape)
    }

    // Another bug from the same cause: a suggestion's tooltip named the cover of its closed series instead of the
    // suggested photo itself, going through its row (which stands for the whole series) rather than its own identity.
    function test_a_suggestions_tooltip_names_its_own_file_even_when_a_closed_series_hides_it() {
        const hiddenId = app.photos.idAt(rowOf("IMG_0002"))
        // IMG_0001 and IMG_0002 become a series by hand; IMG_0001 (the earlier one) is its cover, so the row that
        // stands for the series now answers to IMG_0001's name, but IMG_0002 is still suggested on its own.
        click(rowOf("IMG_0001"))
        click(rowOf("IMG_0002"), Qt.ControlModifier)
        keyClick(Qt.Key_G, Qt.ControlModifier)
        tryCompare(app.photos, "seriesCount", 1, 5000)
        click(rowOf("IMG_0000"))
        keyClick(Qt.Key_M)
        tryVerify(() => panel.reference === app.photos.idAt(grid.currentIndex), 5000)
        tryVerify(() => panel.pending === 0, 30000)
        verify(panel.similar.some(s => s.id === hiddenId), "IMG_0002 is still suggested, its series being closed or not")
        compare(panel.filenameOf(hiddenId), "IMG_0002.jpg")
        keyClick(Qt.Key_M)
    }

    function test_the_panel_speaks_french() {
        useLanguage("fr")
        click(rowOf("IMG_0000"))
        keyClick(Qt.Key_M)
        look("IMG_0000", 2)
        snapshot("similar-fr")
    }
}
