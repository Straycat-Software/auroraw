// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// The duplicates report (WP9, D-036, D-108): every photo Auroraw has found at more than one confirmed location,
// read-only. Nothing here deletes, merges or picks a copy to keep — Auroraw never does that itself; "Show in file
// manager" beside each location is what lets a person go tidy up outside it.
//
// A real secondary window (D-113), not an `AppDialog`: the list can get long, and a real window's own border
// resizes it for free, native title bar included, instead of a hand-rolled drag handle fighting a Popup's
// `contentItem`.
AppWindow {
    id: dialog
    required property var duplicates
    property alias list: list
    property alias exportButton: exportButton
    property alias exportDialog: exportDialog
    readonly property bool browsing: exportDialog.visible

    title: qsTr("Duplicate photos")

    property var entries: []

    function refresh() { entries = JSON.parse(duplicates.list()) }
    onVisibleChanged: if (dialog.visible) dialog.refresh()

    ColumnLayout {
        anchors.fill: parent
        spacing: 8
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            color: Theme.quiet
            text: dialog.entries.length > 0
                  ? qsTr("%n photo(s) found at more than one place. Auroraw never deletes anything itself: use “Show in file manager” to go tidy up.", "", dialog.entries.length)
                  : qsTr("No duplicate photo found.")
        }
        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: dialog.entries
            spacing: 6
            ScrollBar.vertical: ScrollBar {}
            delegate: Rectangle {
                id: entry
                required property var modelData
                required property int index
                width: list.width
                height: content.implicitHeight + 16
                color: index % 2 === 0 ? "#00000000" : "#12ffffff"
                RowLayout {
                    id: content
                    anchors.fill: parent
                    anchors.margins: 8
                    spacing: 10
                    Image {
                        Layout.preferredWidth: 64
                        Layout.preferredHeight: 48
                        source: "image://thumbs/" + entry.modelData.id
                        fillMode: Image.PreserveAspectFit
                        asynchronous: true
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        Label {
                            Layout.fillWidth: true
                            font.bold: true
                            elide: Text.ElideRight
                            text: entry.modelData.filename
                        }
                        Repeater {
                            model: [entry.modelData.primary].concat(entry.modelData.extra)
                            RowLayout {
                                required property var modelData
                                Layout.fillWidth: true
                                Label {
                                    Layout.fillWidth: true
                                    elide: Text.ElideMiddle
                                    color: Theme.quiet
                                    text: modelData.sourceName + " — " + modelData.path
                                }
                                ToolButton {
                                    text: qsTr("Show in file manager")
                                    focusPolicy: Qt.NoFocus
                                    onClicked: dialog.duplicates.revealLocation(modelData.sourceId, modelData.path)
                                }
                            }
                        }
                    }
                }
            }
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Item { Layout.fillWidth: true }
            AppButton {
                id: exportButton
                text: qsTr("Export the list…")
                enabled: dialog.entries.length > 0
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Writes the report to a text file, the same one the CLI's own duplicates command prints")
                onClicked: exportDialog.pick()
            }
            AppButton {
                text: qsTr("Close")
                onClicked: dialog.close()
            }
        }
    }

    FileSaveDialog {
        id: exportDialog
        hostWindow: dialog
        defaultFileName: "duplicates.txt"
        rememberAs: "export-duplicates"
        onChosen: path => dialog.duplicates.exportTo(path)
    }
}
