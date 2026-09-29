// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// The metadata panel (spec §5.7; WP10, slice 1), the keyword panel's own second tab: a photo's title,
// caption and the rest of its plain-text metadata, on one photo or a whole selection. A field where the
// selection disagrees shows "Multiple values" instead of a value; leaving the field (Tab, a click
// elsewhere, or Enter for the single-line ones) applies what was typed to the whole selection as one
// action (`PhotoGrid.setMetadataSelection`, the same shape `labelSelection` already has). Persons shown
// takes one name a line; Creator is a single line (Patrick's own call: almost every photo has exactly
// one, and `MetadataField::Creator`'s own line-splitting get/set still holds underneath, a list of one).
// GPS and place names are a later slice's; custom fields have no UI here yet (the engine and the format
// already carry one end to end, WP10's own custom-field support).
//
// Copy and paste (WP10 item 1): Copy always reads the grid's active photo, its cursor
// (`library.grid.currentIndex`), never the selection -- `InfoPanel.qml`'s own precedent for "one
// specific photo, not the selection" -- into an in-memory `clipboard`, lost when the workspace changes or
// the app quits (no lasting link, D-039's own framing for the analogous Develop-settings feature). Paste
// applies to the current selection, like every other batch edit, through a dialog listing every copied
// field with a checkbox (all checked by default): a checked, blank field really does clear the target,
// not just get skipped.
Item {
    id: panel
    required property var photoGrid
    required property var library

    // The last Copy's own field values, by key; empty until the first Copy. Cleared to `{}` rather than
    // updated in place so `Object.keys(clipboard).length` (Paste's own `enabled`) reacts to it.
    property var clipboard: ({})
    // A paste past `BACKGROUND_THRESHOLD` items runs as a background job (D-127): unlike a single
    // field's own `pendingJob`, one for the whole panel, since a paste touches every row at once.
    property string pendingPasteJob: ""

    function copy() {
        const row = library.grid.currentIndex
        if (row < 0)
            return
        const values = {}
        for (const f of panel.fields)
            values[f.key] = photoGrid.metadataAt(row, f.key)
        panel.clipboard = values
    }

    Connections {
        target: Bus
        function onJobFinished(job) {
            if (job === panel.pendingPasteJob) {
                panel.pendingPasteJob = ""
                panel.refresh()
            }
        }
        function onJobCancelled(job) {
            if (job === panel.pendingPasteJob) {
                panel.pendingPasteJob = ""
                panel.refresh()
            }
        }
    }

    // Every field a row shows, in order: its key (`MetadataField::key`) and its label; `multiline` for
    // the fields whose text can run to more than a line (Creator is not one of them: see above).
    readonly property var fields: [
        { key: "title", label: qsTr("Title"), multiline: false },
        { key: "caption", label: qsTr("Caption"), multiline: true },
        { key: "creator", label: qsTr("Creator"), multiline: false },
        { key: "rights", label: qsTr("Copyright"), multiline: false },
        { key: "usage-terms", label: qsTr("Usage terms"), multiline: false },
        { key: "web-statement", label: qsTr("Web statement of rights"), multiline: false },
        { key: "credit", label: qsTr("Credit"), multiline: false },
        { key: "source", label: qsTr("Source"), multiline: false },
        { key: "headline", label: qsTr("Headline"), multiline: false },
        { key: "instructions", label: qsTr("Instructions"), multiline: false },
        { key: "sublocation", label: qsTr("Sublocation"), multiline: false },
        { key: "city", label: qsTr("City"), multiline: false },
        { key: "region", label: qsTr("Region"), multiline: false },
        { key: "country", label: qsTr("Country"), multiline: false },
        { key: "country-code", label: qsTr("Country code"), multiline: false },
        { key: "persons", label: qsTr("Persons shown"), multiline: true },
        { key: "event", label: qsTr("Event"), multiline: false },
    ]

    property alias list: list
    property alias copyButton: copyButton
    property alias pasteButton: pasteButton
    property alias pasteDialog: pasteDialog

    // Reloads every row from the selection's own current values: called once a selection has settled
    // (the same debounced timer the keyword panel's own usage already uses) and after undo or redo.
    function refresh() {
        for (let i = 0; i < list.count; i++) {
            const row = list.itemAtIndex(i)
            if (row)
                row.load()
        }
    }

    RowLayout {
        id: header
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.margins: 6
        spacing: 6
        AppButton {
            id: copyButton
            text: qsTr("Copy")
            enabled: panel.library.grid.currentIndex >= 0
            onClicked: panel.copy()
        }
        AppButton {
            id: pasteButton
            text: qsTr("Paste…")
            enabled: Object.keys(panel.clipboard).length > 0 && panel.photoGrid.selectedCount > 0
            onClicked: pasteDialog.openFor()
        }
        Item { Layout.fillWidth: true }
    }

    // Lists every copied field with its value and a checkbox (all checked by default); a checked,
    // blank field really does clear the target, not just get skipped.
    AppDialog {
        id: pasteDialog
        preferredWidth: 480
        title: qsTr("Paste metadata")

        // Which of the clipboard's own keys are left checked; rebuilt (all checked) each time the
        // dialog opens.
        property var checkedFields: ({})
        property alias fieldRepeater: fieldRepeater

        // The checkbox row for `key` (its own order, the same filter `fieldRepeater`'s model uses),
        // scrolled into view first (the list can run longer than the dialog is tall) — for a test to
        // click.
        function rowFor(key) {
            const keys = panel.fields.filter((f) => f.key in panel.clipboard).map((f) => f.key)
            const index = keys.indexOf(key)
            if (index < 0)
                return null
            const item = pasteDialog.fieldRepeater.itemAt(index)
            if (item)
                pasteScroller.contentItem.contentY = Math.max(0, item.y - 10)
            return item
        }

        function openFor() {
            const all = {}
            for (const key in panel.clipboard)
                all[key] = true
            pasteDialog.checkedFields = all
            open()
        }

        function toggle(key, value) {
            const next = Object.assign({}, pasteDialog.checkedFields)
            next[key] = value
            pasteDialog.checkedFields = next
        }

        function confirm() {
            const chosen = {}
            for (const key in pasteDialog.checkedFields)
                if (pasteDialog.checkedFields[key])
                    chosen[key] = panel.clipboard[key]
            panel.photoGrid.pasteMetadataSelection(JSON.stringify(chosen))
            if (panel.photoGrid.batchJob === "")
                panel.refresh()
            else
                panel.pendingPasteJob = panel.photoGrid.batchJob
            close()
        }

        contentItem: ScrollView {
            id: pasteScroller
            clip: true
            contentWidth: availableWidth
            implicitHeight: Math.min(fieldColumn.implicitHeight, 360)

            ColumnLayout {
                id: fieldColumn
                width: pasteScroller.availableWidth
                spacing: 4

                Repeater {
                    id: fieldRepeater
                    model: panel.fields.filter((f) => f.key in panel.clipboard)

                    delegate: RowLayout {
                        id: fieldRow
                        required property var modelData
                        property alias checkBox: checkBox
                        Layout.fillWidth: true
                        spacing: 8

                        CheckBox {
                            id: checkBox
                            checked: pasteDialog.checkedFields[fieldRow.modelData.key] === true
                            onToggled: pasteDialog.toggle(fieldRow.modelData.key, checked)
                        }
                        Label {
                            Layout.preferredWidth: 140
                            text: fieldRow.modelData.label
                        }
                        Label {
                            Layout.fillWidth: true
                            elide: Text.ElideRight
                            color: Theme.quiet
                            text: panel.clipboard[fieldRow.modelData.key] === ""
                                  ? qsTr("(empty)") : panel.clipboard[fieldRow.modelData.key]
                        }
                    }
                }
            }
        }

        footer: AppDialogButtonBox {
            AppButton {
                text: qsTr("Paste")
                highlighted: true
                DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
                onClicked: pasteDialog.confirm()
            }
            AppButton {
                text: qsTr("Cancel")
                DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
                onClicked: pasteDialog.close()
            }
        }
    }

    AppListFrame {
        anchors.top: header.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.topMargin: 6

        ListView {
            id: list
            anchors.fill: parent
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            spacing: 6
            model: panel.fields
            ScrollBar.vertical: AppScrollBar { id: vbar }

            delegate: ColumnLayout {
                id: row
                required property var modelData
                required property int index
                width: ListView.view.width - vbar.width
                spacing: 2

                property bool mixed: false
                property alias singleLine: singleLine
                property alias multiLine: multiLine
                // A large selection's edit runs as a background job instead of landing at once (D-126
                // volet B): the job that `apply()` started, while it is still applying, so `load()` is
                // deferred to when it actually ends rather than reading back a stale, pre-edit value.
                property string pendingJob: ""

                // What the selection agrees on, or "" while it does not (the field then shows the
                // "Multiple values" placeholder instead).
                function load() {
                    const answer = JSON.parse(panel.photoGrid.metadataOf(row.modelData.key))
                    row.mixed = answer.mixed
                    const text = answer.mixed ? "" : answer.value
                    singleLine.text = text
                    multiLine.text = text
                }

                // Applies `value` to the whole selection, then reloads once it has actually landed (a value
                // the engine normalises, such as a list field's blank lines dropped, is shown as it was
                // actually kept) — at once for a selection small enough to apply synchronously, or once the
                // background job `setMetadataSelection` started for a larger one reports its end.
                function apply(value) {
                    panel.photoGrid.setMetadataSelection(row.modelData.key, value)
                    const job = panel.photoGrid.batchJob
                    if (job === "")
                        row.load()
                    else
                        row.pendingJob = job
                }

                Connections {
                    target: Bus
                    function onJobFinished(job) {
                        if (job === row.pendingJob) {
                            row.pendingJob = ""
                            row.load()
                        }
                    }
                    function onJobCancelled(job) {
                        if (job === row.pendingJob) {
                            row.pendingJob = ""
                            row.load()
                        }
                    }
                }

                Component.onCompleted: row.load()

                Label {
                    text: row.modelData.label
                    color: Theme.quiet
                }
                TextField {
                    id: singleLine
                    visible: !row.modelData.multiline
                    Layout.fillWidth: true
                    placeholderText: row.mixed ? qsTr("Multiple values") : ""
                    onActiveFocusChanged: if (!activeFocus) row.apply(text)
                    Keys.onReturnPressed: row.apply(text)
                    Keys.onEnterPressed: row.apply(text)
                }
                TextArea {
                    id: multiLine
                    visible: row.modelData.multiline
                    Layout.fillWidth: true
                    Layout.preferredHeight: 52
                    wrapMode: TextArea.Wrap
                    placeholderText: row.mixed ? qsTr("Multiple values") : ""
                    onActiveFocusChanged: if (!activeFocus) row.apply(text)
                    // Unlike TextField, Fusion gives TextArea no background of its own: without one it read
                    // as bare text loose on the panel, not as a field (Patrick's own review caught this).
                    background: Rectangle {
                        color: palette.base
                        radius: Theme.radiusControl
                        border.width: 1
                        border.color: multiLine.activeFocus ? palette.highlight : palette.mid
                    }
                }
            }
        }
    }
}
