// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// The changes other applications made to the XMP files beside originals, waiting for an answer (spec §5.7,
// D-047; WP10). Read from what a scan stored, so it works with the source offline; nothing here writes a file.
// Each photo lists the fields the file changed (yours → the file's), the keywords it gained and lost, and, for a
// field changed on both sides, a choice between your value (the default) and the file's. Accept applies the
// changes as one undoable step; Ignore remembers the file as it stands and does not report it again until it
// changes again. Accept all is the one click of the spec.
//
// A real secondary window (D-113), like the duplicates report: the list can get long.
AppWindow {
    id: dialog
    required property var external
    property alias list: list
    property alias acceptAllButton: acceptAllButton
    property alias ignoreAllButton: ignoreAllButton
    property alias closeButton: closeButton
    property alias introLabel: introLabel
    property alias answersLabel: answersLabel
    property alias safetyLabel: safetyLabel
    // The three buttons of the footer are as wide as the widest label asks, and never narrower than 128px: a row that
    // fills its window squeezes its buttons down to nothing when the window is small, and a label ("Tout accepter")
    // would be cut.
    readonly property real footerButtonWidth: Math.max(128, acceptAllButton.implicitWidth, ignoreAllButton.implicitWidth,
                                                       closeButton.implicitWidth)

    title: qsTr("Metadata changed by another application")

    property var entries: []
    // The conflicts to settle in the file's favour, "<photo>|<field>": true. Absent = keep the photo's value.
    property var useFile: ({})

    function refresh() { entries = JSON.parse(external.list()) }
    onVisibleChanged: if (dialog.visible) dialog.refresh()
    Connections {
        target: Bus
        function onExternalChanges() { if (dialog.visible) dialog.refresh() }
    }

    function choose(photo, field, file) {
        const next = Object.assign({}, dialog.useFile)
        if (file)
            next[photo + "|" + field] = true
        else
            delete next[photo + "|" + field]
        dialog.useFile = next
    }

    // The conflicts of `ids` settled in the file's favour, as the engine takes them.
    function choicesFor(ids) {
        const chosen = []
        for (const entry of dialog.entries)
            if (ids.indexOf(entry.id) >= 0)
                for (const change of entry.changes)
                    if (change.conflict && dialog.useFile[entry.id + "|" + change.field] === true)
                        chosen.push({ id: entry.id, field: change.field })
        return JSON.stringify(chosen)
    }

    function acceptOne(id) { external.accept(JSON.stringify([id]), choicesFor([id])) }
    function acceptAll() {
        const ids = dialog.entries.map(e => e.id)
        external.accept(JSON.stringify(ids), choicesFor(ids))
    }

    function fieldLabel(key) {
        switch (key) {
        case "rating": return qsTr("Rating")
        case "label": return qsTr("Colour label")
        case "title": return qsTr("Title")
        case "caption": return qsTr("Caption")
        case "creator": return qsTr("Creator")
        case "rights": return qsTr("Copyright")
        case "usage-terms": return qsTr("Usage terms")
        case "web-statement": return qsTr("Web statement of rights")
        case "credit": return qsTr("Credit")
        case "source": return qsTr("Source")
        case "headline": return qsTr("Headline")
        case "instructions": return qsTr("Instructions")
        case "sublocation": return qsTr("Sublocation")
        case "city": return qsTr("City")
        case "region": return qsTr("Region")
        case "country": return qsTr("Country")
        case "country-code": return qsTr("Country code")
        case "persons": return qsTr("Persons shown")
        case "event": return qsTr("Event")
        default: return key
        }
    }

    // A value as a person reads it: the rating as stars or "Rejected", nothing as "(empty)", a list on one line.
    function valueText(field, value) {
        if (value === "")
            return field === "rating" ? qsTr("no rating") : qsTr("(empty)")
        if (field === "rating")
            return value === "-1" ? qsTr("Rejected") : qsTr("%n star(s)", "", parseInt(value))
        return value.split("\n").join(", ")
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 8
        // What this is, what the two answers do, and what is safe: three short paragraphs rather than one long sentence.
        Label {
            id: introLabel
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: dialog.entries.length > 0
                  ? qsTr("Another application changed the XMP file beside the original of %n photo(s).", "", dialog.entries.length)
                  : qsTr("Nothing is waiting.")
        }
        Label {
            id: answersLabel
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            visible: dialog.entries.length > 0
            text: qsTr("Accept applies what the file says to the photo, in one step you can undo. Ignore keeps the photo as it is; the file is offered again only if it changes.")
        }
        Label {
            id: safetyLabel
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            visible: dialog.entries.length > 0
            color: Theme.quiet
            text: qsTr("Nothing is applied until you choose. Auroraw changes these files only when you export XMP files.")
        }
        AppListFrame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            AppListView {
                id: list
                anchors.fill: parent
                model: dialog.entries
                spacing: 6
                delegate: Rectangle {
                    id: entry
                    required property var modelData
                    required property int index
                    width: list.rowWidth
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
                            Layout.alignment: Qt.AlignTop
                            source: "image://thumbs/" + entry.modelData.id
                            fillMode: Image.PreserveAspectFit
                            asynchronous: true
                            Accessible.ignored: true
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 3
                            Label {
                                Layout.fillWidth: true
                                font.bold: true
                                elide: Text.ElideRight
                                text: entry.modelData.filename
                            }
                            Label {
                                Layout.fillWidth: true
                                elide: Text.ElideMiddle
                                color: Theme.quiet
                                text: entry.modelData.xmpPath
                            }
                            Repeater {
                                model: entry.modelData.changes
                                delegate: ColumnLayout {
                                    id: change
                                    required property var modelData
                                    Layout.fillWidth: true
                                    spacing: 0
                                    Label {
                                        Layout.fillWidth: true
                                        wrapMode: Text.Wrap
                                        visible: !change.modelData.conflict
                                        text: qsTr("%1: %2 → %3")
                                                  .arg(dialog.fieldLabel(change.modelData.field))
                                                  .arg(dialog.valueText(change.modelData.field, change.modelData.mine))
                                                  .arg(dialog.valueText(change.modelData.field, change.modelData.file))
                                    }
                                    Label {
                                        Layout.fillWidth: true
                                        wrapMode: Text.Wrap
                                        visible: change.modelData.conflict
                                        color: Theme.warning
                                        text: qsTr("%1: changed here and in the file").arg(dialog.fieldLabel(change.modelData.field))
                                    }
                                    AppRadioButton {
                                        visible: change.modelData.conflict
                                        text: qsTr("Keep mine: %1").arg(dialog.valueText(change.modelData.field, change.modelData.mine))
                                        checked: dialog.useFile[entry.modelData.id + "|" + change.modelData.field] !== true
                                        onClicked: dialog.choose(entry.modelData.id, change.modelData.field, false)
                                    }
                                    AppRadioButton {
                                        visible: change.modelData.conflict
                                        text: qsTr("Take the file's: %1").arg(dialog.valueText(change.modelData.field, change.modelData.file))
                                        checked: dialog.useFile[entry.modelData.id + "|" + change.modelData.field] === true
                                        onClicked: dialog.choose(entry.modelData.id, change.modelData.field, true)
                                    }
                                }
                            }
                            Repeater {
                                model: entry.modelData.keywordsAdded
                                delegate: Label {
                                    required property string modelData
                                    Layout.fillWidth: true
                                    elide: Text.ElideRight
                                    text: qsTr("Keyword added: %1").arg(modelData)
                                }
                            }
                            Repeater {
                                model: entry.modelData.keywordsRemoved
                                delegate: Label {
                                    required property string modelData
                                    Layout.fillWidth: true
                                    elide: Text.ElideRight
                                    text: qsTr("Keyword removed: %1").arg(modelData)
                                }
                            }
                        }
                        ColumnLayout {
                            Layout.alignment: Qt.AlignTop
                            spacing: 4
                            // Both as wide as the wider one.
                            AppButton {
                                objectName: "acceptOne"
                                Layout.fillWidth: true
                                text: qsTr("Accept")
                                onClicked: dialog.acceptOne(entry.modelData.id)
                            }
                            AppButton {
                                objectName: "ignoreOne"
                                Layout.fillWidth: true
                                text: qsTr("Ignore")
                                onClicked: dialog.external.ignore(JSON.stringify([entry.modelData.id]))
                            }
                        }
                    }
                }
            }
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Item { Layout.fillWidth: true }
            AppButton {
                id: acceptAllButton
                Layout.minimumWidth: dialog.footerButtonWidth
                Layout.preferredWidth: dialog.footerButtonWidth
                text: qsTr("Accept all")
                highlighted: true
                enabled: dialog.entries.length > 0
                onClicked: dialog.acceptAll()
            }
            AppButton {
                id: ignoreAllButton
                Layout.minimumWidth: dialog.footerButtonWidth
                Layout.preferredWidth: dialog.footerButtonWidth
                text: qsTr("Ignore all")
                enabled: dialog.entries.length > 0
                onClicked: dialog.external.ignoreAll()
            }
            AppButton {
                id: closeButton
                Layout.minimumWidth: dialog.footerButtonWidth
                Layout.preferredWidth: dialog.footerButtonWidth
                text: qsTr("Close")
                onClicked: dialog.close()
            }
        }
    }
}
