// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// The comparison of 2 to 4 photos side by side (spec §5.3, D-103): the photos selected, or the frames of a series
// in pages of two, three or four. The zoom and the place shown are shared, so that dragging or zooming in one
// frame shows the same detail in all of them. Every frame can be marked to keep (a draft, in memory), then R
// resolves the series: the marked are picked, the others rejected. Rating, flags and labels are the library's
// own, on the frame that has the focus (Left and Right move it); the sharpness of each frame is measured and
// shown as a share of the sharpest's, a suggestion and never a decision. Escape goes back to the grid.
FocusScope {
    id: comparison
    required property var library
    required property var launcher

    readonly property var photos: library.photoGrid
    // Every frame to compare, in the order they are shown, and which page and which pane of it have the focus.
    property var ids: []
    property int pageSize: 2
    property int page: 0
    property int focusSlotIndex: 0
    // The view shared by every pane: fitted, or at `zoom` (1 is the picture's own pixels) about `centre`
    // (as shares of the picture); `viewSerial` says it changed, `viewSource` the pane the user is dragging.
    property bool fit: true
    property real zoom: 1
    property real centreX: 0.5
    property real centreY: 0.5
    property int viewSerial: 0
    property var viewSource: null
    // The quality aids, remembered.
    property bool showPeaking: false
    property bool showClipping: false
    property bool showHistogram: false
    // How sharp each frame is, as a share of the sharpest (identifier to percent, -1 when not measured yet).
    property var ranks: ({})
    property int ranksSerial: 0
    property alias panes: panes
    property alias toolbar: toolbar

    readonly property int pages: Math.max(1, Math.ceil(ids.length / pageSize))
    readonly property var pageIds: ids.slice(page * pageSize, page * pageSize + pageSize)
    readonly property string focusedId: pageIds[Math.min(focusSlotIndex, pageIds.length - 1)] || ""
    readonly property int focusedGlobal: page * pageSize + focusSlotIndex

    function rankOf(id, serial) {
        const rank = ranks[id]
        return rank === undefined ? -1 : rank
    }

    // Opens on these photos (identifiers joined by commas), the first one focused.
    function openOn(idsText) {
        ids = idsText.split(",")
        pageSize = Math.max(2, Math.min(4, launcher.intOption("comparePanes")))
        showPeaking = launcher.viewOption("peaking")
        showClipping = launcher.viewOption("clipping")
        showHistogram = launcher.viewOption("histogram")
        page = 0
        focusSlotIndex = 0
        fit = true
        centreX = 0.5
        centreY = 0.5
        ranks = ({})
        photos.analyse(idsText)
        refreshRanks()
        prefetchPages()
        syncSelection()
        viewSerial++
        forceActiveFocus()
    }

    // The frames of a series are measured in the background: what is known is shown, and asked for again.
    function refreshRanks() {
        const values = JSON.parse(photos.sharpnessRanks(ids.join(",")))
        const next = ({})
        let complete = true
        for (let i = 0; i < ids.length; i++) {
            next[ids[i]] = values[i]
            if (values[i] < 0)
                complete = false
        }
        // A rank only means something once every frame of the series is measured (each is a share of the best).
        ranks = complete ? next : ({})
        ranksSerial++
        return complete
    }

    Timer {
        interval: 500
        repeat: true
        running: comparison.visible && comparison.ids.length > 0
        onTriggered: {
            if (comparison.refreshRanks())
                stop()
        }
    }

    // The pictures of this page and of the next are made ahead.
    function prefetchPages() {
        photos.prefetchIds(ids.slice(page * pageSize, (page + 2) * pageSize).join(","))
    }

    // The library's cursor and selection follow the focused frame, so that a rating, a flag or a label key acts on it.
    function syncSelection() {
        const row = photos.rowOf(focusedId)
        if (row >= 0)
            library.goTo(row, 0)
    }

    function focusSlot(slot) {
        if (slot < 0 || slot >= pageIds.length)
            return
        focusSlotIndex = slot
        syncSelection()
    }

    function focusGlobal(index) {
        const g = Math.max(0, Math.min(ids.length - 1, index))
        const newPage = Math.floor(g / pageSize)
        const turned = newPage !== page
        page = newPage
        focusSlotIndex = g % pageSize
        if (turned) {
            prefetchPages()
            viewSerial++
        }
        syncSelection()
    }

    function turnPage(delta) {
        const target = Math.max(0, Math.min(pages - 1, page + delta))
        if (target === page)
            return
        page = target
        focusSlotIndex = Math.min(focusSlotIndex, pageIds.length - 1)
        prefetchPages()
        syncSelection()
        viewSerial++
    }

    // 2, 3 or 4 frames a page, the focused frame staying on screen.
    function setPageSize(n) {
        const g = focusedGlobal
        pageSize = n
        launcher.setIntOption("comparePanes", n)
        page = Math.floor(g / n)
        focusSlotIndex = g % n
        prefetchPages()
        viewSerial++
    }

    function toggleActual() {
        fit = !fit
        zoom = 1
        centreX = 0.5
        centreY = 0.5
        viewSource = null
        viewSerial++
    }

    // A pane was dragged: the others show the same place.
    function panned(source, cx, cy) {
        centreX = Math.max(0, Math.min(1, cx))
        centreY = Math.max(0, Math.min(1, cy))
        viewSource = source
        viewSerial++
    }

    // Zooms every pane by `factor` about the point (px, py) of `source`'s viewport.
    function zoomAt(source, px, py, factor) {
        const before = fit ? source.fitScale : zoom
        const after = Math.max(0.05, Math.min(8, before * factor))
        const share = source.shareAt(px, py)
        const natural = Math.max(source.picture.implicitWidth, 1)
        const naturalH = Math.max(source.picture.implicitHeight, 1)
        // The point under the pointer stays under it.
        centreX = Math.max(0, Math.min(1, share.x - (px - source.flick.width / 2) / (natural * after)))
        centreY = Math.max(0, Math.min(1, share.y - (py - source.flick.height / 2) / (naturalH * after)))
        zoom = after
        fit = false
        viewSource = null
        viewSerial++
    }

    function setOption(name, on) {
        launcher.setViewOption(name, on)
    }

    function actOnFocused(action) {
        syncSelection()
        action()
    }

    Keys.onPressed: event => {
        if (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))
            return
        const key = event.key
        if (key >= Qt.Key_0 && key <= Qt.Key_5) {
            actOnFocused(() => library.rate(key - Qt.Key_0))
        } else if (key >= Qt.Key_6 && key <= Qt.Key_9) {
            actOnFocused(() => library.label(["red", "yellow", "green", "blue"][key - Qt.Key_6]))
        } else if (key === Qt.Key_P || key === Qt.Key_X || key === Qt.Key_U) {
            actOnFocused(() => library.flag(key === Qt.Key_P ? "pick" : key === Qt.Key_X ? "reject" : "clear"))
        } else if (key === Qt.Key_Left) {
            focusGlobal(focusedGlobal - 1)
        } else if (key === Qt.Key_Right) {
            focusGlobal(focusedGlobal + 1)
        } else if (key === Qt.Key_PageUp) {
            turnPage(-1)
        } else if (key === Qt.Key_PageDown) {
            turnPage(1)
        } else if (key === Qt.Key_K || key === Qt.Key_Return || key === Qt.Key_Enter) {
            photos.toggleMark(focusedId)
        } else if (key === Qt.Key_R) {
            resolve()
        } else if (key === Qt.Key_Z) {
            toggleActual()
        } else if (key === Qt.Key_Plus || key === Qt.Key_Equal) {
            zoomAt(panes.itemAt(focusSlotIndex), panes.itemAt(focusSlotIndex).flick.width / 2,
                   panes.itemAt(focusSlotIndex).flick.height / 2, 1.25)
        } else if (key === Qt.Key_Minus) {
            zoomAt(panes.itemAt(focusSlotIndex), panes.itemAt(focusSlotIndex).flick.width / 2,
                   panes.itemAt(focusSlotIndex).flick.height / 2, 0.8)
        } else if (key === Qt.Key_S) {
            showPeaking = !showPeaking
            setOption("peaking", showPeaking)
        } else if (key === Qt.Key_O) {
            showClipping = !showClipping
            setOption("clipping", showClipping)
        } else if (key === Qt.Key_H) {
            showHistogram = !showHistogram
            setOption("histogram", showHistogram)
        } else if (key === Qt.Key_F || key === Qt.Key_F11) {
            library.fullScreenToggled()
        } else if (key === Qt.Key_Escape) {
            library.closeCompare()
        } else {
            return
        }
        event.accepted = true
    }

    // Resolves the series: the frames marked to keep are picked, the others rejected (the focused frame is kept when
    // nothing is marked), and the comparison closes.
    function resolve() {
        syncSelection()
        if (library.photoGrid.resolveSeries() === 1)
            library.closeCompare()
    }

    Rectangle {
        anchors.fill: parent
        color: "#141414"
    }
    // What a click on the dark between the panes must not do: reach the grid under this view.
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        onPressed: comparison.forceActiveFocus()
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // The tools: frames a page, paging, the aids, zoom, resolving and going back.
        Rectangle {
            id: toolbar
            Layout.fillWidth: true
            Layout.preferredHeight: 40
            color: "#202020"
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 8
                anchors.rightMargin: 8
                spacing: 4
                Label {
                    text: qsTr("Frames:")
                    color: Theme.quiet
                }
                Repeater {
                    id: sizeButtons
                    model: [2, 3, 4]
                    ToolButton {
                        id: sizeButton
                        required property int modelData
                        text: sizeButton.modelData
                        checkable: true
                        autoExclusive: true
                        checked: comparison.pageSize === sizeButton.modelData
                        focusPolicy: Qt.NoFocus
                        Accessible.name: qsTr("%n frame(s) a page", "", sizeButton.modelData)
                        onClicked: comparison.setPageSize(sizeButton.modelData)
                    }
                }
                ToolButton {
                    text: "◀"
                    enabled: comparison.page > 0
                    focusPolicy: Qt.NoFocus
                    Accessible.name: qsTr("Previous page")
                    onClicked: comparison.turnPage(-1)
                }
                Label {
                    text: qsTr("%1 to %2 of %3").arg(comparison.page * comparison.pageSize + 1)
                          .arg(Math.min(comparison.ids.length, (comparison.page + 1) * comparison.pageSize)).arg(comparison.ids.length)
                    color: "#e0e0e0"
                }
                ToolButton {
                    text: "▶"
                    enabled: comparison.page < comparison.pages - 1
                    focusPolicy: Qt.NoFocus
                    Accessible.name: qsTr("Next page")
                    onClicked: comparison.turnPage(1)
                }
                Item { Layout.fillWidth: true }
                ToolButton {
                    text: qsTr("Peaking")
                    checkable: true
                    checked: comparison.showPeaking
                    focusPolicy: Qt.NoFocus
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Focus peaking: what is in focus (S)")
                    onClicked: {
                        comparison.showPeaking = checked
                        comparison.setOption("peaking", checked)
                    }
                }
                ToolButton {
                    text: qsTr("Clipping")
                    checkable: true
                    checked: comparison.showClipping
                    focusPolicy: Qt.NoFocus
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Clipping warnings: highlights in red, shadows in blue (O)")
                    onClicked: {
                        comparison.showClipping = checked
                        comparison.setOption("clipping", checked)
                    }
                }
                ToolButton {
                    text: qsTr("Histogram")
                    checkable: true
                    checked: comparison.showHistogram
                    focusPolicy: Qt.NoFocus
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Histogram (H)")
                    onClicked: {
                        comparison.showHistogram = checked
                        comparison.setOption("histogram", checked)
                    }
                }
                ToolButton {
                    text: comparison.fit ? qsTr("100 %") : qsTr("Fit")
                    focusPolicy: Qt.NoFocus
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Fit or 100 % in every frame (Z)")
                    onClicked: comparison.toggleActual()
                }
                ToolButton {
                    id: resolveButton
                    text: qsTr("Resolve")
                    focusPolicy: Qt.NoFocus
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Resolve the series: keep the marked frames, reject the others (R)")
                    onClicked: comparison.resolve()
                }
                ToolButton {
                    text: qsTr("Full screen")
                    focusPolicy: Qt.NoFocus
                    onClicked: comparison.library.fullScreenToggled()
                }
                ToolButton {
                    text: "✕"
                    focusPolicy: Qt.NoFocus
                    Accessible.name: qsTr("Back to the grid")
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Back to the grid (Esc)")
                    onClicked: comparison.library.closeCompare()
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 2
            Repeater {
                id: panes
                model: comparison.pageIds
                ComparePane {
                    required property int index
                    required property string modelData
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    compare: comparison
                    photoId: modelData
                    slot: index
                    focused: index === comparison.focusSlotIndex
                }
            }
        }
    }
}
