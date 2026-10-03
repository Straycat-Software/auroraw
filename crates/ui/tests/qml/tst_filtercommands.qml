// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.ui

// The filters of the library's bar as commands of the View menu (D-153, issue #76): every control of the bar has a command
// that does what it does, reached from the keyboard alone (Alt+V, the arrows, Return), with a check mark on the filter in
// force that is **read from the grid**, so that the bar and the menu cannot disagree; Clear all filters, and its key;
// when the commands are available; and French. The 40-photo machine, shared by the tests: each one puts the filters back.
AppTestCase {
    name: "FilterCommands"

    function init() {
        launchWithPhotos(40)
        app.library.grid.forceActiveFocus()
    }

    function cleanup() {
        if (app) {
            app.launcher.chooseLanguage("en")
            app.library.clearFilters()
            app.library.filterFlags(0)
        }
        quit()
    }

    function viewMenu() { return app.menu.itemAt(2).subMenu }
    function isSeparator(item) { return String(item).indexOf("Separator") >= 0 }
    function texts(menu) {
        return Array.from({ length: menu.count }, (_, i) => isSeparator(menu.itemAt(i)) ? "-" : menu.itemAt(i).text)
    }
    function grid() { return app.photos }

    // The highlighted row of a menu (Qt 6.4's `Menu` has the index, not the item).
    function current(menu) { return menu.currentIndex >= 0 ? menu.itemAt(menu.currentIndex) : null }

    // Presses Down until the menu's highlighted row says `text` (a menu is moved with the arrows, from its first row).
    function keyTo(menu, text) {
        for (let i = 0; i <= menu.count && !(current(menu) && current(menu).text === text); i++)
            keyClick(Qt.Key_Down)
        verify(current(menu) && current(menu).text === text, "the arrows reached \"" + text + "\"")
    }

    // From the keyboard alone: Alt+V, the list of `list`, then `choice` in it.
    function chooseByKeys(list, choice) {
        keyClick(Qt.Key_V, Qt.AltModifier)
        tryVerify(() => viewMenu().visible, 2000, "Alt+V opens View")
        keyTo(viewMenu(), list)
        keyClick(Qt.Key_Right)
        const sub = current(viewMenu()).subMenu
        tryVerify(() => sub.visible, 2000, "Right opens " + list)
        keyTo(sub, choice)
        keyClick(Qt.Key_Return)
        tryVerify(() => !app.menu.opened, 2000, "choosing closes the menu")
    }

    function test_the_view_section_lists_every_control_of_the_bar_and_a_clear_all() {
        compare([0, 1, 2, 3, 4].map(i => app.menu.itemAt(i).text.replace("&", "")), ["File", "Edit", "View", "Tools", "Help"])
        compare(texts(viewMenu()), ["Filter by rating", "Filter by flag", "Filter by colour", "Filter by series",
                                    "Clear all filters", "-", "Open all series", "Close all series", "-",
                                    "Refresh the list", "Export the list…"])
        compare(texts(viewMenu().itemAt(0).subMenu), ["Any rating", "1 star or more", "2 stars or more", "3 stars or more",
                                                      "4 stars or more", "5 stars"])
        compare(texts(viewMenu().itemAt(1).subMenu), ["Not rejected", "All photos", "Picked", "Rejected"])
        compare(texts(viewMenu().itemAt(2).subMenu), ["Red", "Yellow", "Green", "Blue", "Purple", "-", "Any colour"])
        compare(texts(viewMenu().itemAt(3).subMenu), ["Any photo", "Photos in a series", "Unresolved series", "Resolved series"])
    }

    // The point of the decision: a rating is chosen without the mouse and without a key bound to it.
    function test_a_rating_is_chosen_with_the_keyboard_alone() {
        compare(grid().minRating, 0)
        chooseByKeys("Filter by rating", "3 stars or more")
        compare(grid().minRating, 3)
        verify(app.actions.rating3.marked)
        verify(!app.actions.ratingAny.marked)
        verify(app.library.filterButtons.itemAt(3).highlighted, "the bar says the same")
    }

    function test_a_flag_and_a_colour_are_chosen_with_the_keyboard_alone() {
        chooseByKeys("Filter by flag", "Picked")
        compare(grid().flagFilter, 2)
        chooseByKeys("Filter by colour", "Blue")
        compare(grid().labelFilter, "blue")
        verify(app.actions.flagsPicked.marked)
        verify(app.actions.colourBlue.marked)
        chooseByKeys("Filter by colour", "Any colour")
        compare(grid().labelFilter, "")
        verify(app.actions.colourAny.marked)
    }

    // Whoever changed a filter, the menu says what is in force: the state is the grid's.
    function test_the_check_marks_follow_the_grid_whoever_changed_it() {
        mouseClick(app.library.filterButtons.itemAt(4))
        tryCompare(grid(), "minRating", 4)
        verify(app.actions.rating4.marked, "the button of the bar was clicked")
        verify(!app.actions.ratingAny.marked)
        app.actions.rating1.trigger()
        compare(grid().minRating, 1)
        verify(app.actions.rating1.marked, "the command was triggered")
        verify(!app.actions.rating4.marked)
        verify(app.library.filterButtons.itemAt(1).highlighted)
        app.library.filterFlags(3)
        verify(app.actions.flagsRejected.marked)
        verify(!app.actions.flagsNotRejected.marked)
        mouseClick(app.library.labelFilterButtons.itemAt(1))
        tryCompare(grid(), "labelFilter", "yellow")
        verify(app.actions.colourYellow.marked)
        // A colour is chosen, not toggled: the same choice again keeps it, and Any colour lifts it.
        app.actions.colourYellow.trigger()
        compare(grid().labelFilter, "yellow")
        app.actions.colourAny.trigger()
        compare(grid().labelFilter, "")
        verify(app.actions.colourAny.marked)
    }

    function test_a_row_that_is_checked_says_so_in_the_menu_and_the_others_make_no_room_for_a_mark() {
        const ratings = viewMenu().itemAt(0).subMenu
        verify(ratings.itemAt(0).markable)
        verify(ratings.itemAt(0).marked, "Any rating is in force")
        verify(!ratings.itemAt(3).marked)
        app.actions.rating3.trigger()
        verify(ratings.itemAt(3).marked)
        verify(!ratings.itemAt(0).marked)
        verify(!viewMenu().itemAt(4).markable, "Clear all filters is a command, not a choice")
        verify(!app.menu.itemAt(0).subMenu.itemAt(0).markable, "nor is a command of File")
        compare(viewMenu().itemAt(4).leftPadding, app.menu.itemAt(0).subMenu.itemAt(0).leftPadding)
        verify(ratings.itemAt(3).leftPadding > viewMenu().itemAt(4).leftPadding)
    }

    function test_clear_all_filters_lifts_every_filter_and_has_a_key_of_its_own() {
        verify(!app.library.filtered)
        verify(!app.actions.clearFilters.enabled, "nothing to clear")
        const keyword = app.library.keywords.create("Filter commands", "")
        verify(keyword.indexOf("error:") !== 0, keyword)
        const collection = app.library.collections.create("Filter commands", "")
        verify(collection.indexOf("error:") !== 0, collection)
        app.actions.rating2.trigger()
        app.actions.flagsAll.trigger()
        app.actions.colourRed.trigger()
        app.library.filterKeyword(keyword, "Filter commands")
        app.library.filterCollection(collection, "Filter commands")
        compare(grid().minRating, 2)
        compare(grid().flagFilter, 1)
        compare(grid().labelFilter, "red")
        compare(grid().keywordFilter, keyword)
        compare(grid().collectionFilter, collection)
        verify(app.library.filtered)
        verify(app.actions.clearFilters.enabled)
        app.library.grid.forceActiveFocus()
        keyClick(Qt.Key_X, Qt.ControlModifier | Qt.ShiftModifier)
        tryCompare(grid(), "minRating", 0)
        tryCompare(grid(), "flagFilter", 0, 5000, "the flags are back to Not rejected, as a workspace opens")
        tryCompare(grid(), "labelFilter", "")
        tryCompare(grid(), "keywordFilter", "")
        tryCompare(grid(), "collectionFilter", "")
        verify(!app.library.filtered)
        verify(!app.actions.clearFilters.enabled)
        verify(app.actions.ratingAny.marked && app.actions.flagsNotRejected.marked && app.actions.colourAny.marked)
    }

    function test_the_commands_wait_for_the_library_and_not_for_a_photo() {
        verify(app.actions.rating3.enabled)
        verify(app.actions.refreshList.enabled)
        verify(app.actions.exportList.enabled, "photos are listed")
        // A filter that lists nothing is the one to change: its commands stay available, the export has nothing to write.
        app.actions.rating5.trigger()
        app.actions.colourPurple.trigger()
        app.actions.flagsRejected.trigger()
        tryCompare(grid(), "count", 0)
        verify(app.actions.ratingAny.enabled, "a list of no photo can still be changed")
        verify(app.actions.clearFilters.enabled)
        verify(!app.actions.exportList.enabled, "nothing to export")
        app.actions.clearFilters.trigger()
        tryVerify(() => grid().count > 0, 5000, "the list is whole again")
        verify(app.actions.exportList.enabled)
        // Not over a dialog, nor in the other task.
        app.settingsDialog.open()
        tryVerify(() => app.settingsDialog.visible)
        verify(!app.actions.rating3.enabled, "not over a dialog")
        verify(!app.actions.refreshList.enabled)
        app.settingsDialog.close()
        tryVerify(() => !app.settingsDialog.visible)
        verify(app.actions.rating3.enabled)
    }

    function test_there_is_no_series_to_filter_by_or_to_open_in_a_catalogue_without_any() {
        compare(grid().seriesCount, 0)
        verify(!app.actions.openAllSeries.enabled)
        verify(!app.actions.closeAllSeries.enabled)
        verify(!app.actions.seriesIn.enabled)
        verify(!app.actions.seriesResolved.enabled)
        verify(app.actions.seriesAny.enabled, "lifting the filter is always possible")
    }

    function test_the_section_and_its_rows_speak_french_with_a_letter_of_their_own() {
        useLanguage("fr")
        compare(app.menu.itemAt(2).text.replace("&", ""), "Affichage")
        keyClick(Qt.Key_H, Qt.AltModifier)
        tryVerify(() => viewMenu().visible, 2000, "Alt+H opens Affichage in French")
        compare(texts(viewMenu())[0], "Filtrer par note")
        compare(texts(viewMenu().itemAt(0).subMenu)[1], "1 étoile ou plus")
        compare(texts(viewMenu().itemAt(1).subMenu)[0], "Non refusées")
        verify(texts(viewMenu()).indexOf("Effacer tous les filtres") >= 0)
        app.menu.close()
        // The letters of the five sections are five different ones.
        const keys = [0, 1, 2, 3, 4].map(i => app.menu.sectionKey(i))
        compare(new Set(keys).size, 5, keys.join(" "))
    }
}
