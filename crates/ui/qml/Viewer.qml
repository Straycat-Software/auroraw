// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// The image view and cull mode (spec §5.3, D-100): one photo at a time over the grid, from the photo service's
// picture (`image://preview/<id>`, the grid's thumbnail scaled up until it arrives), fit to the window or at
// 100 %, with a filmstrip, a line about the photo and, at the top right, its state (stars, flag, colour) that a
// click goes through. Everything is a key, all of them the grid's (a rating,
// a flag or a colour label is the library's own action, one step of the history, undone with Ctrl+Z):
// Left and Right (Backspace and Space) walk, 0 to 5 rate, P X U flag, 6 to 9 label, Z fits or shows 100 %,
// I, T and A show the information and the filmstrip and turn auto-advance on, F is full screen, Escape or Enter
// goes back to the grid. The photo shown is the grid's cursor, so the grid is where the person left it.
FocusScope {
    id: view
    required property var library
    required property var launcher

    readonly property var photos: library.photoGrid
    readonly property int row: library.grid.currentIndex
    readonly property string photoId: row >= 0 ? photos.idAt(row) : ""

    // Fit to the window, or the picture's own pixels (`zoom` is then its scale: 1 is 100 %).
    property bool fit: true
    property real zoom: 1
    // Options the person's settings remember.
    property bool autoAdvance: false
    property bool showFilmstrip: true
    property bool showInfo: true
    // The quality aids (D-103): overlays and histogram, remembered.
    property bool showPeaking: false
    property bool showClipping: false
    property bool showHistogram: false
    // What was measured on the picture shown (null until it arrives), and its sharpness among the frames of its series.
    property var aids: null
    property string sharpness: ""
    property bool measuring: false
    // What is shown of the photo.
    property int rating: 0
    property int flag: 0
    property string colour: ""
    property string summary: ""

    property alias flick: flick
    property alias picture: big
    property alias strip: strip
    property alias infoBar: infoBar
    property alias histogram: histogram
    property alias ratingButton: ratingButton
    property alias flagButton: flagButton
    property alias colourButton: colourButton
    property alias keepButton: keepButton
    property alias compareButton: compareButton

    readonly property real fitScale: big.implicitWidth > 0 && big.implicitHeight > 0
                                     ? Math.min(flick.width / big.implicitWidth, flick.height / big.implicitHeight) : 1
    readonly property real shownScale: fit ? fitScale : zoom

    signal opened()

    onOpened: {
        autoAdvance = launcher.viewOption("autoAdvance")
        showFilmstrip = launcher.viewOption("filmstrip")
        showInfo = launcher.viewOption("info")
        showPeaking = launcher.viewOption("peaking")
        showClipping = launcher.viewOption("clipping")
        showHistogram = launcher.viewOption("histogram")
        refresh()
        Qt.callLater(strip.centre)
        forceActiveFocus()
    }

    // What the strip says, and the photos to make ahead.
    function refresh() {
        refreshInfo()
        photos.prefetchAround(row)
    }

    // What was measured on the picture, once it is there; the sharpness among the frames of its series, as they are measured.
    function loadAids() {
        const text = photos.aidsOf(photoId)
        aids = text === "" ? null : JSON.parse(text)
        refreshSharpness()
    }

    function refreshSharpness() {
        const members = photoId === "" ? "" : photos.seriesMembersOf(photoId)
        if (members === "") {
            sharpness = ""
            measuring = false
            return
        }
        photos.analyse(members)
        const ids = members.split(",")
        const ranks = JSON.parse(photos.sharpnessRanks(members))
        const rank = ranks[ids.indexOf(photoId)]
        measuring = rank === undefined || rank < 0
        sharpness = measuring ? qsTr("Sharpness: measuring…")
                    : rank >= 100 ? qsTr("Sharpest of the series") : qsTr("Sharpness: %1 % of the series' best").arg(rank)
    }

    Timer {
        // A series' frames are measured in the background: the figure is asked for again until it is there.
        interval: 600
        repeat: true
        running: view.visible && view.measuring
        onTriggered: view.refreshSharpness()
    }

    function refreshInfo() {
        rating = photos.ratingAt(row)
        flag = photos.flagAt(row)
        colour = photos.labelAt(row)
        summary = row >= 0 ? photos.infoAt(row) : ""
    }

    onRowChanged: {
        aids = null
        refresh()
        Qt.callLater(centre)
    }
    onVisibleChanged: if (visible) forceActiveFocus()

    Connections {
        target: view.photos
        // A rating, a flag or a label changed (the engine's event, an undo): the line follows.
        function onDataChanged() { view.refreshInfo() }
        function onModelReset() { view.refreshInfo() }
    }

    function centre() {
        flick.contentX = Math.max(0, (flick.contentWidth - flick.width) / 2)
        flick.contentY = Math.max(0, (flick.contentHeight - flick.height) / 2)
    }

    // Fit to the window, or the picture's own pixels.
    function toggleActual() {
        fit = !fit
        zoom = 1
        Qt.callLater(centre)
    }

    // Zooms by `factor` about the point (x, y) of the window's picture area.
    function zoomAt(x, y, factor) {
        const before = shownScale
        const after = Math.max(0.05, Math.min(8, before * factor))
        // The point of the picture under the pointer stays there.
        const px = (flick.contentX + x - (flick.contentWidth - big.width) / 2) / Math.max(before, 0.0001)
        const py = (flick.contentY + y - (flick.contentHeight - big.height) / 2) / Math.max(before, 0.0001)
        zoom = after
        fit = false
        Qt.callLater(() => {
            flick.contentX = Math.max(0, Math.min(flick.contentWidth - flick.width,
                (flick.contentWidth - big.width) / 2 + px * after - x))
            flick.contentY = Math.max(0, Math.min(flick.contentHeight - flick.height,
                (flick.contentHeight - big.height) / 2 + py * after - y))
        })
    }

    function setOption(name, on) {
        launcher.setViewOption(name, on)
    }

    function step(delta) {
        const target = row + delta
        if (target >= 0 && target < photos.count)
            library.goTo(target, 0)
    }

    // After a key that rated, flagged or labelled: on to the next photo when auto-advance is on.
    function acted() {
        if (autoAdvance)
            step(1)
    }

    Keys.onPressed: event => {
        // Ctrl and Alt are the window's (Ctrl+Z, Ctrl+K...): the keys here are plain.
        if (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))
            return
        const key = event.key
        if (key >= Qt.Key_0 && key <= Qt.Key_5) {
            library.rate(key - Qt.Key_0)
            acted()
        } else if (key >= Qt.Key_6 && key <= Qt.Key_9) {
            library.label(["red", "yellow", "green", "blue"][key - Qt.Key_6])
            acted()
        } else if (key === Qt.Key_P || key === Qt.Key_X || key === Qt.Key_U) {
            library.flag(key === Qt.Key_P ? "pick" : key === Qt.Key_X ? "reject" : "clear")
            acted()
        } else if (key === Qt.Key_Left || key === Qt.Key_Backspace) {
            step(-1)
        } else if (key === Qt.Key_Right || key === Qt.Key_Space) {
            step(1)
        } else if (key === Qt.Key_Home) {
            step(-row)
        } else if (key === Qt.Key_End) {
            step(photos.count - 1 - row)
        } else if (key === Qt.Key_Z) {
            toggleActual()
        } else if (key === Qt.Key_Plus || key === Qt.Key_Equal) {
            zoomAt(flick.width / 2, flick.height / 2, 1.25)
        } else if (key === Qt.Key_Minus) {
            zoomAt(flick.width / 2, flick.height / 2, 0.8)
        } else if (key === Qt.Key_F || key === Qt.Key_F11) {
            library.fullScreenToggled()
        } else if (key === Qt.Key_I) {
            showInfo = !showInfo
            setOption("info", showInfo)
        } else if (key === Qt.Key_T) {
            showFilmstrip = !showFilmstrip
            setOption("filmstrip", showFilmstrip)
        } else if (key === Qt.Key_S) {
            showPeaking = !showPeaking
            setOption("peaking", showPeaking)
        } else if (key === Qt.Key_O) {
            showClipping = !showClipping
            setOption("clipping", showClipping)
        } else if (key === Qt.Key_H) {
            showHistogram = !showHistogram
            setOption("histogram", showHistogram)
        } else if (key === Qt.Key_K) {
            library.toggleMark(photoId)
        } else if (key === Qt.Key_C) {
            library.openCompare()
        } else if (key === Qt.Key_A) {
            autoAdvance = !autoAdvance
            setOption("autoAdvance", autoAdvance)
        } else if (key === Qt.Key_Escape || key === Qt.Key_Return || key === Qt.Key_Enter) {
            library.closeView()
        } else {
            return
        }
        event.accepted = true
    }

    Rectangle {
        anchors.fill: parent
        color: Theme.viewerGround
    }

    // The view is over the grid: what it does not take itself (a click on the dark around the picture, a right click
    // anywhere) must not reach the grid under it. A right click opens the photo's menu (the grid's, without the row that opens the view).
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onPressed: mouse => {
            view.forceActiveFocus()
            if (mouse.button === Qt.RightButton)
                view.library.viewMenu.popup(parent, mouse.x, mouse.y)
        }
    }

    // The picture. The grid's thumbnail stands in until the big one arrives.
    Image {
        id: thumb
        anchors.fill: flick
        source: view.photoId !== "" ? "image://thumbs/" + view.photoId : ""
        fillMode: Image.PreserveAspectFit
        visible: big.status !== Image.Ready
        asynchronous: true
    }

    Flickable {
        id: flick
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: infoBar.visible ? infoBar.top : (strip.visible ? strip.top : parent.bottom)
        clip: true
        contentWidth: Math.max(width, big.width)
        contentHeight: Math.max(height, big.height)
        interactive: contentWidth > width || contentHeight > height
        boundsBehavior: Flickable.StopAtBounds

        Image {
            id: big
            x: (flick.contentWidth - width) / 2
            y: (flick.contentHeight - height) / 2
            source: view.photoId !== "" ? "image://preview/" + view.photoId : ""
            asynchronous: true
            // The service keeps its own few pictures; Qt keeping decoded ones as well would hold tens of megabytes each.
            cache: false
            mipmap: true
            width: implicitWidth * view.shownScale
            height: implicitHeight * view.shownScale
            visible: status === Image.Ready
        }

        // The quality aids lie exactly over the picture: made only once asked for.
        Image {
            x: big.x
            y: big.y
            width: big.width
            height: big.height
            visible: view.showPeaking && big.status === Image.Ready
            source: view.showPeaking && view.photoId !== "" ? "image://peaking/" + view.photoId : ""
            asynchronous: true
            cache: false
        }
        Image {
            x: big.x
            y: big.y
            width: big.width
            height: big.height
            visible: view.showClipping && big.status === Image.Ready
            source: view.showClipping && view.photoId !== "" ? "image://clipping/" + view.photoId : ""
            asynchronous: true
            cache: false
        }

        WheelHandler {
            acceptedModifiers: Qt.NoModifier
            onWheel: event => view.zoomAt(point.position.x, point.position.y, Math.pow(1.15, event.angleDelta.y / 120))
        }
        TapHandler {
            onDoubleTapped: view.toggleActual()
        }
    }

    Connections {
        target: big
        function onStatusChanged() {
            if (big.status === Image.Ready)
                view.loadAids()
        }
    }
    Histogram {
        id: histogram
        anchors.left: parent.left
        anchors.bottom: flick.bottom
        anchors.margins: 10
        visible: view.showHistogram && view.aids !== null
        counts: view.aids ? view.aids.histogram : null
        high: view.aids ? view.aids.high : 0
        low: view.aids ? view.aids.low : 0
    }

    // The photo whose original is not there (a source that is offline, a file that went): its thumbnail stays.
    Label {
        anchors.horizontalCenter: flick.horizontalCenter
        anchors.bottom: flick.bottom
        anchors.bottomMargin: 12
        visible: big.status === Image.Error
        text: qsTr("The original is not available")
        color: Theme.warning
        padding: 6
        background: Rectangle { color: "#b0000000"; radius: Theme.radiusControl }
    }

    // What the person can do with the mouse: the keys are the way, these are for finding them.
    Rectangle {
        anchors.top: parent.top
        anchors.right: parent.right
        anchors.margins: 8
        width: tools.implicitWidth + 12
        height: tools.implicitHeight + 8
        radius: Theme.radiusContainer
        color: "#a0000000"
        RowLayout {
            id: tools
            anchors.centerIn: parent
            spacing: 2
            // The photo's state, in its colours, each a button that goes round its states: the stars from 0 to
            // 5, the flag (none, picked, rejected) and the colour label (none, then the five colours).
            ToolButton {
                id: ratingButton
                focusPolicy: Qt.NoFocus
                text: "★".repeat(view.rating) + "☆".repeat(5 - view.rating)
                font.pixelSize: 15
                palette.buttonText: view.rating > 0 ? Theme.rating : Theme.quiet
                Accessible.name: qsTr("Rating")
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Rating: click to change it (0 to 5)")
                onClicked: view.library.rate((view.rating + 1) % 6)
            }
            ToolButton {
                id: flagButton
                focusPolicy: Qt.NoFocus
                enabled: view.library.canFlag
                opacity: enabled ? 1 : 0.35
                text: view.flag === 1 ? "✔" : view.flag === 2 ? "✖" : "–"
                font.pixelSize: 15
                palette.buttonText: view.flag === 1 ? Theme.picked : view.flag === 2 ? Theme.danger : Theme.quiet
                Accessible.name: qsTr("Flag")
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Flag: click for picked, rejected, none (P, X, U)")
                onClicked: view.library.flag(view.flag === 0 ? "pick" : view.flag === 1 ? "reject" : "clear")
            }
            ToolButton {
                id: colourButton
                focusPolicy: Qt.NoFocus
                Accessible.name: qsTr("Colour label")
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Colour label: click to go through the colours (6 to 9)")
                contentItem: Rectangle {
                    implicitWidth: 14
                    implicitHeight: 14
                    radius: 7
                    color: view.colour === "" ? "transparent" : Theme.labelColour(view.colour)
                    border.width: 2
                    border.color: view.colour === "" ? Theme.quiet : "white"
                }
                onClicked: {
                    const order = ["", "red", "yellow", "green", "blue", "purple"]
                    const next = order[(order.indexOf(view.colour) + 1) % order.length]
                    view.library.label(next === "" ? "none" : next)
                }
            }
            Rectangle {
                implicitWidth: 1
                implicitHeight: 18
                color: Theme.quiet
                opacity: 0.5
            }
            ToolButton {
                id: keepButton
                text: qsTr("Keep")
                visible: view.library.inSeries
                enabled: view.library.canKeep
                opacity: enabled ? 1 : 0.35
                checkable: true
                checked: (view.photos.markSerial, view.photos.isMarked(view.photoId))
                focusPolicy: Qt.NoFocus
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Mark this photo to keep, for resolving its series (K)")
                onClicked: view.library.toggleMark(view.photoId)
            }
            ToolButton {
                text: qsTr("Peaking")
                checkable: true
                checked: view.showPeaking
                focusPolicy: Qt.NoFocus
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Focus peaking: what is in focus (S)")
                onClicked: {
                    view.showPeaking = checked
                    view.setOption("peaking", checked)
                }
            }
            ToolButton {
                text: qsTr("Clipping")
                checkable: true
                checked: view.showClipping
                focusPolicy: Qt.NoFocus
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Clipping warnings: highlights in red, shadows in blue (O)")
                onClicked: {
                    view.showClipping = checked
                    view.setOption("clipping", checked)
                }
            }
            ToolButton {
                text: qsTr("Histogram")
                checkable: true
                checked: view.showHistogram
                focusPolicy: Qt.NoFocus
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Histogram (H)")
                onClicked: {
                    view.showHistogram = checked
                    view.setOption("histogram", checked)
                }
            }
            ToolButton {
                id: compareButton
                text: qsTr("Compare")
                visible: view.library.inSeries
                enabled: view.library.canCompare
                opacity: enabled ? 1 : 0.35
                focusPolicy: Qt.NoFocus
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Compare with the other frames of the series (C)")
                onClicked: view.library.openCompare()
            }
            ToolButton {
                text: view.fit ? qsTr("100 %") : qsTr("Fit")
                focusPolicy: Qt.NoFocus
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Fit or 100 % (Z)")
                onClicked: view.toggleActual()
            }
            ToolButton {
                text: qsTr("Info")
                checkable: true
                checked: view.showInfo
                focusPolicy: Qt.NoFocus
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Information (I)")
                onClicked: {
                    view.showInfo = checked
                    view.setOption("info", checked)
                }
            }
            ToolButton {
                text: qsTr("Filmstrip")
                checkable: true
                checked: view.showFilmstrip
                focusPolicy: Qt.NoFocus
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Filmstrip (T)")
                onClicked: {
                    view.showFilmstrip = checked
                    view.setOption("filmstrip", checked)
                }
            }
            ToolButton {
                text: qsTr("Auto-advance")
                checkable: true
                checked: view.autoAdvance
                focusPolicy: Qt.NoFocus
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Move on after a rating, flag or label (A)")
                onClicked: {
                    view.autoAdvance = checked
                    view.setOption("autoAdvance", checked)
                }
            }
            ToolButton {
                text: qsTr("Full screen")
                focusPolicy: Qt.NoFocus
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Full screen (F)")
                onClicked: view.library.fullScreenToggled()
            }
            ToolButton {
                text: "✕"
                focusPolicy: Qt.NoFocus
                Accessible.name: qsTr("Back to the grid")
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Back to the grid (Esc)")
                onClicked: view.library.closeView()
            }
        }
    }

    // The line about the photo: where it is in the list, its name, its stars, flag and colour.
    Rectangle {
        id: infoBar
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: strip.visible ? strip.top : parent.bottom
        height: visible ? 30 : 0
        visible: view.showInfo
        color: "#c0000000"
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 10
            anchors.rightMargin: 10
            spacing: 12
            Label {
                text: qsTr("%1 / %2").arg(view.row + 1).arg(view.photos.count)
                color: Theme.quiet
            }
            Label {
                Layout.fillWidth: true
                text: view.summary
                color: "#eaeaeb"
                elide: Text.ElideRight
            }
            Label {
                visible: view.sharpness !== ""
                text: view.sharpness
                color: Theme.quiet
            }
        }
    }

    // A discreet filmstrip: the neighbours, the current one framed; a click goes there.
    Rectangle {
        anchors.fill: strip
        visible: strip.visible
        color: "#c0000000"
    }
    ListView {
        id: strip
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: visible ? 76 : 0
        visible: view.showFilmstrip
        orientation: ListView.Horizontal
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        spacing: 4
        model: view.photos
        currentIndex: view.row
        highlightMoveDuration: 0
        onCurrentIndexChanged: centre()
        // The list is rebuilt (a series opened for the view) and laid out after the index is set: centre once it is.
        onCountChanged: Qt.callLater(centre)
        onWidthChanged: Qt.callLater(centre)
        onVisibleChanged: if (visible) Qt.callLater(centre)
        function centre() {
            if (currentIndex >= 0)
                positionViewAtIndex(currentIndex, ListView.Center)
        }

        delegate: Item {
            id: frame
            required property int index
            required property string photoId
            required property int rating
            required property int flag
            required property string colourLabel
            required property bool marked
            width: 96
            height: strip.height
            Rectangle {
                anchors.fill: parent
                anchors.margins: 4
                color: "#1c1d21"
                border.width: frame.index === view.row ? 2 : 0
                border.color: Theme.accent
                Image {
                    anchors.fill: parent
                    anchors.margins: 2
                    source: "image://thumbs/" + frame.photoId
                    fillMode: Image.PreserveAspectFit
                    asynchronous: true
                    opacity: frame.flag === 2 ? 0.35 : 1
                }
                Rectangle {
                    anchors.fill: parent
                    visible: frame.marked
                    color: "transparent"
                    border.width: 3
                    border.color: Theme.picked
                }
                Rectangle {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    height: 4
                    visible: frame.colourLabel !== ""
                    color: Theme.labelColour(frame.colourLabel)
                }
                Text {
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.margins: 3
                    visible: frame.rating > 0
                    text: frame.rating + view.library.star
                    font.pixelSize: 10
                    color: Theme.rating
                }
                Text {
                    anchors.right: parent.right
                    anchors.top: parent.top
                    anchors.margins: 3
                    visible: frame.flag !== 0
                    text: frame.flag === 1 ? "✔" : "✖"
                    font.pixelSize: 10
                    color: frame.flag === 1 ? Theme.picked : Theme.danger
                }
                MouseArea {
                    anchors.fill: parent
                    onClicked: view.library.goTo(frame.index, 0)
                }
            }
        }
    }
}
