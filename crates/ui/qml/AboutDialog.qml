// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// What the application is, its licence, and what it is made with.
AppDialog {
    id: dialog
    required property var launcher
    preferredWidth: 480
    title: qsTr("About Auroraw")

    contentItem: ColumnLayout {
        spacing: 16
        Label {
            text: qsTr("Auroraw %1").arg(dialog.launcher.version())
            font.pixelSize: 18
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: qsTr("A free application for photographers: organise photos, develop RAW files and deliver galleries.")
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            color: Theme.quiet
            text: qsTr("Free software, GNU General Public License 3.0 or later.")
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            color: Theme.quiet
            text: qsTr("Made with Qt, used under the GNU Lesser General Public License 3.0, and Rust.")
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            color: Theme.quiet
            text: qsTr("Set in IBM Plex Sans, under the SIL Open Font License 1.1.")
        }
        Label {
            text: "https://auroraw.org"
            // D-127: the second, sparing accent — a link is exactly the rare highlight it's for.
            color: Theme.accentSecondary
        }
    }

    footer: AppDialogButtonBox {
        AppButton {
            text: qsTr("Close")
            highlighted: true
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            onClicked: dialog.close()
        }
    }
}
