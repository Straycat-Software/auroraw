// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// Changes another application made to the XMP files beside originals (spec §5.7, D-047; WP10): "12 photos have
// metadata changed by another application", noticed by a scan and waiting for an answer. Review… opens the list
// where each can be accepted or declined; Ignore declines them all (the files are remembered as they stand, and
// are reported again only if they change again). Shown whenever something waits, also when a workspace opens.
AppBanner {
    id: banner
    required property var external
    required property var host
    // How many photos have a change waiting (the engine's word, via the bus and at workspace open).
    property int count: 0
    property alias reviewButton: reviewButton
    property alias ignoreButton: ignoreButton
    signal reviewRequested()

    visible: count > 0
    implicitHeight: visible ? 44 : 0

    RowLayout {
        anchors.fill: parent
        anchors.margins: 6
        spacing: 8
        Label {
            Layout.fillWidth: true
            text: qsTr("%n photo(s) have metadata changed by another application", "", banner.count)
            color: Theme.white
            elide: Text.ElideRight
        }
        AppButton {
            id: reviewButton
            text: qsTr("Review…")
            highlighted: true
            enabled: !banner.host.dialogOpen
            onClicked: banner.reviewRequested()
        }
        AppButton {
            id: ignoreButton
            text: qsTr("Ignore")
            onClicked: banner.external.ignoreAll()
        }
    }
}
