// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// A camera card that was inserted while a workspace is open: "Card detected", with a way to import from
// it (spec §5.2, one click). A card that was in already when the workspace opened is not announced; it
// is listed in the Import dialog all the same.
Rectangle {
    id: banner
    required property var form
    required property var host
    // The card that was noticed, and how the banner names it.
    property string cardPath: ""
    property string cardName: ""
    property alias importButton: importButton
    property alias ignoreButton: ignoreButton
    // The mount points seen at the last look.
    property var known: []
    signal importRequested(string path)

    visible: cardPath !== ""
    implicitHeight: visible ? 44 : 0
    // D-129: retuned to the `accentSecondary` teal family, in the same cool-neutral world as the surfaces.
    color: "#183936"
    border.color: "#2a6f68"
    border.width: 1

    function volumes() {
        try {
            return JSON.parse(form.volumes())
        } catch (e) {
            return []
        }
    }

    // What is mounted now is not news.
    function rememberCurrent() {
        cardPath = ""
        known = volumes().map(v => v.path)
    }

    function look() {
        const now = volumes()
        const fresh = now.find(v => v.hasDcim && known.indexOf(v.path) < 0)
        if (fresh) {
            cardPath = fresh.path
            cardName = fresh.name
        }
        known = now.map(v => v.path)
    }

    Timer {
        interval: 2000
        repeat: true
        running: banner.host.inWorkspace
        onTriggered: banner.look()
    }

    RowLayout {
        anchors.fill: parent
        anchors.margins: 6
        spacing: 8
        Label {
            Layout.fillWidth: true
            text: qsTr("Card detected: %1").arg(banner.cardName)
            color: "white"
            elide: Text.ElideRight
        }
        AppButton {
            id: importButton
            text: qsTr("Import…")
            highlighted: true
            enabled: !banner.host.dialogOpen
            onClicked: {
                const path = banner.cardPath
                banner.cardPath = ""
                banner.importRequested(path)
            }
        }
        AppButton {
            id: ignoreButton
            text: qsTr("Ignore")
            onClicked: banner.cardPath = ""
        }
    }
}
