// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// Import (spec §5.2): copies the photos of a card or folder to another folder, verified, with an
// optional second destination. It is separate from the catalogue: the destination need not be one of
// its sources. When it is (or is added as one) the photos also enter the catalogue. `ImportForm` (Rust)
// does the work and answers with codes; the sentences are here, so that they are translated.
AppDialog {
    id: dialog
    required property var form
    required property var sources
    required property var flow
    required property var host
    required property var placeNames
    // The interface's language (`en`, `fr`): the names are given in it.
    required property string language
    property var hostWindow: null

    preferredWidth: 880
    title: qsTr("Import")

    // The fields, for the tests.
    property alias sourceField: sourceField
    property alias destinationField: destinationField
    property alias backupField: backupField
    property alias templateField: templateField
    property alias importButton: importButton
    property alias cancelButton: cancelButton
    property alias closeButton: closeButton
    property alias showPhotosButton: showPhotosButton
    property alias templateButton: templateButton
    property alias foldersButton: foldersButton
    property alias addDestinationBox: addDestinationBox
    property alias findPlacesBox: findPlacesBox
    property alias sourcePicker: sourcePicker
    property alias destinationPicker: destinationPicker
    property alias backupPicker: backupPicker
    // A native folder dialog is open for one of the fields (the window waits for it).
    readonly property bool browsing: sourcePicker.visible || destinationPicker.visible || backupPicker.visible

    // "template" (the folders and names below) or "folders" (the card's own).
    property string layoutChoice: "template"
    property bool addDestination: true
    // Find the place names of the photos this import registers, once it has finished (design note 008 §4): off until
    // the person turns it on, and only with the places file (`placeNames.installed()`).
    property bool findPlaces: false
    property bool placesInstalled: false
    // The run of place names that follows an import that asked for it, and what the import itself said.
    property string placeJob: ""
    property string importStatus: ""
    // What the fields mean for the catalogue (see `ImportForm.inspect`).
    property bool hasFolders: false
    property string folders: ""
    property string kind: ""
    property string kindNames: ""
    property bool registering: false
    // The cards and drives mounted now.
    property var volumes: []

    readonly property bool importing: form.job !== ""
    property bool finished: false
    property string status: ""
    property real progress: 0

    readonly property string findPlacesHint: placesInstalled
        ? qsTr("Looks up where each photo was taken, without the internet, and fills in its city, region and country where they are empty. For the photos that enter the catalogue.")
        : qsTr("Place names are not installed.")

    readonly property string destinationNote: {
        if (kind === "covered")
            return qsTr("This folder is part of the source \"%1\": the photos also enter the catalogue.").arg(kindNames)
        if (kind === "not-covered")
            return addDestination
                ? qsTr("This folder becomes a source: the photos also enter the catalogue.")
                : qsTr("This folder is not in the catalogue: the photos are only copied.")
        if (kind === "contains")
            return qsTr("This folder contains the sources %1: the photos are only copied.").arg(kindNames)
        return ""
    }

    closePolicy: importing ? Popup.NoAutoClose : Popup.CloseOnEscape

    // What was typed last time in this workspace.
    function loadRemembered() {
        const saved = JSON.parse(form.loadRemembered())
        sourceField.text = saved.source
        destinationField.text = saved.destination
        backupField.text = saved.backup
        templateField.text = saved.template
        layoutChoice = saved.layout
        addDestination = saved.add_destination
        findPlaces = saved.find_places === true
        placesInstalled = dialog.placeNames.installed()
        placeJob = ""
        importStatus = ""
        finished = false
        status = ""
        progress = 0
        refreshVolumes()
        fieldsChanged()
    }

    function openWith(source) {
        if (source !== "")
            sourceField.text = source
        refreshVolumes()
        fieldsChanged()
        open()
    }

    function refreshVolumes() {
        try {
            volumes = JSON.parse(form.volumes())
        } catch (e) {
            volumes = []
        }
        // A single card, and nothing typed yet: offer it (one click to import).
        if (sourceField.text === "" && volumes.length === 1)
            sourceField.text = volumes[0].path
    }

    // What the source and the destination are, now that they were typed.
    function fieldsChanged() {
        const info = JSON.parse(form.inspect(sourceField.text, destinationField.text, addDestination))
        hasFolders = info.hasFolders
        folders = info.folders
        kind = info.kind
        kindNames = info.names
        registering = info.registering
    }

    function fields() {
        return {
            source: sourceField.text.trim(),
            destination: destinationField.text.trim(),
            backup: backupField.text.trim(),
            template: templateField.text.trim(),
            layout: layoutChoice,
            add_destination: addDestination,
            find_places: findPlaces
        }
    }

    // Shows the folder a field was understood as (nothing changes when it cannot be understood: the
    // import says why not).
    function showUnderstood(field) {
        const understood = sources.resolve(field.text)
        if (understood.indexOf("error:") !== 0)
            field.text = understood
    }

    function start() {
        fieldsChanged()
        showUnderstood(sourceField)
        showUnderstood(destinationField)
        const reason = form.start(JSON.stringify(fields()))
        if (reason !== "") {
            status = qsTr("Cannot start the import: %1").arg(reason)
            return
        }
        finished = false
        progress = 0
        status = qsTr("Reading the source…")
        sources.refresh()
    }

    Connections {
        target: Bus
        function onJobProgress(job, done, total) {
            if (job !== dialog.form.job || total <= 0)
                return
            dialog.progress = done / total
            dialog.status = qsTr("Importing %1 of %2…").arg(done).arg(total)
        }
        function onImportFinished(job, copied, skipped, failed) {
            if (job !== dialog.form.job)
                return
            dialog.form.job = ""
            dialog.finished = true
            dialog.status = skipped === 0 && failed === 0
                ? qsTr("All %n file(s) copied and verified.", "", copied)
                : qsTr("%1 copied, %2 already in the library, %3 failed. Run it again to retry.")
                    .arg(copied).arg(skipped).arg(failed)
            // The place names of what it registered, when the person asked for them: one run, one undoable step.
            dialog.importStatus = dialog.status
            if (dialog.findPlaces && dialog.placesInstalled) {
                const started = dialog.placeNames.startAfterImport(job, dialog.language)
                if (started.indexOf("error:") === 0) {
                    dialog.status = dialog.importStatus + " " + qsTr("The place names could not be looked up: %1").arg(started.substring(6))
                } else if (started !== "") {
                    dialog.placeJob = started
                    dialog.status = dialog.importStatus + " " + qsTr("Finding the place names…")
                }
            } else {
                // Nobody asked: what the import registered is not kept for a run that will not come.
                dialog.placeNames.forgetImport(job)
            }
            // A destination made a source for this import: its other photos, if any, are scanned now
            // (the imported ones are known already).
            const added = dialog.form.takeAddedSource()
            if (added !== "")
                dialog.flow.scanFolder(added)
            else
                dialog.sources.refresh()
        }
        function onPlaceNamesFound(job, report, cancelled) {
            if (job !== dialog.placeJob)
                return
            dialog.placeJob = ""
            const r = JSON.parse(report)
            dialog.status = dialog.importStatus + " "
                + (cancelled ? qsTr("Place names: stopped.")
                   : r.filled > 0 ? qsTr("Place names found for %n photo(s).", "", r.filled)
                                  : qsTr("No place names were found: the photos have no position, or are in no country."))
        }
        function onImportAborted(job, reason) {
            if (job !== dialog.form.job)
                return
            dialog.form.job = ""
            dialog.placeNames.forgetImport(job)
            dialog.status = qsTr("The import stopped: %1").arg(reason)
        }
        function onJobCancelled(job) {
            if (job !== dialog.form.job)
                return
            dialog.form.job = ""
            dialog.finished = true
            dialog.placeNames.forgetImport(job)
            dialog.status = qsTr("Import cancelled. Running it again resumes where it stopped.")
        }
    }

    contentItem: ScrollView {
        id: scroller
        clip: true
        // The flat scrollbar (D-136, which left the two scroll views' Fusion ones behind in D-132): an overlay at the
        // right edge, so the form stops short of it by its own width.
        ScrollBar.vertical: AppScrollBar { id: importBar }
        rightPadding: importBar.width
        contentWidth: availableWidth
        implicitHeight: Math.min(grid.implicitHeight,
                                 (Overlay.overlay ? Overlay.overlay.height : 700) - 220)

        GridLayout {
            id: grid
            width: scroller.availableWidth
            columns: 3
            columnSpacing: 12
            rowSpacing: 12

            Label {
                Layout.columnSpan: 3
                text: qsTr("Cards and drives")
                font.bold: true
            }
            RowLayout {
                Layout.columnSpan: 3
                spacing: 12
                Repeater {
                    model: dialog.volumes
                    AppButton {
                        required property var modelData
                        text: modelData.name + " (" + modelData.path + ")"
                        onClicked: {
                            sourceField.text = modelData.path
                            dialog.fieldsChanged()
                        }
                    }
                }
                Label {
                    visible: dialog.volumes.length === 0
                    text: qsTr("No card detected")
                    color: Theme.quiet
                }
                AppButton {
                    id: refreshButton
                    text: qsTr("Refresh")
                    onClicked: dialog.refreshVolumes()
                }
                Item { Layout.fillWidth: true }
            }

            Label { text: qsTr("Import from (card or folder)") }
            AppTextField {
                id: sourceField
                Layout.fillWidth: true
                enabled: !dialog.importing
                Accessible.name: qsTr("Import from (card or folder)")
                onTextEdited: dialog.fieldsChanged()
            }
            AppButton {
                text: qsTr("Browse…")
                Accessible.name: qsTr("Browse for: %1").arg(qsTr("Import from (card or folder)"))
                enabled: !dialog.browsing && !dialog.importing
                onClicked: sourcePicker.pick()
            }
            Label {
                Layout.columnSpan: 3
                Layout.fillWidth: true
                visible: dialog.hasFolders
                wrapMode: Text.Wrap
                color: Theme.quiet
                text: qsTr("The photos are in camera folders (%1).").arg(dialog.folders)
            }

            Label { text: qsTr("Destination folder") }
            AppTextField {
                id: destinationField
                Layout.fillWidth: true
                enabled: !dialog.importing
                placeholderText: qsTr("Where the photos are copied to")
                Accessible.name: qsTr("Destination folder")
                onTextEdited: dialog.fieldsChanged()
            }
            AppButton {
                text: qsTr("Browse…")
                Accessible.name: qsTr("Browse for: %1").arg(qsTr("Destination folder"))
                enabled: !dialog.browsing && !dialog.importing
                onClicked: destinationPicker.pick()
            }
            Label {
                Layout.columnSpan: 3
                Layout.fillWidth: true
                visible: dialog.destinationNote !== ""
                wrapMode: Text.Wrap
                color: dialog.kind === "covered" ? Theme.quiet : Theme.warning
                text: dialog.destinationNote
            }
            AppCheckBox {
                id: addDestinationBox
                Layout.columnSpan: 3
                visible: dialog.kind === "not-covered"
                enabled: !dialog.importing
                text: qsTr("Add this folder to the catalogue's sources")
                checked: dialog.addDestination
                onToggled: {
                    dialog.addDestination = checked
                    dialog.fieldsChanged()
                }
            }

            Label { text: qsTr("Folder layout") }
            RowLayout {
                Layout.columnSpan: 2
                spacing: 12
                AppButton {
                    id: templateButton
                    enabled: !dialog.importing
                    text: qsTr("Use the template")
                    highlighted: dialog.layoutChoice === "template"
                    onClicked: dialog.layoutChoice = "template"
                }
                AppButton {
                    id: foldersButton
                    enabled: !dialog.importing
                    text: qsTr("Keep the source's folders")
                    highlighted: dialog.layoutChoice === "folders"
                    onClicked: dialog.layoutChoice = "folders"
                }
                Item { Layout.fillWidth: true }
            }

            Label {
                visible: dialog.layoutChoice === "template"
                text: qsTr("Folders and file names")
            }
            AppTextField {
                id: templateField
                visible: dialog.layoutChoice === "template"
                Layout.fillWidth: true
                Layout.columnSpan: 2
                enabled: !dialog.importing
                placeholderText: "{year}/{date}/{original}.{ext}"
                Accessible.name: qsTr("Folders and file names")
            }

            Label { text: qsTr("Backup folder (optional)") }
            AppTextField {
                id: backupField
                Layout.fillWidth: true
                enabled: !dialog.importing
                Accessible.name: qsTr("Backup folder (optional)")
            }
            AppButton {
                text: qsTr("Browse…")
                Accessible.name: qsTr("Browse for: %1").arg(qsTr("Backup folder (optional)"))
                enabled: !dialog.browsing && !dialog.importing
                onClicked: backupPicker.pick()
            }

            AppCheckBox {
                id: findPlacesBox
                Layout.columnSpan: 3
                enabled: !dialog.importing && dialog.placesInstalled
                text: qsTr("Find the place names of the imported photos")
                checked: dialog.findPlaces
                onToggled: dialog.findPlaces = checked
                Accessible.description: dialog.findPlacesHint
            }
            Label {
                Layout.columnSpan: 3
                Layout.fillWidth: true
                Layout.leftMargin: findPlacesBox.indicator.width + findPlacesBox.spacing
                wrapMode: Text.Wrap
                color: Theme.quiet
                text: dialog.findPlacesHint
            }

            AppProgressBar {
                Layout.columnSpan: 3
                Layout.fillWidth: true
                visible: dialog.importing
                value: dialog.progress
            }
            Label {
                Layout.columnSpan: 3
                Layout.fillWidth: true
                visible: dialog.status !== ""
                wrapMode: Text.Wrap
                color: Theme.quiet
                text: dialog.status
            }
        }
    }

    footer: AppDialogButtonBox {
        AppButton {
            id: showPhotosButton
            visible: dialog.finished && dialog.registering
            text: qsTr("Show photos")
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            onClicked: dialog.host.showPhotos()
        }
        AppButton {
            id: cancelButton
            text: qsTr("Cancel import")
            enabled: dialog.importing
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            onClicked: dialog.form.cancel()
        }
        AppButton {
            id: importButton
            text: qsTr("Import")
            highlighted: true
            enabled: !dialog.importing
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            onClicked: dialog.start()
        }
        // An import that runs is stopped with "Cancel import", not by closing the window over it.
        AppButton {
            id: closeButton
            text: qsTr("Close")
            enabled: !dialog.importing
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            onClicked: dialog.close()
        }
    }

    FolderPicker {
        id: sourcePicker
        field: sourceField
        hostWindow: dialog.hostWindow
        title: qsTr("Choose the card or folder to import from")
        onChosen: dialog.fieldsChanged()
    }
    FolderPicker {
        id: destinationPicker
        field: destinationField
        hostWindow: dialog.hostWindow
        title: qsTr("Choose the destination folder")
        onChosen: dialog.fieldsChanged()
    }
    FolderPicker {
        id: backupPicker
        field: backupField
        hostWindow: dialog.hostWindow
        title: qsTr("Choose the backup folder")
    }
}
