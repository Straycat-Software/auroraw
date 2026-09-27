// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// The duplicates report (WP9, D-036, D-108): every photo Auroraw has found at more than one confirmed location,
// read-only. Nothing here deletes, merges or picks a copy to keep — Auroraw never does that itself; "Show in file
// manager" beside each location is what lets a person go tidy up outside it.
AppDialog {
    id: dialog
    required property var duplicates
    property alias list: list

    preferredWidth: 640
    title: qsTr("Duplicate photos")

    property var entries: []

    function refresh() { entries = JSON.parse(duplicates.list()) }
    onAboutToShow: refresh()

    contentItem: ColumnLayout {
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
            Layout.preferredHeight: 360
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
    }

    footer: DialogButtonBox {
        AppButton {
            text: qsTr("Close")
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            onClicked: dialog.close()
        }
    }
}
