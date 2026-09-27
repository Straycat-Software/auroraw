// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// Adding a folder as a source of the catalogue. It only asks; `CatalogueFlow` does the adding, and
// says here why it cannot (`error`) or that the folder holds other sources to merge (`mergeQuestion`).
AppDialog {
    id: dialog
    property var hostWindow: null
    property string error: ""
    property string mergeQuestion: ""
    property alias folderField: folderField
    property alias nameField: nameField
    property alias addButton: addButton
    property alias mergeButton: mergeButton
    // A native folder dialog is open (its own window: the main window has to be blocked by hand).
    property alias browsing: browse.visible
    property alias picker: browse
    signal addRequested
    signal mergeRequested

    title: qsTr("Add a source")

    // `folder` is where the field starts (issue #10): the last one added, or empty for none yet.
    function openWith(folder) {
        folderField.text = folder === undefined ? "" : folder
        nameField.text = ""
        error = ""
        mergeQuestion = ""
        open()
        folderField.forceActiveFocus()
    }

    contentItem: GridLayout {
        columns: 3
        columnSpacing: 8
        rowSpacing: 10

        Label {
            Layout.columnSpan: 3
            text: qsTr("Kind: Folder (local, or a network share that is mounted)")
            color: Theme.quiet
        }
        Label { text: qsTr("Folder") }
        TextField {
            id: folderField
            Layout.fillWidth: true
            onAccepted: dialog.mergeQuestion === "" ? dialog.addRequested() : dialog.mergeRequested()
        }
        AppButton {
            text: qsTr("Browse…")
            onClicked: browse.pick()
        }
        Label { text: qsTr("Name (optional)") }
        TextField {
            id: nameField
            Layout.fillWidth: true
            Layout.columnSpan: 2
        }
        Label {
            Layout.columnSpan: 3
            Layout.fillWidth: true
            visible: dialog.error !== ""
            text: dialog.error
            color: Theme.danger
            wrapMode: Text.Wrap
        }
        Label {
            Layout.columnSpan: 3
            Layout.fillWidth: true
            visible: dialog.mergeQuestion !== ""
            text: dialog.mergeQuestion
            color: Theme.warning
            wrapMode: Text.Wrap
        }
    }

    footer: DialogButtonBox {
        AppButton {
            id: addButton
            visible: dialog.mergeQuestion === ""
            text: qsTr("Add")
            highlighted: true
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            onClicked: dialog.addRequested()
        }
        AppButton {
            id: mergeButton
            visible: dialog.mergeQuestion !== ""
            text: qsTr("Merge and add")
            highlighted: true
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            onClicked: dialog.mergeRequested()
        }
        AppButton {
            text: qsTr("Cancel")
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            onClicked: dialog.close()
        }
    }

    FolderPicker {
        id: browse
        field: folderField
        hostWindow: dialog.hostWindow
        title: qsTr("Choose the folder to add to the catalogue")
    }
}
