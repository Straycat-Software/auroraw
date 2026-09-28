// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.ui

// The hamburger menu, its shortcuts and the commands behind them, with real key and mouse events.
AppTestCase {
    name: "Menus"

    function openMenu() {
        click(app.hamburger)
        wait(200)
        verify(app.menu.opened, "the hamburger opens the menu")
    }
    function sectionTitle(i) { return app.menu.itemAt(i).text.replace("&", "") }
    function isSeparator(item) { return String(item).indexOf("Separator") >= 0 }
    function pressEscape() { keyClick(Qt.Key_Escape); wait(150) }

    function test_the_hamburger_menu_opens_lists_its_sections_and_runs_a_command() {
        launch(freshMachine())
        openMenu()
        compare(app.menu.count, 4)
        compare([0, 1, 2, 3].map(sectionTitle), ["File", "Edit", "Tools", "Help"])
        // Choosing a section shows its commands; a click on one closes the menu and runs it.
        click(app.menu.itemAt(0))
        const file = app.menu.itemAt(0).subMenu
        verify(file.visible)
        click(file.itemAt(3))
        compare(file.itemAt(3).text, "Settings…")
        wait(200)
        verify(app.settingsDialog.visible)
        verify(!app.menu.opened, "the menu closed")
    }

    function test_the_menus_group_their_items_with_separators_where_they_are_usually_found() {
        launch(freshMachine())
        const file = app.menu.itemAt(0).subMenu
        const edit = app.menu.itemAt(1).subMenu
        const tools = app.menu.itemAt(2).subMenu
        const kinds = menu => Array.from({ length: menu.count }, (_, i) => isSeparator(menu.itemAt(i)) ? "-" : menu.itemAt(i).text)
        // New and Open, then Settings, then Quit.
        compare(kinds(file), ["New workspace…", "Open workspace…", "-", "Settings…", "-", "Quit"])
        // History, then the clipboard, then Select all.
        compare(kinds(edit), ["Undo", "Redo", "-", "Cut", "Copy", "Paste", "Delete", "-", "Select all", "Select none", "Invert selection"])
        // Issue #14: Import, Keywords and Duplicate photos, grouped in their own section.
        compare(kinds(tools), ["Import…", "-", "Keywords", "-", "Duplicate photos…"])
    }

    function test_every_command_shows_its_shortcut_the_way_the_platform_writes_it() {
        launch(freshMachine())
        const file = app.menu.itemAt(0).subMenu
        const tools = app.menu.itemAt(2).subMenu
        const hint = item => item.contentItem.children[1].text
        const key = letter => Qt.platform.os === "osx" ? "⌘" + letter : "Ctrl+" + letter
        compare(hint(file.itemAt(0)), key("N"))
        compare(hint(file.itemAt(1)), key("O"))
        compare(hint(tools.itemAt(0)), key("I"))
        compare(hint(file.itemAt(3)), key(","))
        compare(hint(app.menu.itemAt(3).subMenu.itemAt(0)), "F1")
    }

    function test_the_shortcuts_reach_the_same_commands_as_the_menu() {
        launch(freshMachine())
        keyClick(Qt.Key_N, Qt.ControlModifier)
        wait(200)
        verify(app.newDialog.visible)
        pressEscape()
        verify(!app.newDialog.visible)
        keyClick(Qt.Key_Comma, Qt.ControlModifier)
        wait(200)
        verify(app.settingsDialog.visible)
        pressEscape()
        keyClick(Qt.Key_F1)
        wait(200)
        verify(app.aboutDialog.visible)
        pressEscape()
        verify(!app.aboutDialog.visible)
    }

    function test_the_alt_keys_open_the_menus_sections() {
        launch(freshMachine())
        keyClick(Qt.Key_F, Qt.AltModifier)
        wait(250)
        verify(app.menu.opened)
        verify(app.menu.itemAt(0).subMenu.visible, "Alt+F opens File")
        pressEscape()
        pressEscape()
        keyClick(Qt.Key_T, Qt.AltModifier)
        wait(250)
        verify(app.menu.itemAt(2).subMenu.visible, "Alt+T opens Tools")
        pressEscape()
        pressEscape()
        keyClick(Qt.Key_H, Qt.AltModifier)
        wait(250)
        verify(app.menu.itemAt(3).subMenu.visible, "Alt+H opens Help")
    }

    function test_escape_closes_the_menu_and_the_dialogs() {
        launch(freshMachine())
        openMenu()
        pressEscape()
        pressEscape()
        verify(!app.menu.opened)
        keyClick(Qt.Key_N, Qt.ControlModifier)
        wait(200)
        verify(app.newDialog.visible)
        pressEscape()
        verify(!app.newDialog.visible, "Escape reaches the dialog from its own field")
    }

    function test_the_edit_commands_act_on_the_text_field_that_has_the_keyboard() {
        launch(freshMachine())
        // Nothing to edit on the welcome screen.
        verify(!app.actions.cut.enabled)
        verify(!app.actions.paste.enabled)
        app.newDialog.openWith()
        wait(200)
        const field = app.newDialog.nameField
        verify(field.activeFocus, "the name field has the keyboard")
        verify(app.actions.cut.enabled)
        field.selectAll()
        verify(field.selectedText.length > 0)
        app.actions.cut.trigger()
        compare(field.text, "")
        verify(app.actions.undo.enabled)
        app.actions.undo.trigger()
        verify(field.text.length > 0, "Undo brought it back")
        app.actions.selectAll.trigger()
        app.actions.deleteSelection.trigger()
        compare(field.text, "", "Delete deletes the selected text, nothing else")
        app.actions.undo.trigger()
        field.cursorPosition = field.text.length
        app.actions.selectAll.trigger()
        compare(field.selectedText, field.text)
    }

    function test_importing_is_available_once_a_workspace_is_open_and_no_dialog_is() {
        launch(freshMachine())
        verify(!app.actions.importPhotos.enabled, "no workspace yet")
        createWorkspace("Main")
        verify(app.actions.importPhotos.enabled)
        keyClick(Qt.Key_I, Qt.ControlModifier)
        wait(200)
        verify(app.importDialog.visible, "Ctrl+I opens the Import dialog")
        verify(!app.actions.importPhotos.enabled, "not over a dialog")
        verify(!app.actions.newWorkspace.enabled)
    }

    // Every row of a section is whole the first time the menu opens (the typeface reaches the rows after
    // Qt has sized the menu), in both languages.
    property int cutCount: 0

    function test_no_row_is_cut_short_the_first_time_a_menu_opens() {
        cutCount = 0
        for (const section of [0, 1, 2, 3, 0, 1, 2, 3]) {
            launch(freshMachine())
            createWorkspace("Main")
            // The first four in English, the others in French (the window is new every time).
            app.launcher.chooseLanguage(section === undefined || cutCount++ < 4 ? "en" : "fr")
            wait(250)
            app.menu.openSection(section)
            wait(300)
            const menu = app.menu.itemAt(section).subMenu
            verify(menu.visible)
            for (let i = 0; i < menu.count; i++) {
                const row = menu.itemAt(i)
                if (row.action)
                    verify(!row.contentItem.children[0].truncated, "row " + i + " of section " + section + " is cut")
            }
        }
    }
}
