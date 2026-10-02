// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.ui

// Every view of the interface drawn to a PNG, in English and in French, when AUR_SNAPSHOT_DIR names a
// folder (`snapshot()`): how layout, clipping and French text are looked at without a display, and what
// CI keeps as an artifact. Without the variable the tests only make sure each view can be shown.
// The harness left the fixture workspace at the machine's root (60 photos) and folders to copy from:
// Template3 (3 photos), Cam100 and Cam101 (1).
AppTestCase {
    name: "Views"

    readonly property var languages: ["en", "fr"]

    function inLanguage(code, view) {
        useLanguage(code)
        view(code)
    }

    function test_the_welcome_list_and_its_dialogs() {
        const machine = freshMachine()
        launch(machine)
        for (const code of languages) {
            inLanguage(code, code => {
                compare(app.launcher.screen, "welcome")
                snapshot("welcome-empty-" + code)
            })
        }
        createWorkspace("Main")
        wait(1100)
        createWorkspace("Second")
        quit()
        const parent = machinePath(machine) + "/Pictures/Auroraw"
        move(parent + "/Second", parent + "/Hidden")
        launch(machine)
        compare(app.launcher.screen, "welcome")
        for (const code of languages) {
            inLanguage(code, code => {
                snapshot("welcome-list-" + code)
                app.newDialog.openWith()
                wait(200)
                snapshot("new-workspace-dialog-" + code)
                app.newDialog.close()
                wait(300) // one dialog at a time: the next opens once the last has gone
                app.showSettings()
                wait(200)
                snapshot("settings-dialog-" + code)
                app.settingsDialog.close()
                wait(300)
                app.showAbout()
                wait(200)
                snapshot("about-dialog-" + code)
                app.aboutDialog.close()
                wait(200)
            })
        }
    }

    function test_the_menu_and_the_grid() {
        launch("")
        tryCompare(app.photos, "count", 60)
        for (const code of languages) {
            inLanguage(code, code => {
                app.library.filterBy(0)
                mouseClick(app.library.grid.itemAtIndex(4))
                wait(60)
                keyClick(Qt.Key_3)
                wait(300)
                snapshot("grid-" + code)
                app.menu.openSection(0)
                wait(250)
                snapshot("menu-file-" + code)
                app.menu.close()
                wait(150)
                app.library.grid.forceActiveFocus()
                keyClick(Qt.Key_0)
                wait(200)
            })
        }
    }

    function test_the_catalogue_and_the_import() {
        const machine = freshMachine()
        launch(machine)
        createWorkspace("Main")
        const card = machinePath(machine) + "/Card"
        files.copyDir(home + "/Template3", card)
        files.copyFile(home + "/Cam100/IMG_0000.jpg", card + "/DCIM/100CANON/IMG_0001.JPG")
        files.copyFile(home + "/Cam101/CAM_0000.jpg", card + "/DCIM/101CANON/IMG_0002.JPG")
        for (const code of languages) {
            inLanguage(code, code => {
                app.currentTask = "catalogue"
                wait(150)
                snapshot("catalogue-empty-" + code)
            })
        }
        addSource(card)
        waitForTheScan()
        // The folder that holds the source asks before merging (a parent of it).
        for (const code of languages) {
            inLanguage(code, code => {
                app.currentTask = "catalogue"
                wait(150)
                snapshot("catalogue-" + code)
                click(app.catalogue.addButton)
                wait(150)
                app.flow.addDialog.folderField.text = machinePath(machine)
                click(app.flow.addDialog.addButton)
                wait(150)
                snapshot("add-source-merge-" + code)
                app.flow.addDialog.close()
                wait(150)
                click(app.catalogue.list.itemAtIndex(0).removeButton)
                wait(150)
                snapshot("remove-source-dialog-" + code)
                app.flow.removeDialog.close()
                wait(150)
                app.importForm.volumesOverride = JSON.stringify([{ name: "EOS_DIGITAL", path: card, hasDcim: true }])
                app.cardBanner.cardName = "EOS_DIGITAL"
                app.cardBanner.cardPath = card
                wait(150)
                snapshot("card-banner-" + code)
                app.cardBanner.cardPath = ""
                app.importForm.volumesOverride = ""
                app.importDialog.openWith(card)
                app.importDialog.destinationField.text = machinePath(machine) + "/Archive"
                app.importDialog.fieldsChanged()
                wait(200)
                snapshot("import-dialog-" + code)
                app.importDialog.close()
                wait(150)
            })
        }
    }
}
