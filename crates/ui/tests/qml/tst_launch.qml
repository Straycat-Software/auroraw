// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.ui

// Opening and creating workspaces: the welcome list, the New workspace dialog, what reopens on the
// next launch. Each test runs on a machine of its own.
AppTestCase {
    name: "Launch"

    function test_a_first_launch_offers_to_create_or_open_a_workspace() {
        const machine = freshMachine()
        launch(machine)
        compare(app.launcher.screen, "welcome")
        compare(app.known.count, 0, "nothing is known yet")
        verify(app.welcome.openButton.visible)
        click(app.welcome.newButton)
        verify(app.newDialog.visible)
    }

    function test_creating_a_workspace_from_the_dialog_opens_it_and_remembers_it() {
        const machine = freshMachine()
        launch(machine)
        click(app.welcome.newButton)
        // The folder is the one that will hold the workspace's own folder, named after it.
        const parent = machinePath(machine) + "/Pictures/Auroraw"
        compare(app.newDialog.nameField.text, "Main")
        compare(files.canonical(app.newDialog.folderField.text), files.canonical(parent))
        compare(files.canonical(app.launcher.preview("Main", parent)), files.canonical(parent + "/Main"))

        click(app.newDialog.createButton)
        compare(app.launcher.screen, "workspace")
        compare(app.launcher.workspaceName, "Main")
        compare(app.currentTask, "catalogue", "a new workspace opens on what fills it")
        verify(files.exists(parent + "/Main/workspace.json"))
        verify(!files.exists(parent + "/workspace.json"), "the folder itself is not the workspace")
        app.known.refresh()
        compare(app.known.count, 1)
    }

    function test_the_new_workspace_dialog_remembers_the_last_folder_chosen() {
        // Issue #9: the folder proposed used to always be the fixed default.
        const machine = freshMachine()
        launch(machine)
        const custom = machinePath(machine) + "/Elsewhere"
        app.newDialog.openWith()
        app.newDialog.folderField.text = custom
        click(app.newDialog.createButton)
        wait(200)
        verify(files.exists(custom + "/Main/workspace.json"))

        // Creating a second workspace proposes the folder just used, not the fixed default.
        app.newDialog.openWith()
        compare(files.canonical(app.newDialog.folderField.text), files.canonical(custom))
    }

    function test_the_startup_behavior_setting_controls_whether_the_last_workspace_reopens() {
        // Issue #11: asking for the list instead is honoured on the next launch.
        const machine = freshMachine()
        launch(machine)
        createWorkspace("Main")
        compare(app.launcher.screen, "workspace")

        app.showSettings()
        wait(150)
        click(app.settingsDialog.startupButtons.itemAt(1))
        app.settingsDialog.close()
        wait(150)

        launch(machine)
        compare(app.launcher.screen, "welcome", "asked to see the list instead of reopening")
        app.known.refresh()
        compare(app.known.count, 1, "the workspace is still known")
    }

    function test_the_preview_follows_the_name_and_the_folder_and_the_folder_is_left_alone() {
        const machine = freshMachine()
        launch(machine)
        const parent = machinePath(machine) + "/Pictures/Auroraw"
        const preview = (name, folder) => app.launcher.preview(name, folder)
        compare(files.canonical(preview("Family", parent)), files.canonical(parent + "/Family"))
        compare(files.canonical(preview("Family / trips: 2026", parent)), files.canonical(parent + "/Family _ trips_ 2026"),
                "what a file system refuses is replaced")
        // Nothing to show while the name is empty or the folder is not a full path.
        compare(preview("  ", parent), "")
        compare(preview("Main", "relative/path"), "")
        // In the dialog the preview follows the fields, and a folder typed by hand is left alone.
        app.newDialog.openWith()
        wait(200)
        app.newDialog.folderField.text = machinePath(machine) + "/Elsewhere"
        app.newDialog.nameField.text = "Renamed"
        compare(app.newDialog.folderField.text, machinePath(machine) + "/Elsewhere")
    }

    function test_a_taken_name_is_offered_with_a_number() {
        const machine = freshMachine()
        launch(machine)
        const parent = machinePath(machine) + "/Pictures/Auroraw"
        files.mkdir(parent + "/Main")
        app.newDialog.openWith()
        wait(200)
        compare(app.newDialog.nameField.text, "Main 2")
        compare(files.canonical(app.newDialog.folderField.text), files.canonical(parent))
        click(app.newDialog.createButton)
        compare(app.launcher.screen, "workspace")
        verify(files.exists(parent + "/Main 2/workspace.json"))
    }

    function test_a_workspace_that_cannot_be_created_says_why_and_the_dialog_stays() {
        const machine = freshMachine()
        launch(machine)
        app.newDialog.openWith()
        wait(200)
        const dialog = app.newDialog
        dialog.nameField.text = "   "
        click(dialog.createButton)
        compare(dialog.error, "Give the workspace a name.")

        dialog.nameField.text = "Main"
        dialog.folderField.text = "photos/relative"
        click(dialog.createButton)
        verify(dialog.error.indexOf("Cannot create the workspace:") === 0, dialog.error)
        verify(dialog.error.indexOf("absolute") >= 0)
        compare(app.launcher.screen, "welcome")
        verify(dialog.visible)

        // A folder of that name that already holds something is never built into.
        const parent = machinePath(machine) + "/Pictures/Auroraw"
        files.mkdir(parent + "/Main")
        files.write(parent + "/Main/notes.txt", "mine")
        dialog.folderField.text = parent
        click(dialog.createButton)
        verify(dialog.error.indexOf("already exists") >= 0, dialog.error)
        compare(app.launcher.screen, "welcome")
        verify(!files.exists(parent + "/Main/workspace.json"))

        // An empty one is fine.
        files.write(parent + "/Main/notes.txt", "")
        move(parent + "/Main", parent + "/Main-old")
        files.mkdir(parent + "/Main")
        click(dialog.createButton)
        compare(app.launcher.screen, "workspace")
        verify(files.exists(parent + "/Main/workspace.json"))
    }

    function test_a_typed_folder_is_understood_as_its_canonical_form() {
        const machine = freshMachine()
        launch(machine)
        const parent = machinePath(machine) + "/Pictures"
        // A path with `..` and a repeated separator is what a person may type.
        const typed = parent + "//Auroraw/x/../"
        compare(app.launcher.preview("Main", typed), app.launcher.preview("Main", parent + "/Auroraw"))
        verify(app.launcher.preview("Main", typed).indexOf("..") < 0)
        verify(app.launcher.preview("Main", typed).indexOf("//") < 0)
        // The workspace is made there, not in a folder named "..".
        app.newDialog.openWith()
        wait(200)
        app.newDialog.folderField.text = typed
        click(app.newDialog.createButton)
        compare(app.launcher.screen, "workspace")
        verify(files.exists(parent + "/Auroraw/Main/workspace.json"))
    }

    function test_the_last_workspace_reopens_on_the_next_launch() {
        const machine = freshMachine()
        launch(machine)
        createWorkspace("Main")
        compare(app.launcher.screen, "workspace")
        launch(machine)
        compare(app.launcher.screen, "workspace", app.launcher.note)
        compare(app.launcher.workspaceName, "Main")
        compare(app.currentTask, "catalogue", "a workspace with no photos opens on what fills it")
    }

    function test_a_last_workspace_that_cannot_be_found_leads_back_to_the_welcome_list() {
        const machine = freshMachine()
        launch(machine)
        createWorkspace("Main")
        quit()
        const parent = machinePath(machine) + "/Pictures/Auroraw"
        move(parent + "/Main", parent + "/Moved")

        launch(machine)
        compare(app.launcher.screen, "welcome")
        verify(app.welcome.noteText.indexOf("The last workspace could not be found:") === 0, app.welcome.noteText)
        compare(app.known.count, 1)
        const removeButton = app.welcome.list.itemAtIndex(0)
        verify(removeButton, "the lost workspace is listed")
        // It can be taken off the list.
        app.known.forget(0)
        wait(100)
        compare(app.known.count, 0)
    }

    // Two workspaces on a machine, the second (opened last) then moved out of sight: the next launch
    // cannot reopen it and shows the welcome list. Returns where it went.
    function machineWithALostLastWorkspace(machine) {
        launch(machine)
        createWorkspace("Main")
        // The registry dates an opening to the second: two in one second would tie.
        wait(1100)
        createWorkspace("Second")
        quit()
        const parent = machinePath(machine) + "/Pictures/Auroraw"
        move(parent + "/Second", parent + "/Hidden")
        return parent + "/Hidden"
    }

    function test_a_known_workspace_opens_from_the_list() {
        const machine = freshMachine()
        machineWithALostLastWorkspace(machine)
        launch(machine)
        compare(app.launcher.screen, "welcome")
        compare(app.known.count, 2)
        // Most recent first: Second (lost), then Main.
        const row = app.welcome.list.itemAtIndex(1)
        verify(row)
        click(row)
        compare(app.launcher.screen, "workspace")
        compare(app.launcher.workspaceName, "Main")
    }

    function test_a_workspace_opens_from_a_chosen_folder_and_a_plain_folder_is_refused() {
        const machine = freshMachine()
        const hidden = machineWithALostLastWorkspace(machine)
        const plain = machinePath(machine) + "/Plain"
        files.mkdir(plain)
        launch(machine)
        compare(app.launcher.screen, "welcome")

        // What the folder dialog hands over when a folder is chosen.
        app.openFolder(plain)
        compare(app.launcher.screen, "welcome", "a plain folder is not a workspace")
        verify(app.notice.indexOf("Cannot open the workspace:") === 0, app.notice)
        click(app.noticeBar.dismissButton)
        compare(app.notice, "")

        app.openFolder(hidden)
        compare(app.launcher.screen, "workspace")
        compare(app.launcher.workspaceName, "Second")
        // The registry follows the workspace to where it now is.
        app.known.refresh()
        let found = false
        for (let row = 0; row < app.known.count; row++)
            found = found || app.known.pathAt(row) === files.canonical(hidden)
        verify(found, "the workspace is listed where it now is")
    }

    function test_the_interface_is_set_in_the_typeface_the_application_carries() {
        launch(freshMachine())
        verify(Qt.fontFamilies().indexOf("IBM Plex Sans") >= 0, "the bundled typeface is available")
        compare(app.font.family, "IBM Plex Sans")
        compare(app.font.pointSize, 11)
    }

    // New or Open while a workspace is open: what shows the workspace starts over.
    function test_another_workspace_replaces_the_open_one_everywhere() {
        const machine = freshMachine()
        launch(machine)
        createWorkspace("Main")
        const card = machinePath(machine) + "/Card"
        files.copyDir(home + "/Template2", card)
        addSource(card)
        waitForTheScan()
        app.currentTask = "cull"
        tryCompare(app.photos, "count", 2)
        compare(app.sources.count, 1)
        mouseClick(app.library.grid.itemAtIndex(0))
        keyClick(Qt.Key_3)
        tryCompare(History, "undoKind", "rating")

        createWorkspace("Second")
        compare(History.undoKind, "", "another workspace has a history of its own")
        compare(app.launcher.workspaceName, "Second")
        compare(app.sources.count, 0, "the sources are the new workspace's")
        compare(app.photos.count, 0, "so are the photos")
        compare(app.currentTask, "catalogue", "an empty workspace opens on what fills it")

        app.openFolder(machinePath(machine) + "/Pictures/Auroraw/Main")
        compare(app.launcher.workspaceName, "Main")
        compare(app.sources.count, 1)
        tryCompare(app.photos, "count", 2)
        compare(app.currentTask, "cull")
    }
}
