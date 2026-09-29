// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// What opens when no workspace is: the ones this machine knows, and the two ways to get another.
Item {
    id: root
    required property var launcher
    required property var known
    signal newRequested
    signal openRequested
    signal knownRequested(int row)
    property alias newButton: newButton
    property alias openButton: openButton
    property alias list: list
    // The launcher's note, as a sentence.
    readonly property string noteText: {
        const note = root.launcher.note
        if (note.indexOf("lost:") === 0)
            return qsTr("The last workspace could not be found: %1").arg(note.substring(5))
        if (note.indexOf("open:") === 0) {
            const parts = note.substring(5).split("\t")
            return qsTr("Cannot open %1: %2").arg(parts[0]).arg(parts[1])
        }
        return ""
    }

    ColumnLayout {
        anchors.horizontalCenter: parent.horizontalCenter
        width: Math.min(720, parent.width - 64)
        y: 32
        spacing: 14

        Label {
            text: qsTr("Welcome to Auroraw")
            font.pixelSize: 24
            font.bold: true
        }
        Label {
            text: qsTr("Open a workspace, or create a new one.")
            color: Theme.quiet
        }
        Label {
            visible: root.noteText !== ""
            text: root.noteText
            color: Theme.warning
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }
        RowLayout {
            spacing: 8
            AppButton {
                id: newButton
                text: qsTr("New workspace…")
                highlighted: true
                onClicked: root.newRequested()
            }
            AppButton {
                id: openButton
                text: qsTr("Open workspace…")
                onClicked: root.openRequested()
            }
        }
        Label {
            text: root.known.count > 0 ? qsTr("Recent workspaces") : qsTr("No workspace yet.")
            font.bold: true
        }
        AppListFrame {
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(root.known.count * 60, 380)

            ListView {
                id: list
                anchors.fill: parent
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                model: root.known
                ScrollBar.vertical: AppScrollBar { id: vbar }
                delegate: ItemDelegate {
                    id: row
                    required property int index
                    required property string name
                    required property string path
                    required property string opened
                    required property bool found
                    width: ListView.view.width - vbar.width
                    height: 56
                    Accessible.name: name + ", " + path
                    onClicked: root.knownRequested(index)

                    contentItem: RowLayout {
                        spacing: 8
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 0
                            Label {
                                text: row.name
                                font.bold: true
                                elide: Text.ElideRight
                                Layout.fillWidth: true
                            }
                            Label {
                                text: row.path
                                color: Theme.quiet
                                elide: Text.ElideMiddle
                                Layout.fillWidth: true
                            }
                        }
                        Label {
                            visible: !row.found
                            text: qsTr("Not found")
                            color: Theme.danger
                        }
                        AppButton {
                            visible: !row.found
                            text: qsTr("Remove from the list")
                            Accessible.name: qsTr("Remove from the list: %1").arg(row.name)
                            onClicked: root.known.forget(row.index)
                        }
                        Label {
                            visible: row.found
                            text: row.opened
                            color: Theme.quiet
                        }
                    }
                }
            }
        }
    }
}
