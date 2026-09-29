// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import org.auroraw.ui
import QtQuick.Layouts

// New workspace: a real modal Dialog, and the system's folder dialog.
AppDialog {
    id: dialog
    required property var launcher
    property string error: ""
    property var hostWindow: null
    signal created
    property alias nameField: nameField
    property alias folderField: folderField
    property alias createButton: createButton
    property alias cancelButton: cancelButton
    // A native folder dialog is open (its own window: the main window has to be blocked by hand).
    property alias browsing: browse.visible
    property alias picker: browse

    title: qsTr("New workspace")

    function openWith() {
        var parentFolder = launcher.defaultParent()
        nameField.text = launcher.freeName(parentFolder, qsTr("Main"))
        folderField.text = parentFolder
        error = ""
        open()
        nameField.forceActiveFocus()
    }

    function tryCreate() {
        if (nameField.text.trim() === "") {
            error = qsTr("Give the workspace a name.")
            return
        }
        var reason = launcher.create(nameField.text, folderField.text)
        if (reason === "") {
            close()
            created()
        } else if (reason.indexOf("taken:") === 0) {
            error = qsTr("The folder %1 already exists and is not empty. Choose another name or folder.")
                .arg(reason.substring(6))
        } else {
            error = qsTr("Cannot create the workspace: %1").arg(reason)
        }
    }

    contentItem: GridLayout {
        columns: 3
        columnSpacing: 12
        rowSpacing: 14

        Label { text: qsTr("Name") }
        TextField {
            id: nameField
            objectName: "nameField"
            Layout.fillWidth: true
            Layout.columnSpan: 2
            onAccepted: dialog.tryCreate()
        }
        Label { text: qsTr("Folder") }
        TextField {
            id: folderField
            objectName: "folderField"
            Layout.fillWidth: true
        }
        AppButton {
            text: qsTr("Browse…")
            onClicked: browse.pick()
        }
        Label {
            Layout.columnSpan: 3
            Layout.fillWidth: true
            text: {
                var where = dialog.launcher.preview(nameField.text, folderField.text)
                return where === "" ? "" : qsTr("Workspace folder: %1").arg(where)
            }
            color: Theme.quiet
            wrapMode: Text.Wrap
        }
        Label {
            Layout.columnSpan: 3
            Layout.fillWidth: true
            visible: dialog.error !== ""
            text: dialog.error
            color: Theme.danger
            wrapMode: Text.Wrap
        }
    }

    footer: AppDialogButtonBox {
        AppButton {
            id: createButton
            text: qsTr("Create")
            highlighted: true
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            onClicked: dialog.tryCreate()
        }
        AppButton {
            id: cancelButton
            text: qsTr("Cancel")
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            onClicked: dialog.close()
        }
    }

    FolderPicker {
        id: browse
        field: folderField
        hostWindow: dialog.hostWindow
        title: qsTr("Choose the workspace folder")
    }
}
