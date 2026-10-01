// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// Finding the place names of photos (spec §5.7, design note 008, D-147): from where each photo was taken, offline, its
// city, region and country, written into the fields that are empty. On explicit request only: nothing is looked up until
// Find is pressed, and a field a person typed, or another application wrote, is never changed. Four steps in one dialog:
// "not installed" (the places file is not there: said, and nothing else is affected), the form, the progress, and what
// happened. The whole run is one undoable step (the Edit menu's Undo), whatever the number of photos.
//
// The body is as tall as the step asks, up to the window, and scrolls on a small screen (as the export dialog's does).
AppDialog {
    id: dialog
    required property var finder
    required property var sources
    required property var photoGrid
    // The interface's language (`en`, `fr`): the country and the region are named in it.
    required property string language
    preferredWidth: 560
    title: qsTr("Find place names")

    property alias scopeSelection: selectionScope
    property alias scopeSource: sourceScope
    property alias sourceBox: sourceBox
    property alias refreshBox: refreshBox
    property alias findButton: findButton
    property alias stopButton: stopButton
    property alias closeButton: closeButton
    property alias progressBar: progress
    property alias body: scroller

    // unavailable (no places file), form, running, done.
    property string phase: "form"
    property string job: ""
    property real share: 0
    property int done: 0
    property int total: 0
    property bool stopped: false
    property string failure: ""
    property var report: ({})
    property var sourceNames: []
    property var sourceIds: []
    readonly property int selectedCount: dialog.photoGrid.selectedCount
    readonly property bool canFind: (selectionScope.checked && dialog.selectedCount > 0)
                                    || (sourceScope.checked && dialog.sourceIds.length > 0)
    readonly property string refreshHint: qsTr("For photos whose position was corrected since. A name you edited or typed is left as it is.")

    closePolicy: dialog.phase === "running" ? Popup.NoAutoClose : Popup.CloseOnEscape

    // What a choice means, under it and in step with its text (a check box's own text does not wrap).
    component Hint: Label {
        Layout.fillWidth: true
        Layout.leftMargin: refreshBox.indicator.width + refreshBox.spacing
        wrapMode: Text.Wrap
        color: Theme.quiet
    }

    ButtonGroup { buttons: [selectionScope, sourceScope] }

    // (Not `reset`: a Dialog has a `reset` signal, for its Reset button, and Qt 6.8 warns of a function of that name.)
    function fillForm() {
        dialog.sources.refresh()
        const names = []
        const ids = []
        for (let row = 0; row < dialog.sources.count; row++) {
            names.push(dialog.sources.nameAt(row))
            ids.push(dialog.sources.idAt(row))
        }
        dialog.sourceNames = names
        dialog.sourceIds = ids
        refreshBox.checked = false
        selectionScope.checked = dialog.selectedCount > 0
        sourceScope.checked = dialog.selectedCount === 0
        sourceBox.currentIndex = names.length > 0 ? 0 : -1
        dialog.phase = dialog.finder.installed() ? "form" : "unavailable"
        dialog.job = ""
        dialog.share = 0
        dialog.done = 0
        dialog.total = 0
        dialog.stopped = false
        dialog.failure = ""
        dialog.report = ({})
    }

    onAboutToShow: dialog.fillForm()

    function start() {
        const selection = selectionScope.checked
        const target = selection ? dialog.photoGrid.selectedIds() : dialog.sourceIds[sourceBox.currentIndex]
        const started = dialog.finder.start(selection ? "selection" : "source", target, dialog.language, refreshBox.checked)
        if (started.indexOf("error:") === 0) {
            dialog.failure = started.substring(6)
            dialog.phase = "done"
            return
        }
        dialog.job = started
        dialog.share = 0
        dialog.done = 0
        dialog.total = 0
        dialog.phase = "running"
    }

    Connections {
        target: Bus
        function onJobProgress(job, done, total) {
            if (job === dialog.job && total > 0) {
                dialog.share = done / total
                dialog.done = done
                dialog.total = total
            }
        }
        function onPlaceNamesFound(job, report, cancelled) {
            if (job !== dialog.job)
                return
            dialog.report = JSON.parse(report)
            dialog.stopped = cancelled
            dialog.phase = "done"
        }
    }

    // What happened, one line for each thing that did. `attention` marks what went wrong: the others are good news, or
    // what the person asked for and the data says.
    readonly property var lines: {
        const r = dialog.report
        const out = []
        if (dialog.failure !== "")
            out.push({ text: qsTr("The place names could not be looked up: %1").arg(dialog.failure), attention: true })
        if (r.filled > 0)
            out.push({ text: qsTr("%n photo(s) got place names", "", r.filled), attention: false })
        if (r.hadPlace > 0)
            out.push({ text: qsTr("%n photo(s) already had their place, left as it is", "", r.hadPlace), attention: false })
        if (r.noPosition > 0)
            out.push({ text: qsTr("%n photo(s) have no position, so there is no place to look up", "", r.noPosition), attention: false })
        if (r.openWater > 0)
            out.push({ text: qsTr("%n photo(s) are in no country (open water, a pole): no place names", "", r.openWater), attention: false })
        if (r.failed > 0)
            out.push({ text: qsTr("%n photo(s) could not be updated", "", r.failed), attention: true })
        return out
    }

    contentItem: ScrollView {
        id: scroller
        clip: true
        // The flat scrollbar (D-136), an overlay at the right edge: the body stops short of it by its own width.
        ScrollBar.vertical: AppScrollBar { id: bar }
        rightPadding: bar.width
        contentWidth: availableWidth
        // As tall as the step asks, up to the window less the header, the footer and the padding.
        implicitHeight: Math.min(steps.implicitHeight, (Overlay.overlay ? Overlay.overlay.height : 700) - 220)

        ColumnLayout {
            id: steps
            width: scroller.availableWidth
            spacing: 12

            // ---- the places file is not there
            ColumnLayout {
                Layout.fillWidth: true
                visible: dialog.phase === "unavailable"
                spacing: 6
                Label {
                    id: unavailableTitle
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    font.bold: true
                    text: qsTr("Place names are not installed.")
                }
                Label {
                    id: unavailableText
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: qsTr("Auroraw looks places up in a file that comes with the application, and this copy does not have it. Nothing else is affected.")
                }
            }

            // ---- the form
            Label {
                id: intro
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                visible: dialog.phase === "form"
                text: qsTr("Looks up where each photo was taken, from its position and without the internet, and fills in its city, region and country where they are empty. What you typed, or another application wrote, is never changed.")
            }
            ColumnLayout {
                id: form
                Layout.fillWidth: true
                visible: dialog.phase === "form"
                spacing: 6
                Label {
                    text: qsTr("Photos")
                    font.bold: true
                }
                AppRadioButton {
                    id: selectionScope
                    enabled: dialog.selectedCount > 0
                    text: dialog.selectedCount > 0 ? qsTr("The %n selected photo(s)", "", dialog.selectedCount)
                                                   : qsTr("The selected photos (none is selected)")
                }
                RowLayout {
                    spacing: 8
                    AppRadioButton {
                        id: sourceScope
                        enabled: dialog.sourceIds.length > 0
                        text: qsTr("Every photo of the source:")
                    }
                    AppComboBox {
                        id: sourceBox
                        Layout.fillWidth: true
                        enabled: sourceScope.checked && dialog.sourceIds.length > 0
                        model: dialog.sourceNames
                        sizingTexts: dialog.sourceNames
                        Accessible.name: qsTr("Source")
                    }
                }

                AppCheckBox {
                    id: refreshBox
                    Layout.topMargin: 6
                    text: qsTr("Also update the names Auroraw found earlier")
                    Accessible.description: dialog.refreshHint
                }
                Hint { text: dialog.refreshHint }

                Label {
                    Layout.fillWidth: true
                    Layout.topMargin: 6
                    wrapMode: Text.Wrap
                    color: Theme.quiet
                    text: qsTr("The country and the region are named in the language of the interface, the city as the place spells it.")
                }
            }

            // ---- the progress
            ColumnLayout {
                Layout.fillWidth: true
                visible: dialog.phase === "running"
                spacing: 8
                Label {
                    Layout.fillWidth: true
                    color: Theme.quiet
                    text: dialog.total > 0 ? qsTr("Looking up photo %1 of %2…").arg(dialog.done).arg(dialog.total)
                                           : qsTr("Looking up the places…")
                }
                AppProgressBar {
                    id: progress
                    Layout.fillWidth: true
                    value: dialog.share
                    Accessible.name: qsTr("Place names progress")
                }
            }

            // ---- what happened
            ColumnLayout {
                Layout.fillWidth: true
                visible: dialog.phase === "done"
                spacing: 6
                Label {
                    id: doneTitle
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    font.bold: true
                    text: dialog.stopped ? qsTr("Stopped. What was found stays, as one step.")
                                         : (dialog.lines.length > 0 ? qsTr("Done.") : qsTr("Nothing to look up."))
                }
                Repeater {
                    model: dialog.lines
                    Label {
                        required property var modelData
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        // (The words say what matters; the colour only helps: it is never the one signal.)
                        color: modelData.attention ? Theme.warning : palette.windowText
                        text: modelData.text
                    }
                }
                Label {
                    Layout.fillWidth: true
                    Layout.topMargin: 6
                    wrapMode: Text.Wrap
                    color: Theme.quiet
                    visible: dialog.report.filled > 0
                    text: qsTr("It is one step: Undo takes it back for all of these photos.")
                }
            }
        }
    }

    footer: AppDialogButtonBox {
        AppButton {
            id: findButton
            visible: dialog.phase === "form"
            text: qsTr("Find")
            highlighted: true
            enabled: dialog.canFind
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            onClicked: dialog.start()
        }
        AppButton {
            id: stopButton
            visible: dialog.phase === "running"
            text: qsTr("Stop")
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            onClicked: dialog.finder.cancel()
        }
        AppButton {
            id: closeButton
            visible: dialog.phase !== "running"
            text: dialog.phase === "form" ? qsTr("Cancel") : qsTr("Close")
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            onClicked: dialog.close()
        }
    }
}
