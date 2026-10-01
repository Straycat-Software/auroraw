// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// What the application is, its licence, and what it is made with.
AppDialog {
    id: dialog
    required property var launcher
    // The place names (`PlaceNames`), to say whether the data they are looked up in is here.
    property var placeNames: null
    property bool placesInstalled: false
    property alias placesLabel: placesLabel
    preferredWidth: 480
    title: qsTr("About Auroraw")

    onAboutToShow: dialog.placesInstalled = dialog.placeNames ? dialog.placeNames.installed() : false

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
        // The credit the data asks for (design note 008 §2): GeoNames' licence asks for it, Natural Earth's does not and
        // is named all the same; the choice of boundaries is said here, as the note decided.
        Label {
            id: placesLabel
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            color: Theme.quiet
            visible: dialog.placesInstalled
            text: qsTr("Place names: towns adapted from GeoNames (Creative Commons Attribution 4.0, https://creativecommons.org/licenses/by/4.0/; https://www.geonames.org/), countries and regions from Natural Earth (public domain), with its boundaries as they stand on the ground, in its default worldview. Auroraw takes no position on a disputed border. A name you correct is never overwritten. The data is not code, and the GPL is not its licence.")
        }
        Label {
            text: "https://auroraw.org"
            // D-129: the second, sparing accent — a link is exactly the rare highlight it's for.
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
