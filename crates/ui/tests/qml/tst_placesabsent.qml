// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.ui

// Offline place names without their data (design note 008 §2: "a build without the file still works: the feature reports
// that place names are not installed and the rest of the application is unaffected"). The machine has photos and no places
// file: the dialog says so and offers nothing to press, the Import dialog's option is off and says why, and About says
// nothing of data that this copy does not carry.
AppTestCase {
    name: "PlacesAbsent"

    function init() {
        launchWithPhotos(6)
        app.currentTask = "cull"
    }

    function cleanup() {
        if (app) {
            if (app.placeNamesDialog.visible)
                app.placeNamesDialog.close()
            if (app.importDialog.visible)
                app.importDialog.close()
        }
        quit()
    }

    function test_the_dialog_says_place_names_are_not_installed_and_offers_nothing_to_press() {
        verify(!app.placeNames.installed())
        app.actions.findPlaceNames.trigger()
        tryVerify(() => app.placeNamesDialog.opened)
        const dialog = app.placeNamesDialog
        drawn(dialog.contentItem)
        compare(dialog.phase, "unavailable")
        compare(dialog.title, "Find place names")
        verify(!dialog.findButton.visible, "there is nothing to look up in")
        verify(dialog.closeButton.visible)
        compare(dialog.closeButton.text, "Close")
        snapshot("place-names-not-installed")
        click(dialog.closeButton)
        tryVerify(() => !dialog.visible)
    }

    function test_nothing_else_is_affected_the_command_is_still_there_and_the_application_goes_on() {
        verify(app.actions.findPlaceNames.enabled, "the command is in the menu all the same: it says what is missing")
        compare(app.photos.count, 6)
        app.actions.about.trigger()
        tryVerify(() => app.aboutDialog.visible)
        verify(!app.aboutDialog.placesLabel.visible, "no credit for data this copy does not carry")
        app.aboutDialog.close()
    }

    function test_the_import_option_is_off_and_says_why() {
        app.actions.importPhotos.trigger()
        tryVerify(() => app.importDialog.visible)
        const d = app.importDialog
        verify(!d.findPlacesBox.enabled)
        verify(!d.findPlaces)
        compare(d.findPlacesHint, "Place names are not installed.")
        d.close()
    }
}
