// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// The catalogue task: the sources (folders whose photos are in the catalogue) and what a scan or a
// removal is doing. Adding one copies nothing. A new workspace opens on it, because it is where a
// workspace gets its photos. `CatalogueFlow` does the work; this shows it.
Item {
    id: root
    required property var flow
    required property var sources
    property alias list: list
    property alias addButton: addButton

    ColumnLayout {
        anchors.horizontalCenter: parent.horizontalCenter
        width: Math.min(900, parent.width - 64)
        y: 24
        spacing: 12

        RowLayout {
            Layout.fillWidth: true
            spacing: 12
            Label {
                text: qsTr("Sources")
                font.pixelSize: 24
                font.bold: true
                Layout.fillWidth: true
            }
            AppButton {
                id: addButton
                text: qsTr("Add a source…")
                highlighted: true
                enabled: !root.flow.busy
                onClicked: root.flow.openAdd()
            }
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            color: Theme.quiet
            text: qsTr("A source is a folder whose photos are in the catalogue. Adding one copies nothing: the photos stay where they are.")
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            visible: root.sources.count === 0
            color: Theme.warning
            text: qsTr("No source yet. Add the folder your photos are in.")
        }
        ProgressBar {
            Layout.fillWidth: true
            visible: root.flow.busy
            value: root.flow.progress
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            visible: root.flow.status !== ""
            text: root.flow.status
            color: Theme.quiet
        }
        AppListFrame {
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(root.sources.count * 68, 560)
            // The rows are already sunken cards of their own (`color: root.palette.dark`, below):
            // the frame stays at the canvas tone so it reads as a boundary, not a second well.
            fill: Theme.surface.window

            ListView {
                id: list
                anchors.fill: parent
                clip: true
                spacing: 4
                boundsBehavior: Flickable.StopAtBounds
                model: root.sources
                ScrollBar.vertical: AppScrollBar {}
                delegate: Rectangle {
                    id: row
                    required property int index
                    required property string name
                    required property string path
                    required property bool online
                    required property int photos
                    property alias rescanButton: rescanButton
                    property alias removeButton: removeButton
                    width: ListView.view.width
                    height: 64
                    color: root.palette.dark

                    RowLayout {
                        anchors.fill: parent
                        anchors.margins: 8
                        spacing: 12
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 2
                            RowLayout {
                                spacing: 8
                                Label {
                                    text: row.name
                                    font.bold: true
                                    elide: Text.ElideRight
                                }
                                Label {
                                    visible: !row.online
                                    text: qsTr("Offline")
                                    color: Theme.danger
                                }
                            }
                            Label {
                                Layout.fillWidth: true
                                text: row.path
                                color: Theme.quiet
                                elide: Text.ElideMiddle
                            }
                        }
                        Label { text: qsTr("%n photo(s)", "", row.photos) }
                        AppButton {
                            id: rescanButton
                            text: qsTr("Rescan")
                            Accessible.name: qsTr("Rescan: %1").arg(row.name)
                            enabled: !root.flow.busy && row.online
                            onClicked: root.flow.rescan(row.index)
                        }
                        AppButton {
                            id: removeButton
                            text: qsTr("Remove")
                            Accessible.name: qsTr("Remove: %1").arg(row.name)
                            enabled: !root.flow.busy
                            onClicked: root.flow.askRemove(row.index, row.name)
                        }
                    }
                }
            }
        }
    }
}
