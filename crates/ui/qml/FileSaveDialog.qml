// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Dialogs
import org.auroraw.ui

// The system's save-file dialog (D-106, alongside `FolderPicker`'s own folder dialog): opens the file the answer
// names, whether or not it exists yet. It is a window of its own, which Qt cannot make modal to ours: `Main` covers
// the window while `visible` (see `nativeDialogOpen`).
FileDialog {
    id: picker
    fileMode: FileDialog.SaveFile
    property string defaultFileName: ""
    // A key naming this picker for Folders.last/remember (issue #18): every FileSaveDialog in the
    // app has no field of its own to remember a folder through, unlike some FolderPicker's.
    property string rememberAs: ""
    signal chosen(string path)
    property var hostWindow: null
    parentWindow: hostWindow
    nameFilters: [qsTr("Text files (*.txt)"), qsTr("All files (*)")]

    // A `file:` URL for a path (`file:///C:/photos.txt` on Windows, `file:///home/.../photos.txt` elsewhere).
    function urlFor(path) {
        const slashes = path.replace(/\\/g, "/")
        return encodeURI("file://" + (slashes.charAt(0) === "/" ? "" : "/") + slashes)
    }

    // The path of a `file:` URL, the way the application writes it.
    function pathOf(url) {
        let path = decodeURIComponent(url.toString()).replace(/^file:\/\//, "")
        if (/^\/[A-Za-z]:/.test(path))
            path = path.substring(1)
        return path
    }

    // The folder part of `path` (the last segment, slashes or backslashes), empty if there is none.
    function parentOf(path) {
        const at = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"))
        return at > 0 ? path.substring(0, at) : ""
    }

    function pick() {
        if (rememberAs !== "") {
            const folder = Folders.last(rememberAs)
            if (folder !== "")
                currentFolder = urlFor(folder)
        }
        if (defaultFileName !== "")
            currentFile = urlFor(defaultFileName)
        open()
    }

    // The answer, as the dialog gives it (tests give it directly: no native dialog opens off screen).
    function choose(path) {
        if (rememberAs !== "") {
            const folder = parentOf(path)
            if (folder !== "")
                Folders.remember(rememberAs, folder)
        }
        chosen(path)
    }

    onAccepted: choose(pathOf(selectedFile))
}
