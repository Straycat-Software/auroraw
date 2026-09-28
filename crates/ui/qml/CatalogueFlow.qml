// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import org.auroraw.ui

// What the catalogue task does (spec §5.1): adding a folder as a source, scanning it, rescanning,
// removing. It holds the dialogs (they open from wherever the window is, since a scan asks its
// question whichever task is showing) and follows the job the `SourceList` says is running through
// the `Bus`. The panel (`Catalogue.qml`) is only what shows it.
Item {
    id: flow
    required property var host
    required property var sources
    property var hostWindow: null

    // What the scan or removal in progress says.
    property string status: ""
    property real progress: 0
    readonly property bool busy: sources.job !== ""

    property alias addDialog: addDialog
    property alias removeDialog: removeDialog
    property alias restoreDialog: restoreDialog
    readonly property bool dialogOpen: addDialog.visible || removeDialog.visible || restoreDialog.visible
    // A native folder dialog is open (the window waits for it).
    readonly property bool browsing: addDialog.browsing

    // The scan asked whether to restore while another dialog was open: put once that one is closed.
    property bool restoreWaiting: false
    property string removingName: ""
    property int removingRow: -1

    function begin() {
        progress = 0
        status = qsTr("Reading photos: %1 of %2…").arg(0).arg(0)
    }

    function openAdd() { addDialog.openWith(sources.lastFolder()) }

    // Scans the folder an import has just made a source of (its other photos, if any).
    function scanFolder(folder) {
        if (sources.rescanFolder(folder) === "")
            begin()
        sources.refresh()
    }

    function askRemove(row, name) {
        removingRow = row
        removingName = name
        removeDialog.sourceName = name
        removeDialog.photos = sources.photosAt(row)
        removeDialog.workedOn = sources.workedOnAt(row)
        removeDialog.open()
    }

    function rescan(row) {
        const reason = sources.rescan(row)
        if (reason !== "")
            status = qsTr("The scan could not start: %1").arg(reason)
        else
            begin()
    }

    // The dialog's Add (or Merge and add): what was typed is understood first, then the plan says
    // whether the folder holds other sources.
    function tryAdd(merge) {
        const resolved = sources.resolve(addDialog.folderField.text)
        if (resolved.indexOf("error:") === 0) {
            addDialog.error = qsTr("Cannot add the source: %1").arg(resolved.substring(6))
            return
        }
        // The dialog shows back what was understood.
        addDialog.folderField.text = resolved
        addDialog.error = ""
        if (!merge) {
            const plan = sources.plan(resolved)
            if (plan.indexOf("contains:") === 0) {
                addDialog.mergeQuestion = qsTr("This folder contains the sources %1. Merge them into the new one? Their photos are kept, with their ratings and versions.")
                    .arg(plan.substring(9))
                return
            }
        }
        const reason = sources.add(resolved, addDialog.nameField.text, merge)
        if (reason !== "") {
            addDialog.mergeQuestion = ""
            addDialog.error = qsTr("Cannot add the source: %1").arg(reason)
            return
        }
        addDialog.close()
        begin()
    }

    // Called when a dialog has closed: the question that waited may be put now.
    function askWaitingQuestion() {
        if (restoreWaiting && !host.dialogOpen && busy) {
            restoreWaiting = false
            restoreDialog.open()
        }
    }

    function finish() {
        sources.job = ""
        sources.refresh()
    }

    Connections {
        target: Bus
        function onIndexPlanned(job, newFiles, restorable) {
            if (job !== flow.sources.job || restorable <= 0)
                return
            restoreDialog.restorable = restorable
            restoreDialog.total = newFiles
            // Dialogs are modal: one that is open is not replaced.
            if (flow.host.dialogOpen)
                flow.restoreWaiting = true
            else
                restoreDialog.open()
        }
        function onJobProgress(job, done, total) {
            if (job !== flow.sources.job || total <= 0)
                return
            flow.progress = done / total
            flow.status = qsTr("Reading photos: %1 of %2…").arg(done).arg(total)
        }
        function onIndexFinished(job, added, restored, known, failed, duplicates) {
            if (job !== flow.sources.job)
                return
            // Issue #16: a separate sentence, not a clause tacked onto the usual one, so nothing about
            // word order is assumed across languages; the usual one is untouched when nothing joined.
            flow.status = duplicates > 0
                ? qsTr("Done: %1 added, %2 restored, %3 already known, %4 not readable, %5 duplicate(s) found.")
                      .arg(added).arg(restored).arg(known).arg(failed).arg(duplicates)
                : qsTr("Done: %1 added, %2 restored, %3 already known, %4 not readable.")
                      .arg(added).arg(restored).arg(known).arg(failed)
            flow.finish()
        }
        function onIndexAborted(job, reason) {
            if (job !== flow.sources.job)
                return
            flow.status = qsTr("The scan stopped: %1").arg(reason)
            flow.sources.job = ""
        }
        function onSourceRemoved(job, removed, kept) {
            if (job !== flow.sources.job)
                return
            flow.status = qsTr("Source \"%1\" removed; %n photo(s) left the catalogue.", "", removed)
                .arg(flow.removingName)
            flow.finish()
        }
        function onJobCancelled(job) {
            if (job !== flow.sources.job)
                return
            flow.status = qsTr("Scan cancelled. Rescanning the source picks up where it stopped.")
            flow.finish()
        }
    }

    AddSourceDialog {
        id: addDialog
        hostWindow: flow.hostWindow
        onAddRequested: flow.tryAdd(false)
        onMergeRequested: flow.tryAdd(true)
    }

    RemoveSourceDialog {
        id: removeDialog
        onConfirmed: {
            close()
            const reason = flow.sources.remove(flow.removingRow)
            if (reason === "") {
                flow.progress = 0
                flow.status = ""
            } else {
                flow.status = qsTr("The removal could not start: %1").arg(reason)
            }
        }
    }

    RestoreDialog {
        id: restoreDialog
        onChosen: restore => flow.sources.continueScan(restore)
        onCancelled: flow.sources.cancelScan()
    }
}
