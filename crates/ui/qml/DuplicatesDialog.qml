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
    property var hostWindow: null
    property alias list: list
    property alias exportButton: exportButton
    property alias exportDialog: exportDialog
    // The resize handle, for the tests to drag (`KeywordPanel.qml`'s own `edge` alias).
    property alias resizeCorner: corner
    readonly property bool browsing: exportDialog.visible

    preferredWidth: 640
    resizable: true
    title: qsTr("Duplicate photos")

    property var entries: []

    function refresh() { entries = JSON.parse(duplicates.list()) }
    onAboutToShow: refresh()

    // A plain `Item`, not the `ColumnLayout` directly (D-112): a generic resize handle in
    // `AppDialog.qml` would end up in `contentData`, never part of what is actually drawn, once a
    // dialog gives its own `contentItem` — so the handle lives here instead, in this same tree,
    // overlaid on the bottom-right corner of the list.
    contentItem: Item {
        implicitWidth: column.implicitWidth
        implicitHeight: column.implicitHeight

        ColumnLayout {
            id: column
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

        // Drag to resize the dialog (D-112, issue #12): the same coordinate-mapped, minimum-clamped
        // shape `KeywordPanel.qml`'s own `edge` already uses for its own drag.
        MouseArea {
            id: corner
            z: 1
            width: 18
            height: 18
            x: parent.width - width
            y: parent.height - height
            hoverEnabled: true
            cursorShape: Qt.SizeFDiagCursor
            property real startX: 0
            property real startY: 0
            property real startWidth: 0
            property real startHeight: 0
            onPressed: mouse => {
                const global = mapToItem(null, mouse.x, mouse.y)
                startX = global.x
                startY = global.y
                startWidth = dialog.width
                startHeight = dialog.height
            }
            onPositionChanged: mouse => {
                if (!pressed)
                    return
                const global = mapToItem(null, mouse.x, mouse.y)
                const maxWidth = (Overlay.overlay ? Overlay.overlay.width : startWidth) - 32
                const maxHeight = (Overlay.overlay ? Overlay.overlay.height : startHeight) - 32
                dialog.width = Math.max(dialog.minWidth, Math.min(maxWidth, Math.round(startWidth + global.x - startX)))
                dialog.height = Math.max(dialog.minHeight, Math.min(maxHeight, Math.round(startHeight + global.y - startY)))
            }
            Rectangle {
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                anchors.margins: 4
                width: 8
                height: 8
                color: "transparent"
                border.color: Theme.quiet
                border.width: 2
            }
        }
    }

    footer: DialogButtonBox {
        AppButton {
            id: exportButton
            text: qsTr("Export the list…")
            enabled: dialog.entries.length > 0
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Writes the report to a text file, the same one the CLI's own duplicates command prints")
            onClicked: exportDialog.pick()
        }
        AppButton {
            text: qsTr("Close")
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            onClicked: dialog.close()
        }
    }

    FileSaveDialog {
        id: exportDialog
        hostWindow: dialog.hostWindow
        defaultFileName: "duplicates.txt"
        onChosen: path => dialog.duplicates.exportTo(path)
    }
}
