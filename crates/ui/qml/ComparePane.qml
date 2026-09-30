// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// One frame of the comparison (D-103): the picture (its overlays and histogram when asked for) with its name, stars,
// flag, colour, its sharpness among the frames compared and the mark that says to keep it. The zoom and the
// place shown are the comparison's, shared by every pane: dragging or zooming in one moves them all.
Item {
    id: pane
    required property var compare
    required property string photoId
    required property int slot
    property bool focused: false

    readonly property var photos: compare.photos
    // What is shown of the photo (asked for again when the model says something changed).
    property int rating: 0
    property int flag: 0
    property string colour: ""
    property string name: ""
    property var aids: null
    property bool applying: false

    property alias flick: flick
    property alias picture: big
    // The first frame's shape says how four are laid out.
    Binding {
        target: pane.compare
        property: "frameRatio"
        value: big.implicitHeight > 0 ? big.implicitWidth / big.implicitHeight : 0
        when: pane.slot === 0
    }
    property alias keepButton: keepButton
    property alias peakingOverlay: peakingImage
    property alias clippingOverlay: clippingImage
    property alias histogram: histogramView

    readonly property real fitScale: big.implicitWidth > 0 && big.implicitHeight > 0
                                     ? Math.min(flick.width / big.implicitWidth, flick.height / big.implicitHeight) : 1
    readonly property real shownScale: compare.fit ? fitScale : compare.zoom
    readonly property int rank: compare.rankOf(photoId, compare.ranksSerial)
    readonly property bool marked: (photos.markSerial, photos.isMarked(photoId))

    function refreshInfo() {
        const row = photos.rowOf(photoId)
        rating = photos.ratingAt(row)
        flag = photos.flagAt(row)
        colour = photos.labelAt(row)
        name = row >= 0 ? photos.infoAt(row) : ""
    }
    Component.onCompleted: refreshInfo()
    onPhotoIdChanged: {
        aids = null
        refreshInfo()
    }
    Connections {
        target: pane.photos
        function onDataChanged() { pane.refreshInfo() }
        function onModelReset() { pane.refreshInfo() }
    }

    // Puts the flickable where the shared view says (the centre, as a share of the picture).
    function applyView() {
        if (compare.viewSource === pane || big.width <= 0)
            return
        applying = true
        flick.contentX = Math.max(0, Math.min(flick.contentWidth - flick.width,
                                              compare.centreX * big.width - flick.width / 2))
        flick.contentY = Math.max(0, Math.min(flick.contentHeight - flick.height,
                                              compare.centreY * big.height - flick.height / 2))
        if (compare.fit) {
            flick.contentX = Math.max(0, (flick.contentWidth - flick.width) / 2)
            flick.contentY = Math.max(0, (flick.contentHeight - flick.height) / 2)
        }
        applying = false
    }
    Connections {
        target: pane.compare
        function onViewSerialChanged() { Qt.callLater(pane.applyView) }
    }

    // The point of the picture (as shares of it) under a point of the viewport.
    function shareAt(px, py) {
        const offsetX = (flick.contentWidth - big.width) / 2
        const offsetY = (flick.contentHeight - big.height) / 2
        return Qt.point((flick.contentX + px - offsetX) / Math.max(big.width, 1),
                        (flick.contentY + py - offsetY) / Math.max(big.height, 1))
    }

    Rectangle {
        anchors.fill: parent
        color: Theme.viewerGround
        border.width: pane.focused ? 2 : 0
        border.color: Theme.accent
    }

    Flickable {
        id: flick
        anchors.fill: parent
        anchors.topMargin: 24
        anchors.bottomMargin: 30
        clip: true
        contentWidth: Math.max(width, big.width)
        contentHeight: Math.max(height, big.height)
        interactive: contentWidth > width || contentHeight > height
        boundsBehavior: Flickable.StopAtBounds

        onContentXChanged: pane.dragged()
        onContentYChanged: pane.dragged()

        Image {
            id: thumb
            x: (flick.contentWidth - width) / 2
            y: (flick.contentHeight - height) / 2
            width: flick.width
            height: flick.height
            source: pane.photoId !== "" ? "image://thumbs/" + pane.photoId : ""
            fillMode: Image.PreserveAspectFit
            visible: big.status !== Image.Ready
            asynchronous: true
        }
        Image {
            id: big
            x: (flick.contentWidth - width) / 2
            y: (flick.contentHeight - height) / 2
            source: pane.photoId !== "" ? "image://preview/" + pane.photoId : ""
            asynchronous: true
            cache: false
            mipmap: true
            width: implicitWidth * pane.shownScale
            height: implicitHeight * pane.shownScale
            visible: status === Image.Ready
            onStatusChanged: if (status === Image.Ready) {
                const text = pane.photos.aidsOf(pane.photoId)
                pane.aids = text === "" ? null : JSON.parse(text)
                Qt.callLater(pane.applyView)
            }
        }
        Image {
            id: peakingImage
            x: big.x
            y: big.y
            width: big.width
            height: big.height
            visible: pane.compare.showPeaking && big.status === Image.Ready
            source: pane.compare.showPeaking && pane.photoId !== "" ? "image://peaking/" + pane.photoId : ""
            asynchronous: true
            cache: false
        }
        Image {
            id: clippingImage
            x: big.x
            y: big.y
            width: big.width
            height: big.height
            visible: pane.compare.showClipping && big.status === Image.Ready
            source: pane.compare.showClipping && pane.photoId !== "" ? "image://clipping/" + pane.photoId : ""
            asynchronous: true
            cache: false
        }
        WheelHandler {
            acceptedModifiers: Qt.NoModifier
            onWheel: event => pane.compare.zoomAt(pane, point.position.x, point.position.y,
                                                  Math.pow(1.15, event.angleDelta.y / 120))
        }
    }

    // The user moved the picture in this pane: the others follow.
    function dragged() {
        if (applying || !(flick.dragging || flick.flicking) || compare.fit)
            return
        compare.panned(pane, (flick.contentX + flick.width / 2) / Math.max(big.width, 1),
                       (flick.contentY + flick.height / 2) / Math.max(big.height, 1))
    }

    // A tap anywhere focuses the pane.
    TapHandler {
        onTapped: pane.compare.focusSlot(pane.slot)
    }

    // The header and the footer sit on a scrim, like every other overlay on the viewer's 50% grey (D-129): the
    // text and marks were tuned for a near-black ground, and bare on the grey the header is 3.7:1, `quiet` 1.7:1,
    // the stars 2.2:1 and the flags 1.8:1 and 1.4:1; on a strong scrim they are 5:1 and up.
    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: header.implicitHeight + 8
        color: "#c0000000"
    }
    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: 34
        color: "#c0000000"
    }

    // The header: which photo, and where it comes in the frames compared.
    Label {
        id: header
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: 4
        text: pane.name
        elide: Text.ElideRight
        color: Theme.surface.text
        font.pixelSize: 12
    }

    Histogram {
        id: histogramView
        anchors.left: parent.left
        anchors.bottom: parent.bottom
        anchors.margins: 36
        width: Math.min(220, parent.width - 72)
        height: 90
        visible: pane.compare.showHistogram && pane.aids !== null
        counts: pane.aids ? pane.aids.histogram : null
        high: pane.aids ? pane.aids.high : 0
        low: pane.aids ? pane.aids.low : 0
    }

    // The footer: stars, flag, colour, the sharpness among the frames compared, and the keep mark.
    RowLayout {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 4
        height: 26
        spacing: 8
        Label {
            visible: pane.rating > 0
            text: pane.rating + "★"
            color: Theme.rating
        }
        Label {
            visible: pane.flag !== 0
            text: pane.flag === 1 ? "✔" : "✖"
            color: pane.flag === 1 ? Theme.picked : Theme.danger
        }
        Rectangle {
            visible: pane.colour !== ""
            width: 12
            height: 12
            radius: 6
            color: Theme.labelColour(pane.colour)
        }
        Label {
            visible: pane.rank >= 0
            text: pane.rank >= 100 ? qsTr("★ Sharpest") : qsTr("Sharpness %1 %").arg(pane.rank)
            color: pane.rank >= 100 ? Theme.rating : Theme.quiet
        }
        Item { Layout.fillWidth: true }
        ToolButton {
            id: keepButton
            text: qsTr("Keep")
            // Only a frame of an unresolved series is marked to keep (its series' state is the selection's, the focus).
            visible: ((pane.compare.library.photoGrid.selectionSeries, pane.photos.seriesStateOf(pane.photoId)) & 6) !== 0
            enabled: !pane.compare.resolved
            opacity: enabled ? 1 : 0.35
            checkable: true
            checked: pane.marked
            focusPolicy: Qt.NoFocus
            padding: 2
            Accessible.name: qsTr("Keep this photo")
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Mark this photo to keep (K)")
            onClicked: pane.compare.library.toggleMark(pane.photoId)
        }
    }

    // A frame marked to keep: a green ring around the pane.
    Rectangle {
        anchors.fill: parent
        visible: pane.marked
        color: "transparent"
        border.width: 3
        border.color: Theme.picked
    }
}
