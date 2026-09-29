// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// The collections tab (WP10, slice 3): the manual collections as a tree (a collection can hold photos and
// have collections inside it), each with a check that says whether the selected photos are in it none, some
// or all, and the number of photos it holds itself. A click on the check puts the whole selection in it, or
// takes it out; the field above types ahead (it filters the tree), Enter puts the selection in the best
// match, and makes a collection, with the selection in it, when nothing matches (Shift+Enter makes one even
// when something does). Every one of these is one step of the history, and so are renaming, moving and
// deleting a collection with what is inside it. A collection is moved by dragging it onto another (or onto
// any place of the tab that has no collection, for the top level), or from its menu. Its shape is the
// keyword panel's (`KeywordPanel.qml`), deliberately parallel rather than shared: the models behind the two
// cannot be one type, and this file leaves the keyword panel's shipped behaviour alone.
Item {
    id: panel
    required property var collections
    required property var photoGrid
    required property var library

    // New collections go inside this one (an identifier), if a collection was clicked.
    property string createUnder: ""
    property string createUnderName: ""
    // A collection is being dragged (the top-level strip shows).
    property bool dragging: false
    // Why something was refused, in words.
    property string note: ""

    property alias filterField: field
    property alias tree: tree
    property alias contextMenu: menu
    property alias addButton: addButton
    property alias renameDialog: renameDialog
    property alias moveDialog: moveDialog
    property alias deleteDialog: deleteDialog
    property alias topLevelDrop: topLevelDrop
    property alias ghost: ghost
    // A dialog of the tab is open (the window's commands wait, as for every dialog).
    readonly property bool dialogOpen: renameDialog.visible || moveDialog.visible || deleteDialog.visible

    // Puts the selection in a collection, or takes it out; false when nothing is selected.
    function assign(id, add) {
        if (photoGrid.selectedCount === 0)
            return false
        photoGrid.collectionSelection(id, add)
        return true
    }

    // Enter in the field: the selection goes in the best match; when nothing matches (or with `forceCreate`),
    // what was typed becomes a collection, inside the one that was clicked if any, with the selection in it:
    // making it and filling it is one step of the history.
    function commit(forceCreate) {
        const typed = field.text.trim()
        if (typed === "")
            return
        let id = ""
        if (!forceCreate) {
            const row = collections.bestMatch(typed)
            if (row >= 0)
                id = collections.idAt(row)
        }
        if (id === "")
            id = collections.findSibling(typed, createUnder)
        if (id !== "") {
            note = ""
            assign(id, true)
        } else {
            const made = photoGrid.createCollectionSelection(typed, createUnder)
            if (made.indexOf("error:") === 0) {
                note = explain(made.substring(6))
                return
            }
            note = ""
            collections.refresh()
        }
        field.text = ""
    }

    // A refusal, in words: the models answer with a code (`name`, `taken:<name>`, `cycle`, or `other:` and the
    // engine's own English), the sentence is the interface's, so that it is translated. Empty for no refusal.
    function explain(code) {
        if (code === "")
            return ""
        if (code === "name")
            return qsTr("A collection needs a name.")
        if (code === "cycle")
            return qsTr("A collection cannot be moved under itself or under one of its own collections.")
        if (code.indexOf("taken:") === 0)
            return qsTr("There is already a collection named “%1” there.").arg(code.substring(6))
        return code.indexOf("other:") === 0 ? code.substring(6) : code
    }

    // What was typed can be added as a collection where new ones go (nothing of that name is there yet). Enter
    // gives the best match instead, which may be a collection of the same name elsewhere in the tree: this is
    // how to make a second one (Shift+Enter does the same from the keyboard).
    readonly property string typed: field.text.trim()
    readonly property bool canAdd: (collections.count, typed !== "" && collections.findSibling(typed, createUnder) === "")

    // The pointer was released: the ghost drops (on the row or the strip under it) and goes.
    function endDrag() {
        ghost.Drag.drop()
        ghost.Drag.active = false
        ghost.visible = false
        dragging = false
    }

    // A collection was dropped on `parent` (an identifier; empty for the top level).
    function dropOn(id, parent) {
        if (id === parent)
            return
        note = explain(collections.moveCollection(id, parent))
    }

    // Anywhere in the tab where there is no collection is a place to drop one to make it a top-level collection
    // (the rows, in front of it, take the drops that are theirs).
    DropArea {
        id: topLevelDrop
        anchors.fill: parent
        keys: ["collection"]
        property bool allowed: false
        onEntered: drag => allowed = panel.collections.canMove(drag.source.collectionId, "")
        onDropped: drop => panel.dropOn(drop.source.collectionId, "")
    }
    Rectangle {
        anchors.fill: parent
        visible: topLevelDrop.containsDrag && topLevelDrop.allowed
        color: palette.highlight
        opacity: 0.18
        border.color: palette.highlight
        border.width: 2
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 6

        TextField {
            id: field
            Layout.fillWidth: true
            placeholderText: qsTr("Find or add a collection…")
            Accessible.name: qsTr("Find or add a collection")
            onTextChanged: {
                panel.collections.setFilter(text)
                panel.note = ""
            }
            Keys.onReturnPressed: event => panel.commit((event.modifiers & Qt.ShiftModifier) !== 0)
            Keys.onEnterPressed: event => panel.commit((event.modifiers & Qt.ShiftModifier) !== 0)
            Keys.onEscapePressed: {
                text = ""
                panel.library.grid.forceActiveFocus()
            }
        }

        // One line that always has its place, so that the list below does not move: the button that adds what was
        // typed, else where new collections go (once a collection was clicked).
        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: 30
            Layout.maximumHeight: 30
            AppButton {
                id: addButton
                visible: panel.canAdd
                Layout.fillWidth: true
                focusPolicy: Qt.NoFocus
                text: panel.createUnder === "" ? qsTr("Add “%1” at the top level").arg(panel.typed)
                                               : qsTr("Add “%1” inside %2").arg(panel.typed).arg(panel.createUnderName)
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
                text: qsTr("New collections go inside %1").arg(panel.createUnderName)
                color: Theme.quiet
                elide: Text.ElideRight
            }
            ToolButton {
                visible: !panel.canAdd && panel.createUnder !== ""
                text: "×"
                focusPolicy: Qt.NoFocus
                Accessible.name: qsTr("New collections go at the top level")
                onClicked: panel.createUnder = ""
            }
            Item { Layout.fillWidth: !panel.canAdd && panel.createUnder === "" }
        }

        ListView {
            id: tree
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: panel.collections
            ScrollBar.vertical: ScrollBar {}

            delegate: Item {
                id: row
                required property int index
                required property string collectionId
                required property string name
                required property int depth
                required property int photos
                required property int held
                required property bool hasChildren
                required property bool expanded
                width: ListView.view.width
                height: 28

                // Dropping a collection here puts it inside this one (when that is possible).
                Rectangle {
                    anchors.fill: parent
                    visible: rowDrop.containsDrag && rowDrop.allowed
                    color: palette.highlight
                    opacity: 0.35
                    border.color: palette.highlight
                }
                DropArea {
                    id: rowDrop
                    property bool allowed: false
                    anchors.fill: parent
                    keys: ["collection"]
                    onEntered: drag => allowed = drag.source.collectionId !== row.collectionId
                                              && panel.collections.canMove(drag.source.collectionId, row.collectionId)
                    onDropped: drop => panel.dropOn(drop.source.collectionId, row.collectionId)
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
                            onClicked: panel.collections.toggleExpanded(row.index)
                        }
                    }
                    CheckBox {
                        id: check
                        tristate: true
                        padding: 0
                        focusPolicy: Qt.NoFocus
                        checkState: row.held === 2 ? Qt.Checked : row.held === 1 ? Qt.PartiallyChecked : Qt.Unchecked
                        enabled: panel.photoGrid.selectedCount > 0
                        Accessible.name: row.name
                        // The state comes from the selection, not from the click: the click asks for it.
                        nextCheckState: function () { return checkState }
                        onClicked: panel.assign(row.collectionId, row.held !== 2)
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
                                ghost.collectionId = row.collectionId
                                ghost.label = row.name
                            }
                            drag.onActiveChanged: {
                                if (drag.active) {
                                    ghost.visible = true
                                    ghost.Drag.active = true
                                    panel.dragging = true
                                } else {
                                    // (Not here: the drop can move the collection and the list then rebuilds its
                                    // rows, this one included.)
                                    panel.endDrag()
                                }
                            }
                            onClicked: mouse => {
                                panel.createUnder = row.collectionId
                                panel.createUnderName = row.name
                                if (mouse.button === Qt.RightButton) {
                                    menu.row = row.index
                                    menu.collectionId = row.collectionId
                                    menu.collectionName = row.name
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

    // Why something was refused: over the bottom of the list, so that nothing moves to make room for it.
    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        visible: panel.note !== ""
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

    // What follows the pointer while a collection is dragged (it lives here, not in a row, which the list clips).
    Rectangle {
        id: ghost
        property string collectionId: ""
        property alias label: ghostLabel.text
        visible: false
        z: 100
        width: 180
        height: 26
        radius: Theme.radiusControl
        color: palette.highlight
        opacity: 0.85
        Drag.keys: ["collection"]
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

    AppSubMenu {
        id: menu
        property int row: -1
        property string collectionId: ""
        property string collectionName: ""
        AppMenuItem {
            text: qsTr("Show the photos in this collection")
            onTriggered: panel.library.filterCollection(menu.collectionId, menu.collectionName)
        }
        AppMenuItem {
            text: qsTr("Rename…")
            onTriggered: renameDialog.openFor(menu.row, menu.collectionName)
        }
        MenuSeparator {}
        AppMenuItem {
            text: qsTr("Move to…")
            onTriggered: moveDialog.openFor(menu.collectionId, menu.collectionName)
        }
        AppMenuItem {
            text: qsTr("Move to the top level")
            enabled: panel.collections.canMove(menu.collectionId, "")
            onTriggered: panel.dropOn(menu.collectionId, "")
        }
        MenuSeparator {}
        AppMenuItem {
            text: qsTr("Delete…")
            onTriggered: deleteDialog.openFor(menu.collectionId)
        }
    }

    AppDialog {
        id: renameDialog
        property int row: -1
        property string error: ""
        property alias nameField: nameField
        preferredWidth: 420
        title: qsTr("Rename the collection")

        function openFor(collectionRow, name) {
            row = collectionRow
            nameField.text = name
            error = ""
            open()
            nameField.forceActiveFocus()
            nameField.selectAll()
        }

        function tryRename() {
            const reason = panel.collections.rename(row, nameField.text)
            if (reason === "")
                close()
            else
                error = panel.explain(reason)
        }

        contentItem: ColumnLayout {
            spacing: 8
            TextField {
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
        property string collectionId: ""
        property string collectionName: ""
        property var targets: []
        property alias targetBox: targetBox
        preferredWidth: 460
        title: qsTr("Move the collection")

        function openFor(id, name) {
            collectionId = id
            collectionName = name
            targets = JSON.parse(panel.collections.moveTargets(id))
            targetBox.currentIndex = targets.length > 0 ? 0 : -1
            open()
        }

        function tryMove() {
            if (targetBox.currentIndex < 0)
                return
            panel.note = panel.explain(panel.collections.moveCollection(collectionId, targets[targetBox.currentIndex].id))
            close()
        }

        contentItem: ColumnLayout {
            spacing: 8
            Label {
                Layout.fillWidth: true
                text: moveDialog.targets.length > 0 ? qsTr("Move “%1” inside:").arg(moveDialog.collectionName)
                                                    : qsTr("There is nowhere to move “%1”.").arg(moveDialog.collectionName)
                wrapMode: Text.Wrap
            }
            ComboBox {
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

    // Deleting takes the collection and the ones inside it away, not the photos: the numbers are said, and it
    // can be undone.
    AppDialog {
        id: deleteDialog
        property string collectionId: ""
        property string collectionName: ""
        property int branchCollections: 1
        property int branchPhotos: 0
        preferredWidth: 480
        title: qsTr("Delete the collection")

        function openFor(id) {
            const info = JSON.parse(panel.collections.branch(id))
            collectionId = id
            collectionName = info.name
            branchCollections = info.collections
            branchPhotos = info.photos
            open()
        }

        function confirm() {
            panel.note = panel.explain(panel.collections.remove(collectionId))
            close()
        }

        contentItem: ColumnLayout {
            spacing: 8
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: deleteDialog.branchCollections > 1
                      ? qsTr("Delete “%1” and the %n collection(s) inside it?", "", deleteDialog.branchCollections - 1).arg(deleteDialog.collectionName)
                      : qsTr("Delete “%1”?").arg(deleteDialog.collectionName)
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: deleteDialog.branchPhotos > 0
                      ? qsTr("The %n photo(s) in it stay in your catalogue. You can undo this.", "", deleteDialog.branchPhotos)
                      : qsTr("No photo is in it. You can undo this.")
                color: Theme.quiet
            }
        }

        footer: AppDialogButtonBox {
            AppButton {
                text: qsTr("Delete")
                highlighted: true
                DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
                onClicked: deleteDialog.confirm()
            }
            AppButton {
                text: qsTr("Cancel")
                DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
                onClicked: deleteDialog.close()
            }
        }
    }
}
