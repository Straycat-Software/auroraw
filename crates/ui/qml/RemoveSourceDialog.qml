// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// Taking a source out of the catalogue: what leaves, and that nothing on disk is touched.
AppDialog {
    id: dialog
    property string sourceName: ""
    property int photos: 0
    property int workedOn: 0
    property alias removeButton: removeButton
    signal confirmed

    title: qsTr("Remove the source \"%1\"?").arg(sourceName)

    contentItem: ColumnLayout {
        spacing: 16
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: qsTr("%n photo(s) leave(s) the catalogue.", "", dialog.photos)
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            visible: dialog.workedOn > 0
            color: Theme.warning
            text: qsTr("%n of them has a rating, keywords, a title or a version.", "", dialog.workedOn)
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            color: Theme.quiet
            text: qsTr("Their data is kept in the workspace's removed folder and comes back if you add this folder again. The photo files themselves are never touched.")
        }
    }

    footer: AppDialogButtonBox {
        AppButton {
            id: removeButton
            text: qsTr("Remove")
            highlighted: true
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            onClicked: dialog.confirmed()
        }
        AppButton {
            text: qsTr("Cancel")
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            onClicked: dialog.close()
        }
    }
}
