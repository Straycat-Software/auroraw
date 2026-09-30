// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// The XMP export to the source folders (spec §5.7, D-024, D-138, D-139): writes, beside the original of each photo
// asked for, the XMP file other applications read (Lightroom, darktable, digiKam...). On explicit request only:
// nothing is written until Export is pressed. Four steps in one dialog: the form, a confirmation when a file that
// exists is to be replaced, the progress, and what happened. A file another application changed since Auroraw last
// looked is held back, not written: it is put to the review of external changes (the banner under the header),
// and exporting again writes it once it is answered.
AppDialog {
    id: dialog
    required property var exporter
    required property var sources
    required property var photoGrid
    preferredWidth: 560
    title: qsTr("Export XMP files")

    property alias scopeSelection: selectionScope
    property alias scopeSource: sourceScope
    property alias sourceBox: sourceBox
    property alias stemNaming: stemNaming
    property alias fullNaming: fullNaming
    property alias mergeExisting: mergeExisting
    property alias replaceExisting: replaceExisting
    property alias skipExisting: skipExisting
    property alias minusOneBox: minusOneBox
    property alias exportButton: exportButton
    property alias stopButton: stopButton
    property alias closeButton: closeButton
    property alias backButton: backButton
    property alias progressBar: progress

    // form, confirm (Replace asks first), running, done.
    property string phase: "form"
    property string job: ""
    property real share: 0
    property bool stopped: false
    property var report: ({})
    property var sourceNames: []
    property var sourceIds: []
    readonly property int selectedCount: dialog.photoGrid.selectedCount
    readonly property bool canExport: (selectionScope.checked && dialog.selectedCount > 0)
                                      || (sourceScope.checked && dialog.sourceIds.length > 0)

    closePolicy: dialog.phase === "running" ? Popup.NoAutoClose : Popup.CloseOnEscape

    // What a choice means, under it and in step with its text (the 15px indicator and its 6px gap): a radio button's own text does not wrap.
    component Hint: Label {
        Layout.fillWidth: true
        Layout.leftMargin: 21
        wrapMode: Text.Wrap
        color: Theme.quiet
    }

    // (The radio buttons of one question are not all children of one item, and three questions share a parent:
    // a group says which are which.)
    ButtonGroup { buttons: [selectionScope, sourceScope] }
    ButtonGroup { buttons: [stemNaming, fullNaming] }
    ButtonGroup { buttons: [mergeExisting, replaceExisting, skipExisting] }

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
        const saved = JSON.parse(dialog.exporter.choices())
        stemNaming.checked = saved.naming !== "full"
        fullNaming.checked = saved.naming === "full"
        mergeExisting.checked = saved.existing !== "skip"
        skipExisting.checked = saved.existing === "skip"
        replaceExisting.checked = false
        minusOneBox.checked = saved.minusOne
        selectionScope.checked = dialog.selectedCount > 0
        sourceScope.checked = dialog.selectedCount === 0
        sourceBox.currentIndex = names.length > 0 ? 0 : -1
        dialog.phase = "form"
        dialog.job = ""
        dialog.share = 0
        dialog.stopped = false
        dialog.report = ({})
    }

    onAboutToShow: dialog.fillForm()

    // Pressing Export: a Replace asks first, anything else starts.
    function exportPressed() {
        if (replaceExisting.checked && dialog.phase === "form") {
            dialog.phase = "confirm"
            return
        }
        dialog.start()
    }

    function start() {
        const selection = selectionScope.checked
        const target = selection ? dialog.photoGrid.selectedIds() : dialog.sourceIds[sourceBox.currentIndex]
        const started = dialog.exporter.start(selection ? "selection" : "source", target,
                                              fullNaming.checked ? "full" : "stem",
                                              replaceExisting.checked ? "replace" : (skipExisting.checked ? "skip" : "merge"),
                                              minusOneBox.checked)
        if (started.indexOf("error:") === 0) {
            dialog.report = ({ failed: 1, firstError: started.substring(6) })
            dialog.phase = "done"
            return
        }
        dialog.job = started
        dialog.share = 0
        dialog.phase = "running"
    }

    Connections {
        target: Bus
        function onJobProgress(job, done, total) {
            if (job === dialog.job && total > 0)
                dialog.share = done / total
        }
        function onXmpExportFinished(job, report, cancelled) {
            if (job !== dialog.job)
                return
            dialog.report = JSON.parse(report)
            dialog.stopped = cancelled
            dialog.phase = "done"
        }
    }

    // What happened, one line for each thing that did.
    readonly property var lines: {
        const r = dialog.report
        const out = []
        if (r.written > 0)
            out.push(qsTr("%n file(s) written", "", r.written))
        if (r.upToDate > 0)
            out.push(qsTr("%n file(s) already said the same, left as they are", "", r.upToDate))
        if (r.heldBack > 0)
            out.push(qsTr("%n file(s) held back: another application changed them since Auroraw last looked. Review the changes (the banner under the header), then export again.", "", r.heldBack))
        if (r.skippedExisting > 0)
            out.push(qsTr("%n existing file(s) left alone", "", r.skippedExisting))
        if (r.unreadable > 0)
            out.push(qsTr("%n existing file(s) that are not XMP, left untouched", "", r.unreadable))
        if (r.unreachable > 0)
            out.push(qsTr("%n photo(s) on a source that cannot be reached", "", r.unreachable))
        if (r.nameShared > 0)
            out.push(qsTr("%n file(s) named after the whole file (photo.ARW.xmp), the name photo.xmp being shared by several photos of a folder", "", r.nameShared))
        if (r.failed > 0)
            out.push(qsTr("%n photo(s) could not be exported: %1", "", r.failed).arg(r.firstError || ""))
        return out
    }

    contentItem: ColumnLayout {
        spacing: 12

        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            visible: dialog.phase === "form"
            text: qsTr("Writes an XMP file beside the original of each photo, for other applications to read. Your photos and Auroraw's own files are not changed.")
        }

        // ---- the form
        ColumnLayout {
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

            Label {
                Layout.topMargin: 6
                text: qsTr("Name of a new file")
                font.bold: true
            }
            AppRadioButton {
                id: stemNaming
                text: qsTr("Named after the photo (photo.xmp)")
            }
            AppRadioButton {
                id: fullNaming
                text: qsTr("Named after the whole file (photo.ARW.xmp)")
            }

            Label {
                Layout.topMargin: 6
                text: qsTr("When a file already exists")
                font.bold: true
            }
            AppRadioButton {
                id: mergeExisting
                text: qsTr("Merge into it")
            }
            Hint { text: qsTr("Only what Auroraw owns is rewritten; develop settings and the rest stay.") }
            AppRadioButton {
                id: replaceExisting
                text: qsTr("Replace it")
            }
            Hint { text: qsTr("The old file is kept in the workspace's removed folder.") }
            AppRadioButton {
                id: skipExisting
                text: qsTr("Leave it alone")
            }
            Hint { text: qsTr("Only the missing files are written.") }

            AppCheckBox {
                id: minusOneBox
                Layout.topMargin: 6
                text: qsTr("Write a rejected photo with a rating of −1")
            }
            Hint { text: qsTr("Other applications understand −1 as rejected; the stars stay in the file for Auroraw.") }
        }

        // ---- the confirmation of a Replace
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            visible: dialog.phase === "confirm"
            text: qsTr("Replace the files that already exist? Each old file is first kept in the workspace's removed folder; nothing is deleted.")
        }

        // ---- the progress
        ColumnLayout {
            Layout.fillWidth: true
            visible: dialog.phase === "running"
            spacing: 8
            Label {
                text: qsTr("Writing the files…")
            }
            AppProgressBar {
                id: progress
                Layout.fillWidth: true
                value: dialog.share
            }
        }

        // ---- what happened
        ColumnLayout {
            Layout.fillWidth: true
            visible: dialog.phase === "done"
            spacing: 6
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                font.bold: true
                text: dialog.stopped ? qsTr("Stopped. What was written stays written.")
                                     : (dialog.lines.length > 0 ? qsTr("Done.") : qsTr("Nothing to write."))
            }
            Repeater {
                model: dialog.lines
                Label {
                    required property string modelData
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: modelData
                }
            }
        }
    }

    footer: AppDialogButtonBox {
        AppButton {
            id: exportButton
            visible: dialog.phase === "form" || dialog.phase === "confirm"
            text: dialog.phase === "confirm" ? qsTr("Replace and export") : qsTr("Export")
            highlighted: true
            enabled: dialog.canExport
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            onClicked: dialog.exportPressed()
        }
        AppButton {
            id: backButton
            visible: dialog.phase === "confirm"
            text: qsTr("Back")
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            onClicked: dialog.phase = "form"
        }
        AppButton {
            id: stopButton
            visible: dialog.phase === "running"
            text: qsTr("Stop")
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            onClicked: dialog.exporter.cancel()
        }
        AppButton {
            id: closeButton
            visible: dialog.phase !== "running"
            text: dialog.phase === "done" ? qsTr("Close") : qsTr("Cancel")
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            onClicked: dialog.close()
        }
    }
}
