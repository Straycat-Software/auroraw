// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// The scan found photos that were removed earlier: restore them with their work, or add their files as
// new photos. Closing it any other way (Escape) stops the scan; the source stays and a rescan picks up.
AppDialog {
    id: dialog
    property int restorable: 0
    property int total: 0
    property alias restoreButton: restoreButton
    property alias newButton: newButton
    // The answer: true to restore. `cancelled` when the dialog was closed without one.
    signal chosen(bool restore)
    signal cancelled
    property bool answered: false

    preferredWidth: 560
    title: qsTr("Photos removed earlier")

    onAboutToShow: answered = false
    onClosed: if (!answered) cancelled()

    contentItem: ColumnLayout {
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: qsTr("%1 of the %2 photos in this folder were in the catalogue before, with their ratings, keywords and versions.")
                .arg(dialog.restorable).arg(dialog.total)
        }
    }

    footer: AppDialogButtonBox {
        AppButton {
            id: newButton
            text: qsTr("Add them as new photos")
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            onClicked: { dialog.answered = true; dialog.close(); dialog.chosen(false) }
        }
        AppButton {
            id: restoreButton
            text: qsTr("Restore them")
            highlighted: true
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            onClicked: { dialog.answered = true; dialog.close(); dialog.chosen(true) }
        }
    }
}
