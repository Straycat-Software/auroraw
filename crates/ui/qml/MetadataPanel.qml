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
Item {
    id: panel
    required property var photoGrid

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

    // Reloads every row from the selection's own current values: called once a selection has settled
    // (the same debounced timer the keyword panel's own usage already uses) and after undo or redo.
    function refresh() {
        for (let i = 0; i < list.count; i++) {
            const row = list.itemAtIndex(i)
            if (row)
                row.load()
        }
    }

    AppListFrame {
        anchors.fill: parent

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

                // What the selection agrees on, or "" while it does not (the field then shows the
                // "Multiple values" placeholder instead).
                function load() {
                    const answer = JSON.parse(panel.photoGrid.metadataOf(row.modelData.key))
                    row.mixed = answer.mixed
                    const text = answer.mixed ? "" : answer.value
                    singleLine.text = text
                    multiLine.text = text
                }

                // Applies `value` to the whole selection, then reloads (a value the engine normalises,
                // such as a list field's blank lines dropped, is shown as it was actually kept).
                function apply(value) {
                    panel.photoGrid.setMetadataSelection(row.modelData.key, value)
                    row.load()
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
