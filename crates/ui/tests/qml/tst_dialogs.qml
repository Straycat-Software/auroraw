// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.ui

// Dialogs are modal: nothing opens over one, the rest of the window waits, and a native folder dialog
// (another window) is covered by a popup. Also Settings and its language, live and remembered.
AppTestCase {
    name: "Dialogs"

    function pressEscape() { keyClick(Qt.Key_Escape); wait(150) }

    function test_a_click_behind_a_modal_dialog_does_nothing() {
        launch(freshMachine())
        app.showSettings()
        wait(200)
        verify(app.settingsDialog.visible)
        mouseClick(app.welcome.newButton)
        wait(100)
        verify(!app.newDialog.visible, "the New button behind the dialog was not reached")
        click(app.hamburger)
        wait(150)
        verify(!app.menu.opened, "nor the hamburger")
        pressEscape()
        click(app.welcome.newButton)
        wait(200)
        verify(app.newDialog.visible, "control: with no dialog the same click opens it")
    }

    function test_the_guard_stops_every_command_that_opens_a_window_while_a_dialog_is_open() {
        launch(freshMachine())
        app.showSettings()
        wait(200)
        keyClick(Qt.Key_N, Qt.ControlModifier)
        keyClick(Qt.Key_F1)
        wait(200)
        verify(!app.newDialog.visible)
        verify(!app.aboutDialog.visible)
        verify(!app.actions.newWorkspace.enabled)
        verify(!app.actions.openWorkspace.enabled)
        verify(!app.actions.settings.enabled)
        verify(!app.actions.about.enabled)
        verify(app.actions.quit.enabled, "Quit is not a window")
        pressEscape()
        verify(app.actions.newWorkspace.enabled)
    }

    function test_the_task_tabs_wait_for_the_dialog_that_is_open() {
        const machine = freshMachine()
        launch(machine)
        createWorkspace("Main")
        compare(app.currentTask, "catalogue")
        app.showSettings()
        wait(200)
        mouseClick(app.tabs.itemAt(1))
        wait(100)
        compare(app.currentTask, "catalogue", "a tab behind the dialog was not reached")
        verify(!app.tabs.itemAt(1).enabled)
        pressEscape()
        click(app.tabs.itemAt(1))
        compare(app.currentTask, "cull")
    }

    // A native folder dialog is another window and Qt does not block ours for it: a modal popup
    // covers the window (the hamburger included) while one is open. (None can open off screen:
    // `nativeDialogForced` stands for one.)
    function test_a_folder_dialog_blocks_the_window_behind_it() {
        launch(freshMachine())
        app.nativeDialogForced = true
        wait(300)
        verify(app.nativeDialogOpen)
        verify(app.waiting.visible, "the window covers itself")
        click(app.hamburger)
        wait(150)
        verify(!app.menu.opened, "the menu did not open under the folder dialog")
        verify(!app.actions.newWorkspace.enabled)
        app.nativeDialogForced = false
        wait(300)
        verify(!app.waiting.visible, "once the dialog is closed the window is usable again")
        click(app.hamburger)
        wait(150)
        verify(app.menu.opened)
    }

    function test_the_language_is_chosen_in_settings_applied_at_once_and_remembered() {
        const machine = freshMachine()
        launch(machine)
        compare(app.welcome.newButton.text, "New workspace…")
        app.showSettings()
        wait(200)
        const buttons = app.settingsDialog.languageButtons
        compare(buttons.count, 3)
        click(buttons.itemAt(2))
        wait(200)
        compare(app.launcher.language, "fr")
        compare(app.welcome.newButton.text, "Nouveau workspace…")
        compare(app.settingsDialog.title, "Paramètres")
        pressEscape()
        // The Alt letters follow the language: Alt+É opens Édition.
        keyClick(Qt.Key_Eacute, Qt.AltModifier)
        wait(250)
        verify(app.menu.itemAt(1).subMenu.visible, "Alt+É opens Édition")
        pressEscape()
        pressEscape()
        app.showSettings()
        wait(200)
        verify(files.read(machinePath(machine) + "/data/app-settings.json").indexOf("\"fr\"") >= 0,
               "the choice is written to the settings file")
        click(buttons.itemAt(1))
        wait(200)
        compare(app.welcome.newButton.text, "New workspace…")
        click(buttons.itemAt(0))
        wait(200)
        verify(["en", "fr"].indexOf(app.launcher.effectiveLanguage) >= 0)
        click(buttons.itemAt(1))
    }

    // D-130/D-131 (Patrick's own review): French is the longest of the languages, and a dialog that
    // holds in it holds in every one. Every row must stay inside the dialog, and the footer's one
    // button sits on the right, where a dialog's answer always is.
    function test_settings_in_french_keeps_every_row_inside_the_dialog_and_its_button_on_the_right() {
        launch(freshMachine())
        app.launcher.chooseLanguage("fr")
        app.showSettings()
        wait(300)
        const dialog = app.settingsDialog
        const right = item => item.mapToItem(dialog.contentItem, item.width, 0).x
        const inside = dialog.contentItem.width
        for (let i = 0; i < dialog.startupButtons.count; i++)
            verify(right(dialog.startupButtons.itemAt(i)) <= inside, "startup button " + i + " fits")
        for (let i = 0; i < dialog.languageButtons.count; i++)
            verify(right(dialog.languageButtons.itemAt(i)) <= inside, "language button " + i + " fits")
        for (const box of [dialog.gapBox, dialog.similarDistanceBox, dialog.similarMinutesBox])
            verify(right(box) <= inside, "a spin box fits")
        const close = dialog.footer.itemAt(0)
        compare(close.text, "Fermer")
        const edge = close.mapToItem(dialog.contentItem, close.width, 0).x
        verify(edge <= inside + dialog.rightPadding, "the button is inside the dialog")
        verify(edge > inside / 2, "and on its right half, not centred or left")
        snapshot("settings-fr")
        pressEscape()
        app.launcher.chooseLanguage("en")
    }

    function test_the_about_dialog_says_what_the_application_is() {
        launch(freshMachine())
        app.showAbout()
        wait(200)
        verify(app.aboutDialog.visible)
        verify(app.launcher.version().length > 0)
        pressEscape()
        verify(!app.aboutDialog.visible)
    }
}
