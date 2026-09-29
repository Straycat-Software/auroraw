// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// The similar photos panel (WP9, D-034, D-105), between the grid and the keyword panel: the photos that look like the
// one under the cursor and were taken near it, nearest first, as thumbnails with how close they are. They are only
// suggested: a click goes to a photo in the grid, and the buttons group them with this photo as one series (undone
// with Ctrl+Z) or compare them. The photos come from a hash of each thumbnail that is made in the background
// (`similarPending` says how many are not yet), so a library just opened fills in.
Rectangle {
    id: panel
    required property var library
    required property var photoGrid
    required property var launcher

    // The photo the suggestions are for (the cursor's), and the suggestions: {id, distance} of each, nearest first.
    property string reference: ""
    property var similar: []
    property int pending: 0
    readonly property int count: similar.length
    property alias thumbs: thumbList
    property alias groupButton: groupButton
    property alias compareButton: compareButton

    readonly property int panelWidth: 264
    Layout.preferredWidth: visible ? panelWidth : 0
    Layout.fillHeight: true
    color: palette.window

    // Asks again what looks like the photo under the cursor.
    function refresh() {
        const row = panel.library.grid.currentIndex
        reference = row >= 0 ? photoGrid.idAt(row) : ""
        pending = photoGrid.similarPending()
        if (reference === "") {
            similar = []
            return
        }
        const text = photoGrid.similarTo(reference, launcher.intOption("similarDistance"),
                                         launcher.intOption("similarMinutes"))
        similar = text === "" ? [] : text.split(",").map(pair => {
            const parts = pair.split(":")
            return { id: parts[0], distance: parseInt(parts[1]) }
        })
    }

    // How close a photo is, as a share of the hash's 64 bits that agree.
    function closeness(distance) {
        return Math.round((64 - distance) / 64 * 100)
    }

    // The photo and the first `n` suggestions, as the selection (for Group, which acts on it as a normal selection).
    function selectWith(n) {
        panel.photoGrid.selectIds([panel.reference].concat(panel.similar.slice(0, n).map(s => s.id)).join(","))
        panel.library.updateSummary()
    }

    // The photo and its first `n` suggestions, as a plain list of identifiers (for Compare, which must show exactly
    // these `n` + 1 photos: `selectWith` would do, since a collapsed series' extra members would then be added in,
    // more or fewer photos than the panel promised).
    function idsWith(n) {
        return [panel.reference].concat(panel.similar.slice(0, n).map(s => s.id)).join(",")
    }

    // The file name of a suggested photo, for its tooltip: its own (`rowOf` then `infoAt` would give a closed
    // series' cover's name instead, when the suggestion is one of its other members).
    function filenameOf(id) {
        return panel.photoGrid.filenameOf(id)
    }

    onVisibleChanged: if (visible)
        refresh()

    Timer {
        id: soon
        interval: 150
        onTriggered: panel.refresh()
    }
    // It follows the cursor, and the list when a series is made or the filters change.
    Connections {
        target: panel.library.grid
        enabled: panel.visible
        function onCurrentIndexChanged() { soon.restart() }
    }
    Connections {
        target: panel.photoGrid
        enabled: panel.visible
        function onSeriesCountChanged() { soon.restart() }
        function onCountChanged() { soon.restart() }
    }
    // While hashes are still being made the list is not complete: look again.
    Timer {
        interval: 1500
        repeat: true
        running: panel.visible && panel.pending > 0
        onTriggered: panel.refresh()
    }

    Rectangle {
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: 1
        color: Theme.surface.sunken
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 8
        spacing: 6

        RowLayout {
            Layout.fillWidth: true
            Label {
                text: qsTr("Similar photos")
                font.bold: true
                Layout.fillWidth: true
            }
            ToolButton {
                id: closeButton
                text: "✕"
                focusPolicy: Qt.NoFocus
                Accessible.name: qsTr("Close the similar photos")
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Close (M)")
                onClicked: panel.library.showSimilar(false)
            }
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            color: Theme.quiet
            text: panel.reference === "" ? qsTr("Put the cursor on a photo.")
                  : panel.count > 0 ? qsTr("%n similar photo(s)", "", panel.count)
                  : panel.pending > 0 ? qsTr("Analysing %n photo(s)…", "", panel.pending)
                  : qsTr("No similar photo near this one.")
        }
        Label {
            Layout.fillWidth: true
            visible: panel.count > 0 && panel.pending > 0
            wrapMode: Text.WordWrap
            color: Theme.quiet
            text: qsTr("Analysing %n photo(s)…", "", panel.pending)
        }

        GridView {
            id: thumbList
            Layout.fillWidth: true
            Layout.rightMargin: vbar.width
            // Only as tall as its own rows need, so the buttons sit right under the thumbnails instead of at the
            // bottom of the panel (a handful of suggestions rarely fill it).
            Layout.preferredHeight: Math.max(1, Math.ceil(panel.count / 2)) * cellHeight
            Layout.maximumHeight: parent.height - 140
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            model: panel.similar
            cellWidth: Math.floor(width / 2)
            cellHeight: cellWidth * 3 / 4 + 4
            ScrollBar.vertical: AppScrollBar { id: vbar }
            delegate: Item {
                id: entry
                required property var modelData
                required property int index
                width: thumbList.cellWidth
                height: thumbList.cellHeight
                Rectangle {
                    anchors.fill: parent
                    anchors.margins: 2
                    color: "#1c1d21"
                    border.width: hover.containsMouse ? 2 : 0
                    border.color: Theme.accent
                    Image {
                        anchors.fill: parent
                        anchors.margins: 2
                        source: "image://thumbs/" + entry.modelData.id
                        fillMode: Image.PreserveAspectFit
                        asynchronous: true
                    }
                    Label {
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        anchors.margins: 3
                        padding: 2
                        font.pixelSize: 11
                        text: panel.closeness(entry.modelData.distance) + " %"
                        background: Rectangle { color: "#b0000000"; radius: Theme.radiusControl }
                    }
                    MouseArea {
                        id: hover
                        anchors.fill: parent
                        hoverEnabled: true
                        ToolTip.visible: containsMouse
                        ToolTip.text: panel.filenameOf(entry.modelData.id)
                        onClicked: {
                            const row = panel.photoGrid.rowOf(entry.modelData.id)
                            if (row >= 0)
                                panel.library.goTo(row, 0)
                        }
                    }
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            AppButton {
                id: groupButton
                Layout.fillWidth: true
                text: qsTr("Group with this photo")
                focusPolicy: Qt.NoFocus
                enabled: panel.count > 0 && !panel.library.inResolvedSeries
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Makes one series of this photo and the similar ones")
                onClicked: {
                    panel.selectWith(panel.count)
                    panel.library.group()
                    soon.restart()
                }
            }
            AppButton {
                id: compareButton
                text: qsTr("Compare")
                focusPolicy: Qt.NoFocus
                enabled: panel.count > 0
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Compares this photo with the three nearest")
                onClicked: panel.library.compareOn(panel.idsWith(3))
            }
        }
        Item { Layout.fillWidth: true; Layout.fillHeight: true }
    }
}
