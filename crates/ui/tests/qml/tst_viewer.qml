// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Window
import QtTest
import org.auroraw.ui

// The image view and cull mode (D-100): Enter or a double click opens the cursor's photo, the keys walk and rate,
// flag and label it (the grid's own actions, undone with Ctrl+Z, and the view follows an undo), Z shows 100 %, the
// filmstrip and the information can be hidden, auto-advance moves on after a key and is remembered, F is full
// screen, Escape goes back with the cursor where the view was. The 40-photo machine, window 1680 wide.
AppTestCase {
    name: "Viewer"

    property var grid: null
    property var view: null

    function init() {
        launchWithPhotos(40)
        grid = app.library.grid
        view = app.library.viewer
    }

    // What a test rated, flagged or labelled is cleared, the options put back and the language too.
    function cleanup() {
        if (app) {
            app.launcher.chooseLanguage("en")
            app.launcher.setViewOption("autoAdvance", false)
            app.launcher.setViewOption("filmstrip", true)
            app.launcher.setViewOption("info", true)
            if (app.library.viewing)
                app.library.closeView()
            // (Rejected photos are not listed unless asked for: they are cleared too.)
            app.library.filterFlags(1)
            wait(200)
            app.library.selectAll()
            app.photos.rateSelection(0)
            app.photos.flagSelection("clear")
            app.photos.labelSelection("none")
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

    // Opens the view on photo `index` with Enter, once its picture is there.
    function openOn(index) {
        click(index)
        keyClick(Qt.Key_Return)
        tryVerify(() => app.library.viewing)
        compare(view.row, index)
        tryVerify(() => view.picture.status === Image.Ready, 20000, "the picture arrived")
    }

    function test_enter_opens_the_cursors_photo_and_escape_goes_back_to_it() {
        openOn(3)
        compare(view.photoId, app.photos.idAt(3))
        verify(view.visible && view.activeFocus, "the view has the keyboard")
        for (let i = 0; i < 5; i++)
            keyClick(Qt.Key_Right)
        compare(view.row, 8)
        keyClick(Qt.Key_Left)
        compare(view.row, 7)
        keyClick(Qt.Key_End)
        compare(view.row, 39)
        keyClick(Qt.Key_Right)
        compare(view.row, 39, "there is no photo after the last")
        keyClick(Qt.Key_Home)
        compare(view.row, 0)
        keyClick(Qt.Key_Space)
        keyClick(Qt.Key_Backspace)
        compare(view.row, 0)
        keyClick(Qt.Key_Right)
        keyClick(Qt.Key_Right)
        keyClick(Qt.Key_Escape)
        verify(!app.library.viewing)
        compare(grid.currentIndex, 2, "the grid's cursor is on the photo that was shown")
        verify(grid.activeFocus)
    }

    function test_a_right_click_in_the_view_opens_the_menu_of_its_photo_and_does_not_reach_the_grid() {
        openOn(7)
        const menu = app.library.viewMenu
        // Nothing to open from inside the view: that menu has no such row (the grid's has).
        for (let i = 0; i < menu.count; i++)
            verify(menu.itemAt(i).text !== "Open in the image view", "no Open row in the view's menu")
        let inGrid = false
        for (let i = 0; i < app.library.cellMenu.count; i++)
            inGrid = inGrid || app.library.cellMenu.itemAt(i).text === "Open in the image view"
        verify(inGrid, "the grid's menu has it")
        // Where a cell of the grid under the view would be, and on the picture itself.
        for (const point of [[60, 100], [view.width / 2, view.height / 3]]) {
            mouseClick(view, point[0], point[1], Qt.RightButton)
            tryVerify(() => menu.visible)
            wait(150)
            snapshot("viewer-menu-en")
            compare(grid.currentIndex, 7, "the photo shown is still the cursor")
            compare(app.photos.selectedCount, 1)
            verify(app.library.viewing)
            menu.close()
            tryVerify(() => !menu.visible)
        }
        // A left click on the dark around the picture does not select a photo of the grid either.
        mouseClick(view, 60, 100)
        compare(grid.currentIndex, 7)
        keyClick(Qt.Key_Escape)
    }

    function test_a_double_click_opens_the_photo_under_it() {
        const item = cell(5)
        mouseDoubleClickSequence(item, item.width / 2, item.height / 2)
        tryVerify(() => app.library.viewing)
        compare(view.row, 5)
        snapshot("viewer-en")
    }

    function test_rating_flagging_and_labelling_are_the_grids_and_ctrl_z_undoes_them() {
        openOn(4)
        keyClick(Qt.Key_4)
        compare(app.photos.ratingAt(4), 4)
        keyClick(Qt.Key_P)
        compare(app.photos.flagAt(4), 1)
        keyClick(Qt.Key_7)
        compare(app.photos.labelAt(4), "yellow")
        keyClick(Qt.Key_7)
        compare(app.photos.labelAt(4), "", "the same colour again takes it off")
        keyClick(Qt.Key_9)
        compare(app.photos.labelAt(4), "blue")
        tryCompare(view, "rating", 4)
        tryCompare(view, "colour", "blue")
        tryCompare(view, "flag", 1)
        verify(view.summary.indexOf("IMG_") === 0, view.summary)
        tryVerify(() => app.actions.undo.text === "Undo label", 5000, app.actions.undo.text)
        wait(300)
        keyClick(Qt.Key_Z, Qt.ControlModifier)
        tryCompare(app.photos, "count", 40)
        tryVerify(() => app.photos.labelAt(4) === "", 5000)
        // The view follows an undo: on a photo further on, undo the rating of photo 4, and the view goes back to it.
        keyClick(Qt.Key_Right)
        keyClick(Qt.Key_Right)
        compare(view.row, 6)
        wait(300)
        keyClick(Qt.Key_Z, Qt.ControlModifier) // the label taken off
        wait(300)
        keyClick(Qt.Key_Z, Qt.ControlModifier)
        tryCompare(view, "row", 4)
    }

    function test_z_shows_the_picture_at_100_percent_and_again_fits_it() {
        openOn(1)
        verify(view.fit)
        const natural = view.picture.implicitWidth
        verify(natural > 0)
        keyClick(Qt.Key_Z)
        verify(!view.fit)
        tryCompare(view.picture, "width", natural)
        keyClick(Qt.Key_Z)
        verify(view.fit)
        compare(view.picture.width, view.picture.implicitWidth * view.fitScale)
        // The wheel zooms about the pointer; the plus and minus keys too.
        keyClick(Qt.Key_Plus)
        verify(!view.fit && view.zoom > view.fitScale)
        keyClick(Qt.Key_Z)
        snapshot("viewer-100-en")
        keyClick(Qt.Key_Z)
    }

    function test_the_filmstrip_and_the_information_can_be_hidden_and_are_remembered() {
        openOn(2)
        verify(view.strip.visible && view.infoBar.visible)
        keyClick(Qt.Key_T)
        verify(!view.strip.visible)
        keyClick(Qt.Key_I)
        verify(!view.infoBar.visible)
        verify(!app.launcher.viewOption("filmstrip") && !app.launcher.viewOption("info"), "remembered")
        keyClick(Qt.Key_T)
        keyClick(Qt.Key_I)
        verify(view.strip.visible && view.infoBar.visible)
        // A click on the filmstrip goes to that photo.
        tryVerify(() => view.strip.itemAtIndex(4) !== null)
        const frame = view.strip.itemAtIndex(4)
        mouseClick(frame, frame.width / 2, frame.height / 2)
        compare(view.row, 4)
    }

    function test_auto_advance_moves_on_after_a_key_and_is_remembered() {
        openOn(10)
        keyClick(Qt.Key_3)
        compare(view.row, 10, "off: it stays")
        keyClick(Qt.Key_A)
        verify(view.autoAdvance)
        verify(app.launcher.viewOption("autoAdvance"), "remembered")
        keyClick(Qt.Key_5)
        compare(view.row, 11, "the key rated this photo and moved on")
        keyClick(Qt.Key_X)
        compare(view.row, 12)
        compare(app.photos.ratingAt(10), 5)
        compare(app.photos.flagAt(11), 2)
        // A new window remembers it.
        app.library.closeView()
        launchWithPhotos(39) // the rejected one is not listed
        app.library.openView(0)
        tryVerify(() => app.library.viewer.autoAdvance)
        app.library.closeView()
    }

    function test_f_makes_the_window_full_screen_without_its_menu_and_escape_brings_it_back() {
        openOn(0)
        const header = app.header
        verify(header.visible)
        keyClick(Qt.Key_F)
        tryCompare(app, "visibility", Window.FullScreen)
        tryVerify(() => !header.visible)
        keyClick(Qt.Key_Escape)
        verify(!app.library.viewing)
        tryVerify(() => app.visibility !== Window.FullScreen)
        tryVerify(() => header.visible)
    }

    function test_a_keyboard_only_walk_rates_and_flags_every_photo() {
        app.library.openView(0)
        tryVerify(() => app.library.viewing)
        keyClick(Qt.Key_A) // auto-advance
        for (let i = 0; i < 40; i++) {
            compare(view.row, i)
            keyClick(Qt.Key_1 + (i % 5))
            if (i % 2 === 0) {
                // The flag is the next key's: on the same photo, so step back and forward around it.
                keyClick(Qt.Key_Left)
                keyClick(Qt.Key_P)
            }
        }
        // Auto-advance stopped at the last photo; every photo has its stars, the even ones are picked.
        tryVerify(() => {
            for (let i = 0; i < 40; i++)
                if (app.photos.ratingAt(i) !== (i % 5) + 1 || app.photos.flagAt(i) !== (i % 2 === 0 ? 1 : 0))
                    return false
            return true
        }, 15000, "every photo has its stars and flag")
    }

    function test_a_colour_key_in_the_grid_shows_on_the_cell_at_once() {
        click(3)
        keyClick(Qt.Key_6)
        tryCompare(cell(3), "colourLabel", "red")
        keyClick(Qt.Key_6)
        tryCompare(cell(3), "colourLabel", "", 5000, "the same colour takes it off")
        keyClick(Qt.Key_9)
        tryCompare(cell(3), "colourLabel", "blue")
    }

    function test_the_context_menu_gives_purple_and_the_flags_to_what_is_under_the_pointer() {
        click(1)
        const item = cell(5)
        mouseClick(item, item.width / 2, item.height / 2, Qt.RightButton)
        const menu = app.library.cellMenu
        tryVerify(() => menu.visible)
        compare(app.photos.selectedCount, 1)
        compare(grid.currentIndex, 5, "a photo outside the selection becomes the selection")
        let purple = null, reject = null
        for (let i = 0; i < menu.count; i++) {
            const row = menu.itemAt(i)
            if (row && row.text === "Purple") purple = row
            if (row && row.text === "Reject") reject = row
        }
        verify(purple && reject, "the menu has Purple and Reject")
        wait(150)
        snapshot("grid-menu-en")
        purple.triggered()
        reject.triggered()
        menu.close()
        tryCompare(cell(5), "colourLabel", "purple")
        tryCompare(cell(5), "flag", 2)
    }

    function test_the_grid_can_be_filtered_by_colour() {
        click(0)
        click(2, Qt.ShiftModifier)
        keyClick(Qt.Key_8)
        click(6)
        keyClick(Qt.Key_6)
        wait(500)
        const dots = app.library.labelFilterButtons
        compare(dots.count, 5)
        mouseClick(dots.itemAt(2)) // green
        tryCompare(app.photos, "count", 3)
        compare(app.photos.labelFilter, "green")
        mouseClick(dots.itemAt(2))
        tryCompare(app.photos, "count", 40, 5000, "the same dot again lists them all")
        mouseClick(dots.itemAt(0)) // red
        tryCompare(app.photos, "count", 1)
        mouseClick(dots.itemAt(4)) // purple: none has it
        tryCompare(app.photos, "count", 0)
        mouseClick(dots.itemAt(4))
        tryCompare(app.photos, "count", 40)
    }

    function test_the_state_buttons_of_the_view_go_round_their_states() {
        openOn(2)
        for (let n = 1; n <= 6; n++) {
            mouseClick(view.ratingButton)
            tryCompare(app.photos, "count", 40)
            tryVerify(() => app.photos.ratingAt(2) === n % 6, 5000, "rating " + n)
        }
        mouseClick(view.flagButton)
        tryVerify(() => app.photos.flagAt(2) === 1)
        mouseClick(view.flagButton)
        tryVerify(() => app.photos.flagAt(2) === 2)
        mouseClick(view.flagButton)
        tryVerify(() => app.photos.flagAt(2) === 0)
        for (const colour of ["red", "yellow", "green", "blue", "purple", ""]) {
            mouseClick(view.colourButton)
            tryVerify(() => app.photos.labelAt(2) === colour, 5000, "colour " + colour)
        }
        // The buttons show the state in its colours.
        mouseClick(view.flagButton)
        tryCompare(view, "flag", 1)
        compare(view.flagButton.palette.buttonText.toString(), Qt.color(Theme.picked).toString())
        snapshot("viewer-state-en")
    }

    function test_the_state_buttons_of_the_view_say_their_state_to_a_screen_reader() {
        openOn(2)
        // (Their icons say nothing: the description does.)
        for (let n = 0; n < 3; n++) {
            const rating = view.rating
            mouseClick(view.ratingButton)
            tryVerify(() => view.rating !== rating, 5000, "rating " + rating)
            compare(view.ratingButton.Accessible.description, view.rating + (view.rating === 1 ? " star" : " stars"))
        }
        for (let n = 0; n < 3; n++) {
            const flag = view.flag
            compare(view.flagButton.Accessible.description, ["", "Picked", "Rejected"][flag])
            mouseClick(view.flagButton)
            tryVerify(() => view.flag !== flag, 5000, "flag " + flag)
        }
    }

    function test_leaving_full_screen_gives_back_a_maximised_window() {
        openOn(0)
        app.visibility = Window.Maximized
        tryCompare(app, "visibility", Window.Maximized)
        keyClick(Qt.Key_F)
        tryCompare(app, "visibility", Window.FullScreen)
        keyClick(Qt.Key_F)
        tryCompare(app, "visibility", Window.Maximized)
        keyClick(Qt.Key_F)
        tryCompare(app, "visibility", Window.FullScreen)
        keyClick(Qt.Key_Escape)
        tryCompare(app, "visibility", Window.Maximized, 5000, "closing the view leaves full screen the same way")
    }

    function test_the_view_speaks_french() {
        openOn(1)
        app.launcher.chooseLanguage("fr")
        wait(300)
        keyClick(Qt.Key_8)
        tryVerify(() => app.actions.undo.text === "Annuler l’étiquette", 5000, app.actions.undo.text)
        compare(app.photos.labelAt(1), "green")
        snapshot("viewer-fr")
    }
}
