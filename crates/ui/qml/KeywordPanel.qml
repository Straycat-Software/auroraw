// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// The inspector, on the right of the grid, four tabs (WP10, slice 1 added Metadata, a later change
// added Info, slice 3 Collections; before that this was only the Keywords panel of spec §5.7, D-098, and
// the file kept that name): the vocabulary as a tree, each keyword with a check that says whether the selected photos
// carry it none, some or all, and the number of photos that have it. A click on the check gives it to
// the whole selection, or takes it off; the field above types ahead (it filters the tree), Enter assigns
// the best match, and creates the keyword when nothing matches (Shift+Enter creates even when something
// does). Every assignment is one action, one step of the history, and so is making a keyword (with the
// photos that first get it), renaming, moving, deleting one and setting its synonyms and export flag
// (WP10 slice 2, Properties… in its menu). A keyword is moved by dragging it onto another (or onto any
// place of the panel that has no keyword, for the top level), or from its menu. Metadata is
// `MetadataPanel.qml`; Info, the grid's active photo's own technical metadata, read-only, is
// `InfoPanel.qml`; the manual collections are `CollectionPanel.qml`. This file keeps the shared chrome (the resizable, collapsible shell) every tab shows
// through.
Rectangle {
    id: panel
    required property var keywords
    required property var collections
    required property var photoGrid
    required property var library
    required property var launcher

    property bool expanded: true
    // New keywords go under this one (an identifier), if a keyword was clicked.
    property string createUnder: ""
    property string createUnderName: ""
    property alias filterField: field
    property alias renameDialog: renameDialog
    property alias tree: tree
    property alias collapseButton: collapseButton
    property alias moveDialog: moveDialog
    property alias deleteDialog: deleteDialog
    property alias propertiesDialog: propertiesDialog
    property alias topLevelDrop: topLevelDrop
    property alias contextMenu: menu
    property alias addButton: addButton
    property alias ghost: ghost
    property alias tabs: tabs
    property alias metadataPanel: metadataPanel
    property alias infoPanel: infoPanel
    property alias collectionPanel: collectionPanel
    // A keyword is being dragged (the top-level strip shows).
    property bool dragging: false

    // The width the person dragged the panel to (double-click on the edge gives back the default).
    readonly property int defaultWidth: 320
    readonly property int minimumWidth: 200
    readonly property int maximumWidth: 640
    property int panelWidth: defaultWidth
    // The remembered width is read when the library is shown (the launcher knows its folders by then).
    Connections {
        target: panel.library
        function onVisibleChanged() {
            if (panel.library.visible && !edge.pressed)
                panel.panelWidth = panel.launcher.keywordPanelWidth()
        }
    }
    property alias edge: edge

    // A tab of the bar above, in its `slot`: its width is `tabRow.tabWidth`'s to decide (its label is cut short
    // when there is no room).
    component PanelTab: TabButton {
        id: tab
        required property int slot
        text: tabRow.tabTexts[slot]
        width: Math.ceil(tabRow.tabWidth(slot))
        horizontalPadding: Math.min(8, Math.max(1, (width - tabRow.labelWidth(slot)) / 2))
        // D-129: the same transparent-tab, accent-underline look as the header's AppTabButton.
        background: Rectangle {
            color: "transparent"
            Rectangle {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                height: 2
                color: tab.checked ? Theme.accent : "transparent"
                Behavior on color { ColorAnimation { duration: Theme.motion } }
            }
            // Keyboard focus only (`visualFocus`): an outline round the tab, since the underline alone
            // says which tab is current, not which one has the focus.
            Rectangle {
                anchors.fill: parent
                anchors.margins: 1
                radius: Theme.radiusControl
                color: "transparent"
                border.width: 2
                border.color: Theme.accent
                visible: tab.visualFocus
            }
        }
        contentItem: Label {
            text: tab.text
            font: tab.font
            color: tab.checked ? tab.palette.buttonText : Theme.quiet
            elide: Text.ElideRight
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            Behavior on color { ColorAnimation { duration: Theme.motion } }
        }
    }

    Layout.preferredWidth: expanded ? panelWidth : 30
    color: palette.window

    Rectangle {
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: 1
        color: edge.containsMouse || edge.pressed ? Theme.accent : palette.dark
    }

    // Anywhere in the panel where there is no keyword is a place to drop one to make it a top-level keyword
    // (the rows and the edge, in front of it, take the drops that are theirs).
    DropArea {
        id: topLevelDrop
        anchors.fill: parent
        keys: ["keyword"]
        property bool allowed: false
        onEntered: drag => allowed = panel.keywords.canMove(drag.source.keywordId, "")
        onDropped: drop => panel.dropOn(drop.source.keywordId, "")
    }
    Rectangle {
        anchors.fill: parent
        visible: topLevelDrop.containsDrag && topLevelDrop.allowed
        color: Theme.accent
        opacity: 0.18
        border.color: Theme.accent
        border.width: 2
    }

    // The edge beside the grid: drag it to widen or narrow the panel.
    MouseArea {
        id: edge
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: 6
        visible: panel.expanded
        hoverEnabled: true
        cursorShape: Qt.SizeHorCursor
        property real startX: 0
        property int startWidth: 0
        onPressed: mouse => {
            startX = mapToItem(null, mouse.x, 0).x
            startWidth = panel.panelWidth
        }
        onPositionChanged: mouse => {
            if (pressed) {
                const dx = startX - mapToItem(null, mouse.x, 0).x
                panel.panelWidth = Math.max(panel.minimumWidth, Math.min(panel.maximumWidth, Math.round(startWidth + dx)))
            }
        }
        // The width is remembered once the drag is over, not at every pixel.
        onReleased: panel.launcher.setKeywordPanelWidth(panel.panelWidth)
        onDoubleClicked: {
            panel.panelWidth = panel.defaultWidth
            panel.launcher.setKeywordPanelWidth(panel.panelWidth)
        }
    }

    // Shows the panel on its Keywords tab and puts the keyboard in its field.
    function open() {
        expanded = true
        tabs.currentIndex = 0
        field.forceActiveFocus()
        field.selectAll()
    }

    // Gives a keyword to the selection, or takes it off; false when nothing is selected.
    function assign(id, add) {
        if (photoGrid.selectedCount === 0)
            return false
        photoGrid.keywordSelection(id, add)
        return true
    }

    // Enter in the field: the best match is assigned; when nothing matches (or with `forceCreate`), what was
    // typed becomes a keyword, under the one that was clicked if any, and is assigned: making it and giving it
    // to the selection is one step of the history.
    function commit(forceCreate) {
        const typed = field.text.trim()
        if (typed === "")
            return
        let id = ""
        if (!forceCreate) {
            const row = keywords.bestMatch(typed)
            if (row >= 0)
                id = keywords.idAt(row)
        }
        if (id === "")
            id = keywords.findSibling(typed, createUnder)
        if (id !== "") {
            note = ""
            assign(id, true)
        } else {
            const made = photoGrid.selectedCount > 0 ? photoGrid.createKeywordSelection(typed, createUnder)
                                                      : keywords.create(typed, createUnder)
            if (made.indexOf("error:") === 0) {
                note = explain(made.substring(6))
                return
            }
            note = ""
            keywords.refresh()
        }
        field.text = ""
    }

    // A refusal, in words: the models answer with a code (`name`, `taken:<name>`, `cycle`, or `other:` and the
    // engine's own English), the sentence is the interface's, so that it is translated. Empty for no refusal.
    function explain(code) {
        if (code === "")
            return ""
        if (code === "name")
            return qsTr("A keyword needs a name, without |.")
        if (code === "synonym")
            return qsTr("A synonym cannot contain |.")
        if (code === "cycle")
            return qsTr("A keyword cannot be moved under itself or under one of its own keywords.")
        if (code.indexOf("taken:") === 0)
            return qsTr("There is already a keyword named “%1” there.").arg(code.substring(6))
        return code.indexOf("other:") === 0 ? code.substring(6) : code
    }

    // What was typed can be added as a keyword where new keywords go (nothing of that name is there yet). Enter
    // gives the best match instead, which may be a keyword of the same name elsewhere in the tree: this is
    // how to make a second one (Shift+Enter does the same from the keyboard).
    readonly property string typed: field.text.trim()
    readonly property bool canAdd: (keywords.count, typed !== "" && keywords.findSibling(typed, createUnder) === "")

    // The pointer was released: the ghost drops (on the row or the strip under it) and goes.
    function endDrag() {
        ghost.Drag.drop()
        ghost.Drag.active = false
        ghost.visible = false
        dragging = false
    }

    // A keyword was dropped on `parent` (an identifier; empty for the top level).
    function dropOn(id, parent) {
        if (id === parent)
            return
        note = explain(keywords.moveKeyword(id, parent))
    }

    property string note: ""

    ColumnLayout {
        id: content
        anchors.fill: parent
        anchors.leftMargin: 8
        anchors.rightMargin: 6
        anchors.topMargin: 6
        anchors.bottomMargin: 6
        spacing: 6
        visible: panel.expanded

        RowLayout {
            id: tabRow
            Layout.fillWidth: true
            // The tabs' labels, and (measured by labels of their own, out of the bar, which are never cut) how wide
            // each needs to be shown whole. The four are wider than the panel's default width in French, and the
            // person can narrow the panel: the tabs give up their padding first (down to 1 px), then the labels
            // are cut short, all sharing what there is.
            readonly property var tabTexts: [qsTr("Keywords"), qsTr("Metadata"), qsTr("Info"), qsTr("Collections")]
            Repeater {
                id: measures
                model: tabRow.tabTexts
                Label {
                    required property string modelData
                    visible: false
                    text: modelData
                }
            }
            function labelWidth(slot) {
                const measure = measures.itemAt(slot)
                return measure ? Math.ceil(measure.implicitWidth) : 0
            }
            readonly property real labelsTotal: {
                let sum = 0
                for (let i = 0; i < measures.count; i++)
                    sum += labelWidth(i)
                return sum
            }
            // What the panel's width leaves the bar. (Not the row's or the bar's own `width`: an item's width is its
            // implicit width until a layout sets it, and the tabs' widths would depend on themselves, through the
            // bar's implicit width.)
            readonly property real room: panel.panelWidth - content.anchors.leftMargin - content.anchors.rightMargin
                                         - collapseButton.implicitWidth - spacing
            function tabWidth(slot) {
                const n = Math.max(measures.count, 1)
                const label = labelWidth(slot)
                // The generous ceiling was 8 (4px padding a side); Patrick's own review of D-129 found the
                // inactive tabs, with no fill of their own, read as one run-on string without more room.
                if (room >= labelsTotal + 16 * n)
                    return label + 16
                if (room >= labelsTotal + 4 * n)
                    return label + 4 + (room - labelsTotal - 4 * n) / n
                return labelsTotal > 0 ? Math.max(room, 0) * label / labelsTotal : label
            }
            TabBar {
                id: tabs
                Layout.fillWidth: true
                background: null
                PanelTab { slot: 0 }
                PanelTab { slot: 1 }
                PanelTab { slot: 2 }
                PanelTab { slot: 3 }
            }
            AppToolButton {
                id: collapseButton
                text: "»"
                focusPolicy: Qt.NoFocus
                Accessible.name: qsTr("Hide the panel")
                onClicked: panel.expanded = false
            }
        }

        AppTextField {
            id: field
            visible: tabs.currentIndex === 0
            Layout.fillWidth: true
            placeholderText: qsTr("Find or add a keyword…")
            Accessible.name: qsTr("Find or add a keyword")
            onTextChanged: {
                panel.keywords.setFilter(text)
                panel.note = ""
            }
            Keys.onReturnPressed: event => panel.commit((event.modifiers & Qt.ShiftModifier) !== 0)
            Keys.onEnterPressed: event => panel.commit((event.modifiers & Qt.ShiftModifier) !== 0)
            Keys.onEscapePressed: {
                text = ""
                panel.library.grid.forceActiveFocus()
            }
        }

        // Where a new keyword will go, once a keyword was clicked.
        // One line that always has its place, so that the list below does not move: the button that adds what was
        // typed, else where new keywords go (once a keyword was clicked).
        RowLayout {
            visible: tabs.currentIndex === 0
            Layout.fillWidth: true
            Layout.preferredHeight: 30
            Layout.maximumHeight: 30
            AppButton {
                id: addButton
                visible: panel.canAdd
                Layout.fillWidth: true
                focusPolicy: Qt.NoFocus
                text: panel.createUnder === "" ? qsTr("Add “%1” at the top level").arg(panel.typed)
                                               : qsTr("Add “%1” under %2").arg(panel.typed).arg(panel.createUnderName)
                contentItem: Label {
                    text: addButton.text
                    elide: Text.ElideRight
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Shift+Enter")
                onClicked: {
                    panel.commit(true)
                    field.forceActiveFocus()
                }
            }
            Label {
                visible: !panel.canAdd && panel.createUnder !== ""
                Layout.fillWidth: true
                text: qsTr("New keywords go under %1").arg(panel.createUnderName)
                color: Theme.quiet
                elide: Text.ElideRight
            }
            AppToolButton {
                visible: !panel.canAdd && panel.createUnder !== ""
                text: "×"
                focusPolicy: Qt.NoFocus
                Accessible.name: qsTr("New keywords go at the top level")
                onClicked: panel.createUnder = ""
            }
            Item { Layout.fillWidth: !panel.canAdd && panel.createUnder === "" }
        }

        AppListFrame {
            visible: tabs.currentIndex === 0
            Layout.fillWidth: true
            Layout.fillHeight: true

            AppListView {
                id: tree
                anchors.fill: parent
                model: panel.keywords

                delegate: Item {
                    id: row
                    required property int index
                    required property string keywordId
                    required property string name
                    required property int depth
                    required property int photos
                    required property int carried
                    required property bool hasChildren
                    required property bool expanded
                    width: tree.rowWidth
                    height: 28

                    // Dropping a keyword here makes it a child of this one (when that is possible).
                    Rectangle {
                        anchors.fill: parent
                        visible: rowDrop.containsDrag && rowDrop.allowed
                        color: Theme.accent
                        opacity: 0.35
                        border.color: Theme.accent
                    }
                    DropArea {
                        id: rowDrop
                        property bool allowed: false
                        anchors.fill: parent
                        keys: ["keyword"]
                        onEntered: drag => allowed = drag.source.keywordId !== row.keywordId
                                                  && panel.keywords.canMove(drag.source.keywordId, row.keywordId)
                        onDropped: drop => panel.dropOn(drop.source.keywordId, row.keywordId)
                    }

                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: row.depth * 14
                        spacing: 2
                        Label {
                            Layout.preferredWidth: 16
                            horizontalAlignment: Text.AlignHCenter
                            text: row.hasChildren ? (row.expanded ? "▾" : "▸") : ""
                            color: Theme.quiet
                            MouseArea {
                                anchors.fill: parent
                                enabled: row.hasChildren
                                onClicked: panel.keywords.toggleExpanded(row.index)
                            }
                        }
                        AppCheckBox {
                            id: check
                            tristate: true
                            padding: 0
                            focusPolicy: Qt.NoFocus
                            checkState: row.carried === 2 ? Qt.Checked : row.carried === 1 ? Qt.PartiallyChecked : Qt.Unchecked
                            enabled: panel.photoGrid.selectedCount > 0
                            Accessible.name: row.name
                            // The state comes from the selection, not from the click: the click asks for it.
                            nextCheckState: function () { return checkState }
                            onClicked: panel.assign(row.keywordId, row.carried !== 2)
                        }
                        Label {
                            Layout.fillWidth: true
                            text: row.name
                            elide: Text.ElideRight
                            MouseArea {
                                id: nameArea
                                anchors.fill: parent
                                acceptedButtons: Qt.LeftButton | Qt.RightButton
                                drag.target: ghost
                                drag.threshold: 8
                                onPressed: mouse => {
                                    if (mouse.button !== Qt.LeftButton)
                                        return
                                    const at = mapToItem(panel, mouse.x, mouse.y)
                                    ghost.x = at.x - ghost.Drag.hotSpot.x
                                    ghost.y = at.y - ghost.Drag.hotSpot.y
                                    ghost.keywordId = row.keywordId
                                    ghost.label = row.name
                                }
                                drag.onActiveChanged: {
                                    if (drag.active) {
                                        ghost.visible = true
                                        ghost.Drag.active = true
                                        panel.dragging = true
                                    } else {
                                        // (Not here: the drop can move the keyword and the list then rebuilds its rows,
                                        // this one included.)
                                        panel.endDrag()
                                    }
                                }
                                onClicked: mouse => {
                                    panel.createUnder = row.keywordId
                                    panel.createUnderName = row.name
                                    if (mouse.button === Qt.RightButton) {
                                        menu.row = row.index
                                        menu.keywordId = row.keywordId
                                        menu.keywordName = row.name
                                        menu.popup()
                                    }
                                }
                            }
                        }
                        Label {
                            text: row.photos
                            color: Theme.quiet
                            Layout.rightMargin: 6
                        }
                    }
                }
            }
        }

        MetadataPanel {
            id: metadataPanel
            visible: tabs.currentIndex === 1
            Layout.fillWidth: true
            Layout.fillHeight: true
            photoGrid: panel.photoGrid
            library: panel.library
        }

        InfoPanel {
            id: infoPanel
            visible: tabs.currentIndex === 2
            Layout.fillWidth: true
            Layout.fillHeight: true
            photoGrid: panel.photoGrid
            library: panel.library
        }

        CollectionPanel {
            id: collectionPanel
            visible: tabs.currentIndex === 3
            Layout.fillWidth: true
            Layout.fillHeight: true
            collections: panel.collections
            photoGrid: panel.photoGrid
            library: panel.library
        }
    }

    // Why something was refused: over the bottom of the list, so that nothing moves to make room for it.
    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 6
        visible: panel.expanded && panel.note !== ""
        height: noteLabel.implicitHeight + 12
        radius: Theme.radiusControl
        color: palette.window
        border.color: Theme.danger
        Label {
            id: noteLabel
            anchors.fill: parent
            anchors.margins: 6
            text: panel.note
            color: Theme.danger
            wrapMode: Text.Wrap
        }
    }

    // What follows the pointer while a keyword is dragged (it lives here, not in a row, which the list clips).
    Rectangle {
        id: ghost
        property string keywordId: ""
        property alias label: ghostLabel.text
        visible: false
        z: 100
        width: 180
        height: 26
        radius: Theme.radiusControl
        color: palette.highlight
        opacity: 0.85
        Drag.keys: ["keyword"]
        Drag.source: ghost
        Drag.hotSpot.x: 12
        Drag.hotSpot.y: height / 2
        Label {
            id: ghostLabel
            anchors.fill: parent
            anchors.leftMargin: 8
            verticalAlignment: Text.AlignVCenter
            color: palette.highlightedText
            elide: Text.ElideRight
        }
    }

    // Collapsed: a strip with the button that brings the panel back.
    AppToolButton {
        anchors.top: parent.top
        anchors.horizontalCenter: parent.horizontalCenter
        visible: !panel.expanded
        text: "«"
        focusPolicy: Qt.NoFocus
        Accessible.name: qsTr("Show the keyword panel")
        onClicked: panel.expanded = true
    }

    AppSubMenu {
        id: menu
        property int row: -1
        property string keywordId: ""
        property string keywordName: ""
        AppMenuItem {
            text: qsTr("Show the photos with this keyword")
            onTriggered: panel.library.filterKeyword(menu.keywordId, menu.keywordName)
        }
        AppMenuItem {
            text: qsTr("Rename…")
            onTriggered: renameDialog.openFor(menu.row, menu.keywordName)
        }
        AppMenuItem {
            text: qsTr("Properties…")
            onTriggered: propertiesDialog.openFor(menu.row, menu.keywordName)
        }
        MenuSeparator {}
        AppMenuItem {
            text: qsTr("Move to…")
            onTriggered: moveDialog.openFor(menu.keywordId, menu.keywordName)
        }
        AppMenuItem {
            text: qsTr("Move to the top level")
            enabled: panel.keywords.canMove(menu.keywordId, "")
            onTriggered: panel.dropOn(menu.keywordId, "")
        }
        MenuSeparator {}
        AppMenuItem {
            text: qsTr("Delete…")
            onTriggered: deleteDialog.openFor(menu.keywordId)
        }
    }

    AppDialog {
        id: renameDialog
        property int row: -1
        property string error: ""
        property alias nameField: nameField
        preferredWidth: 420
        title: qsTr("Rename the keyword")

        function openFor(keywordRow, name) {
            row = keywordRow
            nameField.text = name
            error = ""
            open()
            nameField.forceActiveFocus()
            nameField.selectAll()
        }

        function tryRename() {
            const reason = panel.keywords.rename(row, nameField.text)
            if (reason === "")
                close()
            else
                error = panel.explain(reason)
        }

        contentItem: ColumnLayout {
            spacing: 8
            AppTextField {
                id: nameField
                Layout.fillWidth: true
                Accessible.name: qsTr("Name")
                onAccepted: renameDialog.tryRename()
            }
            Label {
                Layout.fillWidth: true
                visible: renameDialog.error !== ""
                text: renameDialog.error
                color: Theme.danger
                wrapMode: Text.Wrap
            }
        }

        footer: AppDialogButtonBox {
            AppButton {
                text: qsTr("Rename")
                highlighted: true
                DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
                onClicked: renameDialog.tryRename()
            }
            AppButton {
                text: qsTr("Cancel")
                DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
                onClicked: renameDialog.close()
            }
        }
    }

    AppDialog {
        id: moveDialog
        property string keywordId: ""
        property string keywordName: ""
        property var targets: []
        property alias targetBox: targetBox
        preferredWidth: 460
        title: qsTr("Move the keyword")

        function openFor(id, name) {
            keywordId = id
            keywordName = name
            targets = JSON.parse(panel.keywords.moveTargets(id))
            targetBox.currentIndex = targets.length > 0 ? 0 : -1
            open()
        }

        function tryMove() {
            if (targetBox.currentIndex < 0)
                return
            panel.note = panel.explain(panel.keywords.moveKeyword(keywordId, targets[targetBox.currentIndex].id))
            close()
        }

        contentItem: ColumnLayout {
            spacing: 8
            Label {
                Layout.fillWidth: true
                text: moveDialog.targets.length > 0 ? qsTr("Move “%1” under:").arg(moveDialog.keywordName)
                                                    : qsTr("There is nowhere to move “%1”.").arg(moveDialog.keywordName)
                wrapMode: Text.Wrap
            }
            AppComboBox {
                id: targetBox
                Layout.fillWidth: true
                model: moveDialog.targets
                textRole: "path"
                enabled: moveDialog.targets.length > 0
                Accessible.name: qsTr("New parent")
            }
        }

        footer: AppDialogButtonBox {
            AppButton {
                text: qsTr("Move")
                highlighted: true
                enabled: targetBox.currentIndex >= 0
                DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
                onClicked: moveDialog.tryMove()
            }
            AppButton {
                text: qsTr("Cancel")
                DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
                onClicked: moveDialog.close()
            }
        }
    }

    // Deleting takes the keyword and its branch off every photo: the numbers are said, and it can be
    // undone. Past BACKGROUND_THRESHOLD photos it runs as a background sweep instead (D-126 volet B):
    // the dialog stays open with a progress bar and a cancel button until it actually ends.
    AppDialog {
        id: deleteDialog
        property string keywordId: ""
        property string keywordName: ""
        property int branchKeywords: 1
        property int branchPhotos: 0
        property string job: ""
        property real progress: 0
        property string status: ""
        preferredWidth: 480
        title: qsTr("Delete the keyword")

        function openFor(id) {
            const info = JSON.parse(panel.keywords.branch(id))
            keywordId = id
            keywordName = info.name
            branchKeywords = info.keywords
            branchPhotos = info.photos
            job = ""
            status = ""
            open()
        }

        function confirm() {
            const result = panel.keywords.remove(keywordId)
            if (result.indexOf("job:") === 0) {
                job = result.substring(4)
                status = qsTr("Removing it from %1 photo(s)…").arg(branchPhotos)
            } else {
                panel.note = panel.explain(result)
                close()
            }
        }

        onClosed: job = ""

        Connections {
            target: Bus
            function onJobProgress(job, done, total) {
                if (job !== deleteDialog.job || total <= 0)
                    return
                deleteDialog.progress = done / total
                deleteDialog.status = qsTr("Removing it from %1 of %2 photo(s)…").arg(done).arg(total)
            }
            function onKeywordDeleted(job, keywordId, keywords, photos, finished) {
                if (job !== deleteDialog.job)
                    return
                deleteDialog.job = ""
                panel.keywords.refresh()
                if (finished) {
                    deleteDialog.close()
                } else {
                    deleteDialog.status = qsTr("Stopped: %1 photo(s) done. Delete again to finish.").arg(photos)
                }
            }
        }

        contentItem: ColumnLayout {
            spacing: 8
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                visible: deleteDialog.job === ""
                text: deleteDialog.branchKeywords > 1
                      ? qsTr("Delete “%1” and the %n keyword(s) under it?", "", deleteDialog.branchKeywords - 1).arg(deleteDialog.keywordName)
                      : qsTr("Delete “%1”?").arg(deleteDialog.keywordName)
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                visible: deleteDialog.job === ""
                text: deleteDialog.branchPhotos > 0
                      ? qsTr("%n photo(s) will lose it. You can undo this.", "", deleteDialog.branchPhotos)
                      : qsTr("No photo has it. You can undo this.")
                color: Theme.quiet
            }
            AppProgressBar {
                Layout.fillWidth: true
                visible: deleteDialog.job !== ""
                value: deleteDialog.progress
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                visible: deleteDialog.status !== ""
                color: Theme.quiet
                text: deleteDialog.status
            }
        }

        footer: AppDialogButtonBox {
            AppButton {
                text: qsTr("Delete")
                highlighted: true
                visible: deleteDialog.job === ""
                DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
                onClicked: deleteDialog.confirm()
            }
            AppButton {
                text: qsTr("Cancel sweep")
                visible: deleteDialog.job !== ""
                DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
                onClicked: panel.keywords.cancelDelete()
            }
            AppButton {
                // Before confirming, this declines; once a sweep is running, closing does not cancel it
                // (it keeps going in the background) — "Cancel sweep" is the button that does.
                text: deleteDialog.job === "" ? qsTr("Cancel") : qsTr("Close")
                DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
                onClicked: deleteDialog.close()
            }
        }
    }

    // Synonyms and the export flag (spec §5.7, D-045, WP10 slice 2): neither has a UI of its own
    // yet, both are set here, together, as one step. Nothing reads the export flag yet (WP5.8).
    AppDialog {
        id: propertiesDialog
        property int row: -1
        property string keywordName: ""
        property string error: ""
        property alias synonymsField: synonymsField
        property alias doNotExportField: doNotExportField
        preferredWidth: 420
        title: qsTr("Keyword properties")

        function openFor(keywordRow, name) {
            row = keywordRow
            keywordName = name
            const info = JSON.parse(panel.keywords.properties(row))
            synonymsField.text = info.synonyms
            doNotExportField.checked = !info.export
            error = ""
            open()
            synonymsField.forceActiveFocus()
        }

        function trySave() {
            const reason = panel.keywords.setProperties(row, synonymsField.text, !doNotExportField.checked)
            if (reason === "")
                close()
            else
                error = panel.explain(reason)
        }

        contentItem: ColumnLayout {
            spacing: 8
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: qsTr("“%1”").arg(propertiesDialog.keywordName)
                color: Theme.quiet
            }
            Label {
                text: qsTr("Synonyms, one a line:")
            }
            AppTextArea {
                id: synonymsField
                Layout.fillWidth: true
                Layout.preferredHeight: 84
                wrapMode: TextArea.Wrap
                Accessible.name: qsTr("Synonyms")
            }
            AppCheckBox {
                id: doNotExportField
                text: qsTr("Do not export")
            }
            Label {
                Layout.fillWidth: true
                visible: propertiesDialog.error !== ""
                text: propertiesDialog.error
                color: Theme.danger
                wrapMode: Text.Wrap
            }
        }

        footer: AppDialogButtonBox {
            AppButton {
                text: qsTr("Save")
                highlighted: true
                DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
                onClicked: propertiesDialog.trySave()
            }
            AppButton {
                text: qsTr("Cancel")
                DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
                onClicked: propertiesDialog.close()
            }
        }
    }
}
