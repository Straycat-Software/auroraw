// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.ui

// What every suite of the interface needs: the application window made on a machine of its own (a
// folder under AURORAW_TEST_HOME), a few file helpers, and the window put away after each test.
// (QML files are not types of the module for an outside importer: `Main.qml` is loaded by URL.)
TestCase {
    id: tc
    when: windowShown

    property var appComponent: Qt.createComponent("qrc:/qt/qml/org/auroraw/ui/qml/Main.qml")
    property var app: null
    property int machineCount: 0
    property alias files: files

    Files { id: files }

    readonly property string home: files.env("AURORAW_TEST_HOME")

    function machinePath(name) { return home + "/" + name }
    function freshMachine() { machineCount++; return "m" + machineCount }

    // The application on `machine`, once it has started.
    function launch(machine) {
        quit()
        app = createTemporaryObject(appComponent, tc, { machine: machine })
        verify(app, "the window was made: " + appComponent.errorString())
        wait(300)
        app.requestActivate()
        wait(100)
        return app
    }

    // The application on the shared machine, with the window widened for the keyword panel, once its grid lists
    // `photos` photos. A slow runner (Windows) sometimes has not let go of the workspace of the window before, and
    // the new one opens nothing: it is then closed and made again.
    function launchWithPhotos(photos) {
        for (let attempt = 0; attempt < 3; attempt++) {
            launch("")
            app.width = 1680
            const deadline = Date.now() + 12000
            while (app.photos.count !== photos && Date.now() < deadline)
                wait(100)
            if (app.photos.count === photos)
                return
            quit()
            wait(1000)
        }
        compare(app.photos.count, photos, "the machine's photos were listed")
    }

    function quit() {
        if (app) {
            app.close()
            app.destroy()
            app = null
            wait(150)
        }
    }

    // The translation is the process's, not the window's: a test that changed the language must not
    // leave it to the next one.
    function cleanup() {
        if (app)
            app.launcher.chooseLanguage("en")
        quit()
    }

    // Moves a folder that a workspace has just closed. Windows keeps it until the event loop has
    // destroyed the window's objects, so this retries between turns of the loop.
    function move(from, to) {
        tryVerify(() => files.exists(to) || files.rename(from, to), 10000, "the folder could be moved")
    }

    // Writes what the window shows to `<AUR_SNAPSHOT_DIR>/<name>.png`, when that variable names a
    // folder (to look at what a run draws).
    function snapshot(name) {
        const root = files.env("AUR_SNAPSHOT_DIR")
        if (root === "")
            return
        const dir = root + "/views"
        files.mkdir(dir)
        // A dialog that is a real secondary window (D-113, `AppWindow.qml`: the duplicates report and the
        // external changes review) is not part of the main window's `contentItem` or `overlay` at all: grab
        // it on its own instead.
        const secondary = [app.duplicatesDialog, app.externalDialog].find(d => d && d.visible)
        if (secondary)
            grabImage(secondary.contentItem).save(dir + "/" + name + ".png")
        else
            grabImage(app.contentItem).save(dir + "/" + name + ".png")
        // Dialogs and menus live in the overlay, above the content.
        if (app.overlay)
            grabImage(app.overlay).save(dir + "/" + name + "-overlay.png")
    }

    // Adds `folder` as a source through the catalogue task's dialog, as a person does.
    function addSource(folder, name) {
        app.currentTask = "catalogue"
        click(app.catalogue.addButton)
        wait(150)
        verify(app.flow.addDialog.visible, "the Add a source dialog opened")
        app.flow.addDialog.folderField.text = folder
        if (name)
            app.flow.addDialog.nameField.text = name
        click(app.flow.addDialog.addButton)
    }

    function waitForTheScan() {
        tryVerify(() => !app.flow.busy, 30000, "the scan or removal ended")
    }

    // Waits until a frame has been drawn with `item` and what it shows in it. A banner, a popup or a window that
    // has just been shown is `visible` at once, but its contents are given their place by the first frame: a click
    // sent before that lands where the button was, and is lost for good (waiting longer does not bring it back).
    // `waitForRendering` alone waits for a frame *after* the call, and times out when one has just been drawn, so a
    // frame is asked for first. (Issue #27: the first "Review…" click of tst_external.qml on the Windows runner.)
    function drawn(item) {
        item.update()
        if (item.Window.window)
            item.Window.window.update()
        verify(waitForRendering(item), "a frame was drawn with " + item + " in it")
    }

    // Chooses the interface's language and returns once it is on the screen. The launcher retranslates at once (`language`
    // says which is chosen), but what the new texts do to the layout (a label that needs more room, a row that wraps)
    // happens at the next frame: a test that measures it reads after a frame has been drawn, not after a duration. (Issue
    // #73: of the 26 pauses that followed a change of language, 24 held nothing up and 2 held up a measure of the layout,
    // the width of the Collections panel's tabs and that of the Import dialog's label column.)
    function useLanguage(code) {
        app.launcher.chooseLanguage(code)
        tryCompare(app.launcher, "language", code)
        drawn(app.contentItem)
    }

    // Waits until the review of external changes lists `count` photos. (Not `tryCompare(app.externalDialog.entries,
    // "length", count)`: `entries` is a new array at each refresh, and `tryCompare` reads the array once, when it is
    // called, so it keeps waiting on a list nobody updates; the test passed whenever the engine answered within the
    // pause of a click, and on a slow macOS runner it did not: issue #63. Nor `tryVerify` with a message: the message
    // is built before the wait, and would say what the window listed at the start.)
    function tryEntries(count) {
        const deadline = Date.now() + 5000
        while (app.externalDialog.entries.length !== count && Date.now() < deadline)
            wait(20)
        compare(app.externalDialog.entries.length, count, "the review lists the wrong number of photos")
    }

    // Clicks the centre of an item as a person would.
    function click(item) {
        mouseClick(item)
        wait(60)
    }

    // The workspace folder a machine's New workspace dialog proposes for `name`.
    function workspaceFolder(machine, name) { return machinePath(machine) + "/Pictures/Auroraw/" + name }

    // Creates a workspace through the dialog, as a person does.
    function createWorkspace(name) {
        app.newDialog.openWith()
        wait(200)
        app.newDialog.nameField.text = name
        click(app.newDialog.createButton)
        wait(200)
    }
}
