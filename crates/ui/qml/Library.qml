// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// The library grid (the cull task): a filter bar with how many photos it lists, the photos as
// thumbnails from `image://thumbs`, and a strip that describes what is selected. Several photos can be
// selected (D-097): the selection is a set the model keeps by photo (`PhotoGrid`), the cursor (where the
// keyboard is) is the `GridView`'s current index, and ranges start from the model's anchor. Click, Shift and
// Ctrl with the mouse and the keys, Space, Escape, and a rubber band, as in a file manager.
FocusScope {
    id: root
    required property var photoGrid
    required property var launcher
    property var hostWindow: null

    property alias grid: grid
    property alias filterBar: filterBar
    property alias filterButtons: filterButtons
    property alias refreshButton: refreshButton
    // What the strip under the grid says: the selected photo, or how many are selected.
    // (For several photos the text is a binding, so that a change of language reaches it.)
    property string single: ""
    readonly property string summary: photoGrid.selectedCount > 1
        ? qsTr("%n photo(s) selected", "", photoGrid.selectedCount) : single
    readonly property string status: qsTr("%n photo(s)", "", photoGrid.total)
    readonly property int selectedCount: photoGrid.selectedCount
    property alias keywords: keywordList
    property alias collections: collectionList
    property alias viewer: viewer
    property alias compareView: compareView
    property alias similarPanel: similarPanel
    property alias sizeSlider: sizeSlider
    property alias cellMenu: cellMenu
    property alias viewMenu: viewMenu
    property alias labelFilterButtons: labelFilterButtons
    property alias placeMenu: placeMenu
    // The image view (one photo at a time) is open over the grid.
    property bool viewing: false
    // The comparison of frames is open over the grid.
    property bool comparing: false
    // The grid's thumbnails: their width in pixels (96 to 256, remembered), and the height that goes with it.
    property int thumbW: 160
    readonly property int thumbH: Math.round(thumbW * 3 / 4)
    onVisibleChanged: if (visible) {
        const remembered = launcher.intOption("thumbSize")
        if (remembered >= 96)
            thumbW = remembered
    }
    // The similar photos panel (D-105) is shown.
    property bool similarShown: false
    function showSimilar(shown) {
        similarShown = shown
        if (!shown)
            grid.forceActiveFocus()
    }
    // Every series is shown open (the button of the filter bar says which way it goes next).
    property bool allOpen: false
    // Asked to make the window full screen or back (the window's business).
    signal fullScreenToggled()
    property alias keywordPanel: keywordPanel
    // A dialog of the panel is open (the window's commands wait, as for every dialog).
    readonly property bool dialogOpen: keywordPanel.renameDialog.visible || keywordPanel.moveDialog.visible
                                       || keywordPanel.deleteDialog.visible || keywordPanel.propertiesDialog.visible
                                       || keywordPanel.collectionPanel.dialogOpen
    // The name of the keyword the list is filtered by, for its chip.
    property string keywordFilterName: ""
    // The same for the collection the list is filtered by.
    property string collectionFilterName: ""

    KeywordList { id: keywordList }
    CollectionList { id: collectionList }

    // The keyword panel shows which keywords the selection carries, and the metadata panel its fields'
    // current values: both asked once a selection has settled.
    Timer {
        id: usageTimer
        interval: 60
        onTriggered: {
            keywordList.applyUsage(photoGrid.keywordUsage(), photoGrid.selectedCount)
            collectionList.applyUsage(photoGrid.collectionUsage(), photoGrid.selectedCount)
            keywordPanel.metadataPanel.refresh()
        }
    }
    Connections {
        target: Bus
        // Every keyword added or removed, undone or redone, changes what the selection carries.
        function onHistoryChanged() {
            keywordList.refresh()
            collectionList.refresh()
            usageTimer.restart()
            // The keyword the list is filtered by was deleted (or its creation undone): back to the whole list.
            if (photoGrid.keywordFilter !== "") {
                const name = keywordList.nameOf(photoGrid.keywordFilter)
                if (name === "")
                    root.filterKeyword("", "")
                else
                    keywordFilterName = name // it may have been renamed
            }
            // The same for the collection: deleted (or its creation undone), or renamed.
            if (photoGrid.collectionFilter !== "") {
                const name = collectionList.nameOf(photoGrid.collectionFilter)
                if (name === "")
                    root.filterCollection("", "")
                else
                    collectionFilterName = name
            }
        }
    }

    // A colour label's name, in the person's language.
    function colourTitle(name) {
        switch (name) {
        case "red": return qsTr("Red")
        case "yellow": return qsTr("Yellow")
        case "green": return qsTr("Green")
        case "blue": return qsTr("Blue")
        case "purple": return qsTr("Purple")
        }
        return qsTr("No colour")
    }

    // Lists only the photos with this colour label; the same colour again lists them all.
    function filterLabel(name) {
        photoGrid.filterLabel(photoGrid.labelFilter === name ? "" : name)
        grid.currentIndex = -1
        grid.positionViewAtBeginning()
        updateSummary()
    }

    function seriesName(index) {
        return index === 0 ? qsTr("Series") : index === 1 ? qsTr("In a series")
               : index === 2 ? qsTr("Unresolved series") : qsTr("Resolved series")
    }

    function filterSeries(kind) {
        photoGrid.filterSeries(kind)
        grid.currentIndex = -1
        grid.positionViewAtBeginning()
        updateSummary()
    }

    function flagName(index) {
        return index === 0 ? qsTr("Not rejected") : index === 1 ? qsTr("All photos")
               : index === 2 ? qsTr("Picked") : qsTr("Rejected")
    }

    function updateSummary() {
        usageTimer.restart()
        single = photoGrid.selectedCount === 1 ? photoGrid.summaryAt(photoGrid.firstSelectedRow()) : ""
    }

    // Puts the cursor on `index` and brings it into view.
    function showCursor(index) {
        grid.currentIndex = index
        if (index >= 0)
            grid.positionViewAtIndex(index, GridView.Contain)
    }

    // The cursor goes to `index`, and the gesture's modifiers say what happens to the selection: nothing
    // for Ctrl alone, the range from the anchor for Shift (added to the selection with Ctrl too), else
    // only that photo.
    function goTo(index, modifiers) {
        const shift = (modifiers & Qt.ShiftModifier) !== 0
        const ctrl = (modifiers & Qt.ControlModifier) !== 0
        if (shift) {
            // A range needs somewhere to start from: the cursor, when nothing anchors it.
            if (photoGrid.anchorRow() < 0 && grid.currentIndex >= 0)
                photoGrid.selectOnly(grid.currentIndex)
            photoGrid.extendTo(index, ctrl)
        } else if (!ctrl) {
            photoGrid.selectOnly(index)
        }
        showCursor(index)
        updateSummary()
    }

    // Selects only this photo (or nothing, for -1) and puts the cursor there.
    function select(index) {
        if (index < 0) {
            photoGrid.selectNone()
            grid.currentIndex = -1
        } else {
            goTo(index, 0)
        }
        updateSummary()
    }

    // Ctrl+click or Space: this photo joins the selection or leaves it.
    function toggle(index) {
        photoGrid.toggle(index)
        showCursor(index)
        updateSummary()
    }

    function selectAll() { photoGrid.selectAll(); updateSummary() }
    function selectNone() { photoGrid.selectNone(); updateSummary() }
    function invertSelection() { photoGrid.invert(); updateSummary() }

    // What the selection is as to series (`PhotoGrid.selectionSeries`): what a resolved series does not take is off for
    // it (its flags are what its resolution made: reopen it first), and what only a series has is off for a photo in
    // none. The commands, their keys, menus and buttons all ask these.
    readonly property bool inSeries: (photoGrid.selectionSeries & 6) !== 0
    readonly property bool inResolvedSeries: (photoGrid.selectionSeries & 4) !== 0
    readonly property bool inOpenSeries: (photoGrid.selectionSeries & 2) !== 0 && !inResolvedSeries
    readonly property bool canFlag: !inResolvedSeries
    readonly property bool canKeep: inOpenSeries
    readonly property bool canResolve: inOpenSeries
    readonly property bool canGroup: !inResolvedSeries
    readonly property bool canUngroup: inOpenSeries
    readonly property bool canReopen: inResolvedSeries

    // The cursor's photo is what the commands act on when nothing is selected.
    function selectCursorIfNone() {
        if (photoGrid.selectedCount === 0 && grid.currentIndex >= 0)
            photoGrid.selectOnly(grid.currentIndex)
    }

    // Marks a photo to keep, or takes the mark off, unless its series is resolved or it is in none.
    function toggleMark(id) {
        if ((photoGrid.seriesStateOf(id) & 2) !== 0)
            photoGrid.toggleMark(id)
    }

    // Flags what is selected (or the cursor's photo when nothing is): `pick`, `reject` or `clear`.
    function flag(kind) {
        selectCursorIfNone()
        if (!canFlag)
            return
        photoGrid.flagSelection(kind)
        updateSummary()
    }

    // Gives a colour label to what is selected (or the cursor's photo when nothing is): `red`, `yellow`, `green`,
    // `blue`, `purple`, or `none`; the same colour again takes it off.
    function label(name) {
        if (photoGrid.selectedCount === 0 && grid.currentIndex >= 0)
            photoGrid.selectOnly(grid.currentIndex)
        photoGrid.labelSelection(name)
        updateSummary()
    }

    // Runs `change`, which rebuilds the list of rows (a series opens or closes), and leaves the view where it was with
    // the cursor on the same photo. Left alone, the view scrolls back to the cursor's photo, or to the top.
    function keepingTheView(change) {
        const cursor = grid.currentIndex >= 0 ? photoGrid.idAt(grid.currentIndex) : ""
        const scrolled = grid.contentY
        grid.currentIndex = -1
        const result = change()
        grid.contentY = scrolled
        grid.currentIndex = cursor !== "" ? photoGrid.rowOf(cursor) : -1
        grid.contentY = scrolled
        return result
    }

    // Opens or closes the series of the photo in `index` (the cursor's when it is -1); the cursor stays on its photo.
    function toggleSeries(index) {
        const row = index >= 0 ? index : grid.currentIndex
        if (keepingTheView(() => photoGrid.toggleSeries(row)))
            updateSummary()
    }

    // Opens or closes every series.
    function expandAll(open) {
        keepingTheView(() => photoGrid.expandAll(open))
        allOpen = open
        updateSummary()
    }

    // Shows the photo's original in the platform's file manager (the cursor's when `index` is -1).
    function revealCell(index) {
        const row = index >= 0 ? index : grid.currentIndex
        if (row >= 0)
            photoGrid.showInFileManager(photoGrid.idAt(row))
    }

    // Groups what is selected into a series (one step), or takes it out of its series.
    function group() {
        selectCursorIfNone()
        if (canGroup)
            photoGrid.groupSelection()
    }
    function ungroup() {
        selectCursorIfNone()
        if (canUngroup)
            photoGrid.ungroupSelection()
    }
    function reopen() {
        selectCursorIfNone()
        if (canReopen)
            photoGrid.reopenSeries()
    }

    // Resolves the series of the selection, keeping the selected photos: the others are rejected, the kept picked. A
    // collapsed series has nothing to choose from: it is opened for the person to select what to keep.
    function resolve() {
        selectCursorIfNone()
        if (!canResolve)
            return
        if (photoGrid.resolveSeries() === -1)
            toggleSeries(-1)
        updateSummary()
    }

    // The series that the image view or the comparison opened to walk their frames (the covers' photos), and where the
    // grid was scrolled: back in the grid they are closed again and the grid is where it was.
    property var openedForView: []
    property real scrollBeforeView: 0

    function openSeriesForView(row) {
        if (openedForView.length === 0)
            scrollBeforeView = grid.contentY
        const id = photoGrid.idAt(row)
        keepingTheView(() => photoGrid.toggleSeries(row))
        openedForView = openedForView.concat([id])
    }

    function closeSeriesOpenedForView() {
        if (openedForView.length === 0)
            return
        const covers = openedForView
        openedForView = []
        keepingTheView(() => {
            for (const id of covers) {
                const row = photoGrid.rowOf(id)
                if (row >= 0 && !photoGrid.isCollapsed(row))
                    photoGrid.toggleSeries(row)
            }
        })
        grid.contentY = scrollBeforeView
        updateSummary()
    }

    // Opens the image view on `index` (the cursor's photo when it is -1). A collapsed series is opened first: the
    // view walks its members, from its cover.
    function openView(index) {
        let row = index >= 0 ? index : Math.max(grid.currentIndex, 0)
        if (photoGrid.count === 0 || row >= photoGrid.count)
            return
        if (photoGrid.isCollapsed(row)) {
            const id = photoGrid.idAt(row)
            openSeriesForView(row)
            row = photoGrid.rowOf(id)
        }
        goTo(row, 0)
        viewing = true
        viewer.opened()
    }

    // Opens the comparison: on the 2 to 4 photos selected, else on the frames of the series the cursor's photo is in.
    // A closed series is opened first (its frames are rows of the grid).
    // The photos the comparison would show now (identifiers joined by commas), "" when there are fewer than two: the
    // frames of a series the filters leave one photo of are not enough.
    function compareIds() {
        if (photoGrid.count === 0)
            return ""
        const cursor = grid.currentIndex >= 0 ? photoGrid.idAt(grid.currentIndex) : ""
        const chosen = photoGrid.selectedCount
        const ids = chosen >= 2 && chosen <= 4 ? photoGrid.selectedIds() : (cursor !== "" ? photoGrid.seriesMembersOf(cursor) : "")
        return ids.split(",").length >= 2 ? ids : ""
    }

    // Whether there is something to compare (for what shows the command: it depends on the selection, the list and
    // the filters, which the first three terms stand for).
    readonly property bool canCompare: (photoGrid.selectionSeries, photoGrid.count, photoGrid.selectedCount, compareIds() !== "")

    function openCompare() {
        const ids = compareIds()
        if (ids === "")
            return
        for (const id of ids.split(",")) {
            const row = photoGrid.rowOf(id)
            if (row >= 0 && photoGrid.isCollapsed(row))
                openSeriesForView(row)
        }
        viewing = false
        comparing = true
        compareView.openOn(ids)
    }

    // Opens the comparison on exactly these photos (identifiers joined by commas): unlike `openCompare`, the ids are
    // not re-derived from the selection, and a collapsed series among them is compared as the one photo it is (its
    // cover), not opened and expanded to its members. The similar-photos panel uses this: its reference photo is one
    // specific photo, even when the series it happens to be the cover of is still closed.
    function compareOn(idsCsv) {
        if (idsCsv === "" || idsCsv.split(",").length < 2)
            return
        viewing = false
        comparing = true
        compareView.openOn(idsCsv)
    }

    // Back to the grid, with the cursor on the frame that had the focus.
    function closeCompare() {
        if (!comparing)
            return
        comparing = false
        closeSeriesOpenedForView()
        showCursor(grid.currentIndex)
        grid.forceActiveFocus()
    }

    // Back to the grid, with the cursor on the photo that was shown.
    function closeView() {
        if (!viewing)
            return
        viewing = false
        closeSeriesOpenedForView()
        showCursor(grid.currentIndex)
        grid.forceActiveFocus()
    }

    // Puts the keyboard in the keyword field (Ctrl+K).
    function focusKeywords() {
        keywordPanel.open()
    }

    // Lists only the photos in this collection, or inside it (`""` for all).
    function filterCollection(id, name) {
        photoGrid.filterCollection(id)
        collectionFilterName = name
        grid.currentIndex = -1
        grid.positionViewAtBeginning()
        updateSummary()
    }

    // The collections changed without a step of the history (photos left with their source).
    function collectionsWereChanged() {
        collectionList.refresh()
        usageTimer.restart()
    }

    // Lists only the photos with this keyword, or under it (`""` for all).
    function filterKeyword(id, name) {
        photoGrid.filterKeyword(id)
        keywordFilterName = name
        grid.currentIndex = -1
        grid.positionViewAtBeginning()
        updateSummary()
    }

    function filterFlags(flags) {
        photoGrid.filterFlags(flags)
        grid.currentIndex = -1
        grid.positionViewAtBeginning()
        updateSummary()
    }

    // Rates what is selected (as one action, one step of the history), or the cursor's photo when nothing is.
    function rate(stars) {
        if (photoGrid.selectedCount > 0)
            photoGrid.rateSelection(stars)
        else if (grid.currentIndex >= 0)
            photoGrid.setRating(grid.currentIndex, stars)
        updateSummary()
    }

    // Reads the list again (photos arrived): the selection stays on its photos and the cursor on its own,
    // if they are still listed, and the view where it was.
    function reload() {
        const cursor = grid.currentIndex >= 0 ? photoGrid.idAt(grid.currentIndex) : ""
        const scrolled = grid.contentY
        photoGrid.load()
        keywordList.refresh()
        collectionList.refresh()
        grid.currentIndex = cursor !== "" ? photoGrid.rowOf(cursor) : -1
        grid.contentY = scrolled
        grid.returnToBounds()
        updateSummary()
    }

    // Lists the photos rated `minRating` or more; nothing is selected any more.
    function filterBy(minRating) {
        photoGrid.filterBy(minRating)
        keywordList.refresh()
        grid.currentIndex = -1
        grid.positionViewAtBeginning()
        updateSummary()
    }

    // An action was undone or redone (Edit menu, Ctrl+Z): the photos it touched are shown as they are now,
    // selected, and the first is brought into view, as a person expects to see what was undone.
    function historyApplied(photoIds) {
        // A step about the vocabulary touches no photo the person should be sent to: the selection stays.
        if (photoIds.length === 0) {
            updateSummary()
            return
        }
        for (const id of photoIds)
            photoGrid.syncPhoto(id)
        // A filter may now list a photo it did not, or not list one it did.
        if (photoGrid.minRating > 0)
            reload()
        photoGrid.selectPhotos(photoIds.join(","))
        showCursor(photoGrid.rowOf(photoIds[0]))
        updateSummary()
    }

    // A series changed (a step, an undo, the detection after a scan): the list is read again, once for a run of them.
    function seriesChanged() { seriesTimer.restart() }
    Timer {
        id: seriesTimer
        interval: 80
        onTriggered: root.reload()
    }

    // The engine filled the places of a catalogue made before they were kept, once: no photo "changed", and the places
    // of all of them are new to the place menu.
    function placesFilled() { photoGrid.placesChanged() }

    // The engine says a photo changed: its cell and, when it is what the strip describes, the strip follow.
    function photoChanged(photoId) {
        photoGrid.refreshPhoto(photoId)
        if (photoGrid.selectedCount === 1 && photoGrid.isSelected(photoGrid.rowOf(photoId)))
            updateSummary()
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            Rectangle {
                id: filterBar
                Layout.fillWidth: true
                Layout.preferredHeight: 40
                color: root.palette.window
                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 6
                    anchors.rightMargin: 6
                    spacing: 6
                    Repeater {
                        id: filterButtons
                        // (The model holds no text, so that a change of language does not rebuild the buttons.)
                        model: 6
                        AppButton {
                            required property int index
                            readonly property int minRating: index
                            text: index === 0 ? qsTr("All") : index === 1 ? qsTr("1+") : index === 2 ? qsTr("2+")
                                  : index === 3 ? qsTr("3+") : index === 4 ? qsTr("4+") : qsTr("5")
                            highlighted: root.photoGrid.minRating === minRating
                            focusPolicy: Qt.NoFocus
                            onClicked: {
                                root.filterBy(minRating)
                                grid.forceActiveFocus()
                            }
                        }
                    }
                    // Which flags are shown: rejected photos are hidden unless asked for (spec §5.3).
                    AppComboBox {
                        id: flagBox
                        sizingTexts: [0, 1, 2, 3].map(i => root.flagName(i))
                        Layout.leftMargin: 8
                        model: 4
                        focusPolicy: Qt.NoFocus
                        currentIndex: root.photoGrid.flagFilter
                        displayText: root.flagName(currentIndex)
                        Accessible.name: qsTr("Show photos by flag")
                        delegate: AppItemDelegate {
                            required property int index
                            width: flagBox.width
                            text: root.flagName(index)
                            highlighted: flagBox.highlightedIndex === index
                        }
                        onActivated: index => {
                            root.filterFlags(index)
                            grid.forceActiveFocus()
                        }
                    }
                    // Only the photos with a colour label: one dot for each, the same one again lists them all.
                    Repeater {
                        id: labelFilterButtons
                        model: ["red", "yellow", "green", "blue", "purple"]
                        AppToolButton {
                            id: dot
                            required property string modelData
                            padding: 4
                            focusPolicy: Qt.NoFocus
                            Accessible.name: root.colourTitle(dot.modelData)
                            ToolTip.visible: hovered
                            ToolTip.text: qsTr("Only the photos labelled %1").arg(root.colourTitle(dot.modelData))
                            contentItem: Rectangle {
                                implicitWidth: 14
                                implicitHeight: 14
                                radius: 7
                                color: Theme.labelColour(dot.modelData)
                                border.width: root.photoGrid.labelFilter === dot.modelData ? 2 : 0
                                border.color: Theme.white
                            }
                            onClicked: {
                                root.filterLabel(dot.modelData)
                                grid.forceActiveFocus()
                            }
                        }
                    }
                    // The place (design note 008 §5): the tree of countries, regions and cities, and what is under a node. It is
                    // not there until the engine can answer for it (`PlaceMenu.available`).
                    PlaceMenu {
                        id: placeMenu
                        grid: root.photoGrid
                    }
                    // Series (WP9): which photos by series, and every series open or closed.
                    AppComboBox {
                        id: seriesBox
                        sizingTexts: [0, 1, 2, 3].map(i => root.seriesName(i))
                        visible: root.photoGrid.seriesCount > 0
                        model: 4
                        focusPolicy: Qt.NoFocus
                        currentIndex: root.photoGrid.seriesFilter
                        displayText: root.seriesName(currentIndex)
                        Accessible.name: qsTr("Show photos by series")
                        delegate: AppItemDelegate {
                            required property int index
                            width: seriesBox.width
                            text: root.seriesName(index)
                            highlighted: seriesBox.highlightedIndex === index
                        }
                        onActivated: index => {
                            root.filterSeries(index)
                            grid.forceActiveFocus()
                        }
                    }
                    AppToolButton {
                        id: expandButton
                        visible: root.photoGrid.seriesCount > 0
                        text: root.allOpen ? qsTr("Close all") : qsTr("Open all")
                        focusPolicy: Qt.NoFocus
                        ToolTip.visible: hovered
                        ToolTip.text: qsTr("Open or close every series (E for the one under the cursor)")
                        onClicked: {
                            root.expandAll(!root.allOpen)
                            grid.forceActiveFocus()
                        }
                    }
                    AppButton {
                        visible: root.photoGrid.keywordFilter !== ""
                        text: qsTr("Keyword: %1").arg(root.keywordFilterName) + " ×"
                        focusPolicy: Qt.NoFocus
                        onClicked: {
                            root.filterKeyword("", "")
                            grid.forceActiveFocus()
                        }
                    }
                    AppButton {
                        visible: root.photoGrid.collectionFilter !== ""
                        text: qsTr("Collection: %1").arg(root.collectionFilterName) + " ×"
                        focusPolicy: Qt.NoFocus
                        onClicked: {
                            root.filterCollection("", "")
                            grid.forceActiveFocus()
                        }
                    }
                    AppSlider {
                        id: sizeSlider
                        Layout.preferredWidth: 90
                        from: 96
                        to: 256
                        stepSize: 8
                        value: root.thumbW
                        focusPolicy: Qt.NoFocus
                        Accessible.name: qsTr("Thumbnail size")
                        ToolTip.visible: hovered
                        ToolTip.text: qsTr("Thumbnail size")
                        onMoved: {
                            root.thumbW = value
                            root.launcher.setIntOption("thumbSize", value)
                        }
                    }
                    Label {
                        text: root.status
                        color: Theme.quiet
                        Layout.leftMargin: 6
                    }
                    // Reads the list again: photos that arrived, and rejected ones that were left in place.
                    AppButton {
                        id: refreshButton
                        text: qsTr("Refresh")
                        focusPolicy: Qt.NoFocus
                        onClicked: {
                            root.reload()
                            grid.forceActiveFocus()
                        }
                    }
                    // Exports the file of every photo the grid lists right now (whatever the filters are), one path
                    // a line: not only for the Rejected view, for tidying up outside Auroraw with any list (D-106).
                    AppButton {
                        id: exportButton
                        text: qsTr("Export the list…")
                        enabled: root.photoGrid.count > 0
                        focusPolicy: Qt.NoFocus
                        ToolTip.visible: hovered
                        ToolTip.text: qsTr("Writes the file of every listed photo to a text file, one path a line")
                        onClicked: {
                            exportDialog.pick()
                            grid.forceActiveFocus()
                        }
                    }
                    Item { Layout.fillWidth: true }
                }
            }

            GridView {
                id: grid
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                focus: true
                boundsBehavior: Flickable.StopAtBounds
                model: root.photoGrid
                cellWidth: root.thumbW + 4
                cellHeight: root.thumbH + 4
                currentIndex: -1
                // The arrows are ours, so that a step down from above a short last row lands on its
                // last photo (`gridmath::step`).
                keyNavigationEnabled: false
                readonly property int columns: Math.max(1, Math.floor(width / cellWidth))
                readonly property int visibleRows: Math.max(1, Math.floor(height / cellHeight))
                // The room the columns leave over is shared by the two sides, so that the grid sits in the middle of its panel
                // and not against the left (D-144). Never negative: with one column wider than the view there is nothing to share.
                leftMargin: Math.max(0, Math.floor((width - columns * cellWidth) / 2))
                // Qt does not move the content when the margin changes under it: the grid would stay where the last margin put
                // it, off centre and out of step with the pointer's area (which reads `leftMargin`). It never scrolls sideways.
                flickableDirection: Flickable.VerticalFlick
                onLeftMarginChanged: contentX = originX - leftMargin

                ScrollBar.vertical: AppScrollBar {}

                // The window was resized and the rows re-flowed: the cursor stays in view (once the
                // view has laid its cells out again).
                onColumnsChanged: Qt.callLater(keepCursorInView)

                function keepCursorInView() {
                    if (currentIndex >= 0)
                        positionViewAtIndex(currentIndex, GridView.Contain)
                }

                // Moves the cursor by a step or a jump; the modifiers say what that does to the selection.
                function move(dx, dy, modifiers) {
                    if (count === 0)
                        return
                    // With no cursor, any move goes to the first photo.
                    root.goTo(currentIndex < 0 ? 0 : root.photoGrid.step(currentIndex, dx, dy, columns), modifiers)
                }

                function jump(kind, modifiers) {
                    if (count === 0)
                        return
                    root.goTo(root.photoGrid.jump(kind, Math.max(currentIndex, 0), columns, visibleRows), modifiers)
                }

                Keys.onPressed: event => {
                    if (event.modifiers & (Qt.AltModifier | Qt.MetaModifier))
                        return
                    const ctrlOrShift = event.modifiers & (Qt.ControlModifier | Qt.ShiftModifier)
                    if (event.key >= Qt.Key_0 && event.key <= Qt.Key_5) {
                        if (ctrlOrShift)
                            return
                        root.rate(event.key - Qt.Key_0)
                    } else if (event.key === Qt.Key_P || event.key === Qt.Key_X || event.key === Qt.Key_U) {
                        if (ctrlOrShift)
                            return
                        root.flag(event.key === Qt.Key_P ? "pick" : event.key === Qt.Key_X ? "reject" : "clear")
                    } else if (event.key >= Qt.Key_6 && event.key <= Qt.Key_9) {
                        if (ctrlOrShift)
                            return
                        root.label(["red", "yellow", "green", "blue"][event.key - Qt.Key_6])
                    } else if (event.key === Qt.Key_G && (event.modifiers & Qt.ControlModifier)) {
                        if (event.modifiers & Qt.ShiftModifier)
                            root.ungroup()
                        else
                            root.group()
                    } else if (event.key === Qt.Key_M) {
                        if (ctrlOrShift)
                            return
                        root.showSimilar(!root.similarShown)
                    } else if (event.key === Qt.Key_C || event.key === Qt.Key_K) {
                        if (ctrlOrShift)
                            return
                        if (event.key === Qt.Key_C)
                            root.openCompare()
                        else if (grid.currentIndex >= 0)
                            root.toggleMark(root.photoGrid.idAt(grid.currentIndex))
                    } else if (event.key === Qt.Key_E || event.key === Qt.Key_R) {
                        if (ctrlOrShift)
                            return
                        if (event.key === Qt.Key_E)
                            root.toggleSeries(-1)
                        else
                            root.resolve()
                    } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                        if (ctrlOrShift)
                            return
                        root.openView(-1)
                    } else if (event.key === Qt.Key_Left) {
                        move(-1, 0, event.modifiers)
                    } else if (event.key === Qt.Key_Right) {
                        move(1, 0, event.modifiers)
                    } else if (event.key === Qt.Key_Up) {
                        move(0, -1, event.modifiers)
                    } else if (event.key === Qt.Key_Down) {
                        move(0, 1, event.modifiers)
                    } else if (event.key === Qt.Key_PageUp) {
                        jump("page-up", event.modifiers)
                    } else if (event.key === Qt.Key_PageDown) {
                        jump("page-down", event.modifiers)
                    } else if (event.key === Qt.Key_Home) {
                        jump("home", event.modifiers)
                    } else if (event.key === Qt.Key_End) {
                        jump("end", event.modifiers)
                    } else if (event.key === Qt.Key_Space) {
                        if (currentIndex >= 0)
                            root.toggle(currentIndex)
                    } else if (event.key === Qt.Key_Escape) {
                        if (root.selectedCount === 0)
                            return
                        root.selectNone()
                    } else {
                        return
                    }
                    event.accepted = true
                }

                // Every click and drag on the grid, under the cells (they are not clickable themselves): a press
                // on a photo selects it as the modifiers say, a press on empty space clears the selection, and
                // a drag from anywhere is a rubber band. It does not let the view take the drag to scroll.
                MouseArea {
                    id: pointer
                    // In the view's content (so that what it reports is where the photos are, scrolled or
                    // not), under the cells.
                    parent: grid.contentItem
                    z: -1
                    // (The view's content starts at its origin, which is not always 0: after the cells were made another
                    // size, the first row can sit lower than 0 in the content's own coordinates.)
                    // With the grid centred (`leftMargin`) the view starts `gutter` before the content: the area covers
                    // it too, so that a press in the margin clears the selection as a press on any empty space does.
                    readonly property real gutter: grid.leftMargin
                    x: grid.originX - gutter
                    y: grid.originY
                    width: grid.width
                    height: Math.max(grid.contentHeight, grid.height)
                    preventStealing: true
                    acceptedButtons: Qt.LeftButton | Qt.RightButton

                    property real startX: 0
                    property real startY: 0
                    property real lastX: 0
                    property real lastY: 0
                    property int modifiers: 0
                    property bool onEmpty: false
                    property bool banding: false

                    // The photo under a point of the view's content, -1 for none (a gap, empty space).
                    function photoAt(pointerX, y) {
                        const x = pointerX - gutter
                        const column = Math.floor(x / grid.cellWidth)
                        const row = Math.floor(y / grid.cellHeight)
                        const inside = x - column * grid.cellWidth >= 4 && x - column * grid.cellWidth < grid.cellWidth
                                       && y - row * grid.cellHeight < root.thumbH
                        const index = row * grid.columns + column
                        return inside && column >= 0 && column < grid.columns && index >= 0 && index < grid.count ? index : -1
                    }

                    // The rubber band, now: the cells it covers are selected.
                    function band() {
                        const left = Math.min(startX, lastX), right = Math.max(startX, lastX)
                        const top = Math.min(startY, lastY), bottom = Math.max(startY, lastY)
                        // (The columns from the content's own left, not the pointer area's: the margin is not a column.)
                        root.photoGrid.rubberTo(Math.floor(top / grid.cellHeight), Math.floor(bottom / grid.cellHeight),
                                                Math.max(0, Math.floor((left - gutter) / grid.cellWidth)),
                                                Math.min(grid.columns - 1, Math.max(0, Math.floor((right - gutter) / grid.cellWidth))),
                                                grid.columns)
                        root.updateSummary()
                    }

                    onPressed: mouse => {
                        grid.forceActiveFocus()
                        if (mouse.button === Qt.RightButton) {
                            // A menu for what is under the pointer: a photo outside the selection becomes the selection.
                            const under = photoAt(mouse.x, mouse.y)
                            if (under >= 0) {
                                if (!root.photoGrid.isSelected(under))
                                    root.goTo(under, 0)
                                cellMenu.popup(pointer, mouse.x, mouse.y)
                            }
                            banding = false
                            onEmpty = false
                            return
                        }
                        startX = lastX = mouse.x
                        startY = lastY = mouse.y
                        modifiers = mouse.modifiers
                        banding = false
                        const index = photoAt(mouse.x, mouse.y)
                        onEmpty = index < 0
                        if (index >= 0) {
                            if ((modifiers & Qt.ControlModifier) && !(modifiers & Qt.ShiftModifier))
                                root.toggle(index)
                            else
                                root.goTo(index, modifiers)
                        }
                    }

                    onPositionChanged: mouse => {
                        if (!(pressedButtons & Qt.LeftButton))
                            return
                        lastX = Math.max(0, Math.min(mouse.x, width))
                        lastY = Math.max(0, Math.min(mouse.y, height))
                        if (!banding && Math.abs(lastX - startX) + Math.abs(lastY - startY) > 6) {
                            banding = true
                            root.photoGrid.rubberBegin((modifiers & Qt.ControlModifier) !== 0)
                        }
                        if (banding)
                            band()
                    }

                    onDoubleClicked: mouse => {
                        const index = photoAt(mouse.x, mouse.y)
                        if (index >= 0)
                            root.openView(index)
                    }

                    onReleased: {
                        if (banding)
                            root.photoGrid.rubberEnd()
                        else if (onEmpty && !(modifiers & (Qt.ControlModifier | Qt.ShiftModifier)))
                            root.selectNone()
                        banding = false
                        autoScroll.stop()
                    }

                    // A rubber band held at an edge of the view scrolls it.
                    Timer {
                        id: autoScroll
                        interval: 30
                        repeat: true
                        running: pointer.banding && pointer.pressed
                        onTriggered: {
                            const shown = pointer.lastY + grid.originY - grid.contentY
                            const step = shown < 24 ? -20 : shown > grid.height - 24 ? 20 : 0
                            if (step === 0)
                                return
                            grid.contentY = Math.max(grid.originY, Math.min(grid.contentY + step,
                                                                            grid.originY + Math.max(0, grid.contentHeight - grid.height)))
                            pointer.lastY = Math.max(0, Math.min(pointer.lastY + step, pointer.height))
                            pointer.band()
                        }
                    }
                }

                delegate: Item {
                    id: cell
                    required property int index
                    required property string photoId
                    required property int rating
                    required property bool selected
                    required property int flag
                    required property string colourLabel
                    required property string seriesId
                    required property int seriesSize
                    required property int seriesTotal
                    required property bool seriesResolved
                    required property bool seriesOpen
                    required property int seriesEdge
                    required property bool marked
                    required property bool isRawOriginal
                    required property bool isMissing
                    // A series is one thumbnail with a count, that opens in place.
                    readonly property bool inSeries: cell.seriesId !== "" && cell.seriesTotal > 1
                    // How many photos the series' badge counts: those listed, and of how many when the filters hide some.
                    readonly property string badgeCount: cell.seriesSize < cell.seriesTotal ? cell.seriesSize + "/" + cell.seriesTotal : String(cell.seriesSize)
                    readonly property bool collapsed: cell.inSeries && cell.seriesSize > 1 && !cell.seriesOpen
                    // No thumbnail can be made for this photo (it says so instead of staying empty).
                    readonly property bool unavailable: thumbnail.status === Image.Error
                    readonly property bool shown: thumbnail.status === Image.Ready
                    width: grid.cellWidth
                    height: grid.cellHeight
                    Accessible.role: Accessible.ListItem
                    Accessible.selected: cell.selected
                    Accessible.name: cell.rating > 0 ? qsTr("Photo, %n star(s)", "", cell.rating) : qsTr("Photo")

                    // A collapsed series looks like a pile: two edges under the picture.
                    Rectangle {
                        x: 9
                        y: root.thumbH + 1
                        width: root.thumbW - 10
                        height: 2
                        visible: cell.collapsed
                        color: Theme.surface.border
                    }
                    Rectangle {
                        x: 14
                        y: root.thumbH + 3
                        width: root.thumbW - 20
                        height: 1
                        visible: cell.collapsed
                        color: Theme.surface.border
                    }
                    // The members of an open series are joined by a line under them.
                    Rectangle {
                        x: cell.seriesEdge === 1 ? 4 : 0
                        y: root.thumbH + 1
                        width: cell.seriesEdge === 1 ? root.thumbW : grid.cellWidth
                        height: 3
                        visible: cell.seriesOpen && cell.seriesEdge !== 0
                        color: cell.seriesResolved ? Theme.picked : Theme.accent
                    }
                    Rectangle {
                        x: 4
                        width: root.thumbW
                        height: root.thumbH
                        color: root.palette.dark

                        Image {
                            id: thumbnail
                            anchors.fill: parent
                            source: "image://thumbs/" + cell.photoId
                            fillMode: Image.PreserveAspectFit
                            asynchronous: true
                            // A rejected photo is dimmed, and stays where it is until the list is read again; a
                            // missing one more so, since its thumbnail (kept from before its file went away) is
                            // now only a memory of it.
                            opacity: cell.isMissing ? 0.3 : (cell.flag === 2 ? 0.35 : 1)
                        }
                        // A photo no thumbnail can be made for (an unreadable file, a RAW without a preview).
                        Label {
                            anchors.centerIn: parent
                            visible: cell.unavailable && !cell.isMissing
                            text: qsTr("No preview")
                            color: Theme.surface.placeholder
                        }
                        // The last scan of this photo's source no longer found its file there (issue #7): its
                        // thumbnail may still be the one cached from before, so this is said outright rather than
                        // left to "No preview" (which would not even show when a cached thumbnail still loads).
                        Label {
                            anchors.centerIn: parent
                            visible: cell.isMissing
                            text: qsTr("Missing")
                            color: Theme.danger
                        }
                        // What is selected is tinted, so that a set reads at a glance.
                        Rectangle {
                            anchors.fill: parent
                            visible: cell.selected
                            color: Theme.accent
                            opacity: 0.38
                        }
                        // The rating, on a dark chip so that it reads over any picture.
                        Rectangle {
                            x: 4
                            y: 4
                            visible: cell.rating > 0
                            width: stars.implicitWidth + 8
                            height: stars.implicitHeight + 2
                            radius: Theme.radiusControl
                            color: Theme.scrimLight
                            AppRatingMark {
                                id: stars
                                anchors.centerIn: parent
                                rating: cell.rating
                            }
                        }
                        // The flag: picked ✔, rejected ✖.
                        Rectangle {
                            anchors.right: parent.right
                            anchors.top: parent.top
                            anchors.margins: 4
                            visible: cell.flag !== 0
                            width: flagMark.width + 8
                            height: flagMark.height + 2
                            radius: Theme.radiusControl
                            color: Theme.scrimLight
                            AppIcon {
                                id: flagMark
                                anchors.centerIn: parent
                                name: cell.flag === 1 ? "check" : "close"
                                size: 15
                                color: cell.flag === 1 ? Theme.picked : Theme.danger
                                Accessible.ignored: false
                                Accessible.role: Accessible.StaticText
                                Accessible.name: cell.flag === 1 ? qsTr("Picked") : qsTr("Rejected")
                            }
                        }
                        // The series' badge: how many photos, a tick once it is resolved; a click opens or closes it.
                        Rectangle {
                            id: badge
                            anchors.right: parent.right
                            anchors.bottom: parent.bottom
                            anchors.margins: 4
                            anchors.bottomMargin: 9
                            visible: cell.inSeries && (cell.collapsed || cell.seriesEdge <= 1)
                            width: badgeRow.implicitWidth + 10
                            height: badgeRow.implicitHeight + 2
                            radius: Theme.radiusControl
                            color: Theme.scrimStrong
                            Accessible.role: Accessible.StaticText
                            Accessible.name: qsTr("Series of %n photo(s)", "", cell.seriesTotal)
                            // A tick once resolved, the series' icon (an arrow while it is open), and how many photos are
                            // listed, and of how many when the filters hide some of the series.
                            Row {
                                id: badgeRow
                                anchors.centerIn: parent
                                spacing: 3
                                AppIcon {
                                    visible: cell.seriesResolved
                                    name: "check"
                                    size: 13
                                    color: Theme.picked
                                    anchors.verticalCenter: badgeText.verticalCenter
                                }
                                AppIcon {
                                    name: cell.seriesOpen ? "caret-down" : "series"
                                    size: 13
                                    color: badgeText.color
                                    anchors.verticalCenter: badgeText.verticalCenter
                                }
                                Text {
                                    id: badgeText
                                    text: cell.badgeCount
                                    color: cell.seriesResolved ? Theme.picked : Theme.white
                                }
                            }
                            MouseArea {
                                anchors.fill: parent
                                anchors.margins: -3
                                // With one photo of the series listed there is nothing to open.
                                enabled: cell.seriesSize > 1
                                hoverEnabled: true
                                onClicked: root.toggleSeries(cell.index)
                                ToolTip.visible: containsMouse && cell.seriesSize < cell.seriesTotal
                                ToolTip.text: qsTr("%1 of the series' %2 photos are listed (the filters hide the others)")
                                              .arg(cell.seriesSize).arg(cell.seriesTotal)
                            }
                        }
                        // No RAW file for this photo (issue #5): only the JPEG (or other standard format) it was
                        // imported with, so there is nothing left to develop.
                        Rectangle {
                            anchors.left: parent.left
                            anchors.bottom: parent.bottom
                            anchors.margins: 4
                            anchors.bottomMargin: 9
                            visible: !cell.isRawOriginal
                            width: noRawText.implicitWidth + 8
                            height: noRawText.implicitHeight + 2
                            radius: Theme.radiusControl
                            color: Theme.scrimLight
                            Text {
                                id: noRawText
                                anchors.centerIn: parent
                                text: qsTr("No RAW")
                                color: Theme.surface.placeholder
                            }
                        }
                        // The colour label, a bar along the bottom of the picture.
                        Rectangle {
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.bottom: parent.bottom
                            height: 5
                            visible: cell.colourLabel !== ""
                            color: Theme.labelColour(cell.colourLabel)
                        }
                        // A frame marked to keep (a draft until the series is resolved): a green ring.
                        Rectangle {
                            anchors.fill: parent
                            visible: cell.marked
                            color: "transparent"
                            border.width: 3
                            border.color: Theme.picked
                        }
                        // The selection's frame, over the picture.
                        Rectangle {
                            anchors.fill: parent
                            color: "transparent"
                            border.width: cell.selected ? 3 : 0
                            border.color: Theme.accent
                        }
                        // The cursor, when it is not the only thing selected: where the keyboard is.
                        Rectangle {
                            anchors.fill: parent
                            anchors.margins: 3
                            visible: cell.GridView.isCurrentItem && (root.selectedCount !== 1 || !cell.selected)
                            color: "transparent"
                            border.width: 1
                            border.color: root.palette.windowText
                        }
                    }
                }
            }

            // What is selected: the photo, or how many photos (the panels come with a later work
            // package); or, while a selection edit runs as a background job (D-126 volet B, past
            // `BACKGROUND_THRESHOLD` items), its own progress and a way to cancel it.
            Rectangle {
                id: statusStrip
                Layout.fillWidth: true
                Layout.preferredHeight: 28
                color: root.palette.window
                readonly property bool running: root.photoGrid.batchJob !== ""

                Connections {
                    target: Bus
                    function onJobProgress(job, done, total) {
                        if (job !== root.photoGrid.batchJob || total <= 0)
                            return
                        batchProgress.value = done / total
                        batchLabel.text = qsTr("Applying to %1 of %2 photo(s)…").arg(done).arg(total)
                    }
                    function onJobFinished(job) {
                        if (job === root.photoGrid.batchJob)
                            root.photoGrid.batchJob = ""
                    }
                    function onJobCancelled(job) {
                        if (job === root.photoGrid.batchJob)
                            root.photoGrid.batchJob = ""
                    }
                }

                AppIconLabel {
                    visible: !statusStrip.running
                    x: 6
                    anchors.verticalCenter: parent.verticalCenter
                    // What the engine wrote holds the star, the tick and the cross for the rating and the flag: shown as icons.
                    sentence: root.summary
                    color: Theme.quiet
                    width: parent.width - 12
                }

                RowLayout {
                    visible: statusStrip.running
                    anchors.fill: parent
                    anchors.leftMargin: 6
                    anchors.rightMargin: 6
                    spacing: 8
                    AppProgressBar {
                        id: batchProgress
                        Layout.preferredWidth: 120
                    }
                    Label {
                        id: batchLabel
                        Layout.fillWidth: true
                        color: Theme.quiet
                        elide: Text.ElideRight
                    }
                    AppButton {
                        text: qsTr("Cancel")
                        onClicked: root.photoGrid.cancelBatch()
                    }
                }
            }
        }

        SimilarPanel {
            id: similarPanel
            visible: root.similarShown
            library: root
            photoGrid: root.photoGrid
            launcher: root.launcher
        }

        KeywordPanel {
            id: keywordPanel
            Layout.fillHeight: true
            keywords: root.keywords
            collections: root.collections
            photoGrid: root.photoGrid
            library: root
            launcher: root.launcher
        }
    }

    // The mouse's way to what the keys do to the selection, and to purple (which has no key).
    component MarkItem: MenuItem {
        id: mark
        property string colour: ""
        property string keyHint: ""
        contentItem: RowLayout {
            spacing: 10
            Rectangle {
                visible: mark.colour !== ""
                implicitWidth: 12
                implicitHeight: 12
                radius: 6
                color: Theme.labelColour(mark.colour)
            }
            Label {
                Layout.fillWidth: true
                text: mark.text
                color: mark.enabled ? mark.palette.windowText : mark.palette.placeholderText
            }
            Label {
                text: mark.keyHint
                color: Theme.quiet
            }
        }
    }

    // The grid's menu: the same rows as the view's, and one to open the photo in the view.
    AppSubMenu {
        id: cellMenu
        MarkItem { text: qsTr("Open in the image view"); keyHint: "↵"; onTriggered: root.openView(-1) }
        MarkItem { text: qsTr("Show in file manager"); onTriggered: root.revealCell(-1) }
        MenuSeparator {}
        MarkItem { text: qsTr("Similar photos"); keyHint: "M"; onTriggered: root.showSimilar(!root.similarShown) }
        MarkItem { text: qsTr("Open or close the series"); keyHint: "E"; onTriggered: root.toggleSeries(-1) }
        MarkItem { text: qsTr("Group as a series"); keyHint: "Ctrl+G"; enabled: root.canGroup; onTriggered: root.group() }
        MarkItem { text: qsTr("Take out of the series"); keyHint: "Ctrl+Shift+G"; enabled: root.canUngroup; onTriggered: root.ungroup() }
        MarkItem { text: qsTr("Resolve the series"); keyHint: "R"; enabled: root.canResolve; onTriggered: root.resolve() }
        MarkItem { text: qsTr("Reopen the series"); enabled: root.canReopen; onTriggered: root.reopen() }
        MenuSeparator {}
        MarkItem { text: root.colourTitle("red"); colour: "red"; keyHint: "6"; onTriggered: root.label("red") }
        MarkItem { text: root.colourTitle("yellow"); colour: "yellow"; keyHint: "7"; onTriggered: root.label("yellow") }
        MarkItem { text: root.colourTitle("green"); colour: "green"; keyHint: "8"; onTriggered: root.label("green") }
        MarkItem { text: root.colourTitle("blue"); colour: "blue"; keyHint: "9"; onTriggered: root.label("blue") }
        MarkItem { text: root.colourTitle("purple"); colour: "purple"; onTriggered: root.label("purple") }
        MarkItem { text: root.colourTitle("none"); onTriggered: root.label("none") }
        MenuSeparator {}
        MarkItem { text: qsTr("Pick"); enabled: root.canFlag; keyHint: "P"; onTriggered: root.flag("pick") }
        MarkItem { text: qsTr("Reject"); enabled: root.canFlag; keyHint: "X"; onTriggered: root.flag("reject") }
        MarkItem { text: qsTr("Clear the flag"); enabled: root.canFlag; keyHint: "U"; onTriggered: root.flag("clear") }
    }

    // The image view's menu: the same rows without the one that opens the view (a menu of its own rather than a
    // hidden row, which leaves a gap).
    AppSubMenu {
        id: viewMenu
        MarkItem { text: qsTr("Show in file manager"); onTriggered: root.revealCell(-1) }
        MenuSeparator {}
        MarkItem { text: root.colourTitle("red"); colour: "red"; keyHint: "6"; onTriggered: root.label("red") }
        MarkItem { text: root.colourTitle("yellow"); colour: "yellow"; keyHint: "7"; onTriggered: root.label("yellow") }
        MarkItem { text: root.colourTitle("green"); colour: "green"; keyHint: "8"; onTriggered: root.label("green") }
        MarkItem { text: root.colourTitle("blue"); colour: "blue"; keyHint: "9"; onTriggered: root.label("blue") }
        MarkItem { text: root.colourTitle("purple"); colour: "purple"; onTriggered: root.label("purple") }
        MarkItem { text: root.colourTitle("none"); onTriggered: root.label("none") }
        MenuSeparator {}
        MarkItem { text: qsTr("Pick"); enabled: root.canFlag; keyHint: "P"; onTriggered: root.flag("pick") }
        MarkItem { text: qsTr("Reject"); enabled: root.canFlag; keyHint: "X"; onTriggered: root.flag("reject") }
        MarkItem { text: qsTr("Clear the flag"); enabled: root.canFlag; keyHint: "U"; onTriggered: root.flag("clear") }
    }

    property alias exportButton: exportButton
    property alias exportDialog: exportDialog
    FileSaveDialog {
        id: exportDialog
        hostWindow: root.hostWindow
        defaultFileName: "photos.txt"
        rememberAs: "export-photos"
        onChosen: path => root.photoGrid.exportListedTo(path)
    }

    // Two to four photos side by side (spec §5.3, D-103).
    Compare {
        id: compareView
        anchors.fill: parent
        visible: root.comparing
        library: root
        launcher: root.launcher
    }

    // One photo at a time, over the grid, the keyword panel and the filter bar (spec §5.3).
    Viewer {
        id: viewer
        anchors.fill: parent
        visible: root.viewing
        library: root
        launcher: root.launcher
    }
}
