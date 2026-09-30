// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import org.auroraw.ui

ApplicationWindow {
    id: window
    // Tests: a machine (a folder under AURORAW_TEST_HOME) to run on, chosen before the launcher starts.
    property string machine: ""

    visible: true
    width: 1400
    height: 900
    minimumWidth: 640
    minimumHeight: 420
    color: palette.window
    background: Rectangle { color: window.palette.window }
    // The surface ladder and its accents come from `Theme` (D-094, refreshed by D-129); every
    // control inherits this palette. Dialogs and popups paint their own `Theme.surface.raised`
    // background (AppDialog.qml, AppMenu.qml) rather than reading it from here, since Fusion's own
    // dialog/popup roles (`window`, `base`) are shared with the canvas and sunken surfaces.
    palette {
        window: Theme.surface.window
        windowText: Theme.surface.text
        base: Theme.surface.sunken
        alternateBase: Theme.surface.window
        text: Theme.surface.text
        button: Theme.surface.hover
        buttonText: Theme.surface.text
        light: Theme.surface.border
        midlight: Theme.surface.hover
        mid: Theme.surface.sunken
        dark: Theme.surface.sunken
        shadow: "#000000"
        highlight: Theme.accentFill
        highlightedText: "#ffffff"
        placeholderText: Theme.surface.placeholder
        // What cannot be used is dimmed (Fusion draws a disabled control from these).
        disabled {
            windowText: Theme.surface.placeholder
            text: Theme.surface.placeholder
            buttonText: Theme.surface.placeholder
        }
        toolTipBase: Theme.surface.sunken
        toolTipText: Theme.surface.text
    }
    title: launcher.screen === "workspace" ? launcher.workspaceName + " — Auroraw" : "Auroraw"

    // What the tests reach into (tests/qml/tst_*.qml).
    property alias welcome: welcomeView
    property alias library: libraryView
    property alias keywordPanel: libraryView.keywordPanel
    property alias catalogue: catalogueView
    property alias flow: catalogueFlow
    property alias sources: sourceList
    property alias newDialog: newDialog
    property alias settingsDialog: settingsDialog
    property alias aboutDialog: aboutDialog
    property alias duplicatesDialog: duplicatesDialog
    property alias externalBanner: externalBanner
    property alias externalDialog: externalDialog
    property alias externalChanges: externalChanges
    property alias openDialog: openDialog
    property alias importDialog: importDialog
    property alias cardBanner: cardBanner
    property alias importForm: importForm
    property alias waiting: waiting
    property alias launcher: launcher
    property alias known: known
    property alias photos: photoGrid
    property alias actions: actions
    property alias menu: appMenu
    property alias hamburger: hamburger
    property alias tabs: tabs
    property alias noticeBar: noticeBar

    Launcher { id: launcher }
    KnownWorkspaces { id: known }
    PhotoGrid { id: photoGrid }
    SourceList { id: sourceList }
    ImportForm { id: importForm }
    CatalogueFlow {
        id: catalogueFlow
        host: window
        sources: sourceList
        hostWindow: window
    }
    AppActions { id: actions; host: window }

    // The task on screen: what a workspace opens on is the catalogue when it is new or empty, and
    // the grid otherwise (D-090).
    property string currentTask: "cull"
    // Something that could not be done, said at the top until dismissed.
    property string notice: ""

    // A dialog is open: every command that opens a window waits (the mouse is blocked by the
    // dialog's own modality; keyboard shortcuts may not be, depending on the Qt version and the
    // kind of shortcut, so the actions check this).
    // A native folder dialog is another window: Qt cannot block this one for it, so while one is open
    // a modal popup covers everything (the menu included) and the commands wait.
    // (`nativeDialogForced` stands for one in the tests: none can open off screen.)
    property bool nativeDialogForced: false
    readonly property bool nativeDialogOpen: nativeDialogForced || openDialog.visible || catalogueFlow.browsing || newDialog.browsing
                                            || importDialog.browsing || libraryView.exportDialog.visible || duplicatesDialog.browsing
    readonly property bool dialogOpen: newDialog.visible || settingsDialog.visible || importDialog.visible || libraryView.dialogOpen
                                       || aboutDialog.visible || duplicatesDialog.visible || externalDialog.visible || catalogueFlow.dialogOpen || nativeDialogOpen
    readonly property bool inWorkspace: launcher.screen === "workspace"
    // The grid is what the person is looking at and can act on: the selection commands mean something.
    readonly property bool gridActive: inWorkspace && currentTask === "cull" && !dialogOpen && photoGrid.count > 0
    readonly property int selectedPhotos: photoGrid.selectedCount

    // The Edit commands act on the text field that has the keyboard. The menu takes the keyboard
    // while it is open, so the field that had it is remembered.
    readonly property Item focusItem: window.activeFocusItem
    property Item fieldBeforeMenu: null
    readonly property Item editTarget: appMenu.opened ? fieldBeforeMenu
                                                       : (isField(focusItem) ? focusItem : null)
    function isField(item) {
        return item !== null && item !== undefined && item.selectedText !== undefined
    }

    // Commands (what `AppActions` calls).
    function newWorkspace() { newDialog.openWith() }
    function openWorkspace() { openDialog.pick() }
    function showSettings() { settingsDialog.open() }
    function showAbout() { aboutDialog.open() }
    function showDuplicates() { duplicatesDialog.open() }
    // Ctrl+K: the keyboard goes to the keyword field.
    function focusKeywords() { libraryView.focusKeywords() }
    // The grid's selection commands (`all`, `none`, `invert`).
    function selectPhotos(what) {
        if (what === "all") libraryView.selectAll()
        else if (what === "none") libraryView.selectNone()
        else libraryView.invertSelection()
    }
    // The Import dialog, on `source` (a card) when given.
    function showImport(source) {
        if (inWorkspace && !dialogOpen)
            importDialog.openWith(source === undefined ? "" : source)
    }
    // Once photos were imported: the grid, with them.
    function showPhotos() {
        importDialog.close()
        currentTask = "cull"
        libraryView.reload()
    }
    // Opens the workspace in `folder`, or says why not.
    function openFolder(folder) {
        const reason = launcher.openPath(folder)
        if (reason !== "")
            notice = qsTr("Cannot open the workspace: %1").arg(reason)
    }

    Component.onCompleted: {
        // (Always: a test process makes several windows, each on its own machine.)
        launcher.useMachine(machine)
        // Starting loads the typeface the application carries.
        launcher.start()
        // IBM Plex Sans at 11 points, unless AURORAW_FONT and AURORAW_FONT_SIZE try another.
        Theme.fontFamily = launcher.env("AURORAW_FONT") || Theme.defaultFamily
        Theme.fontSize = parseInt(launcher.env("AURORAW_FONT_SIZE")) || Theme.defaultSize
        window.font.family = Theme.fontFamily
        window.font.pointSize = Theme.fontSize
        known.refresh()
    }
    Connections {
        target: launcher
        // Another workspace replaces the open one (New or Open while one is open): everything that
        // shows the workspace starts over.
        function onWorkspaceSerialChanged() {
            // Another workspace has a history of its own (a new engine, empty).
            History.undoKind = ""
            History.undoCount = 0
            History.redoKind = ""
            History.redoCount = 0
            catalogueFlow.status = ""
            sourceList.job = ""
            sourceList.refresh()
            importForm.job = ""
            importDialog.loadRemembered()
            cardBanner.rememberCurrent()
            externalBanner.count = externalChanges.pending()
            libraryView.filterBy(0)
            window.currentTask = photoGrid.count === 0 ? "catalogue" : "cull"
        }
        function onScreenChanged() {
            if (launcher.screen !== "workspace")
                known.refresh()
        }
    }
    // What the engine reports arrives on the Bus (a singleton, created here at the latest).
    Connections {
        target: Bus
        function onIndexFinished() { libraryView.reload() }
        function onSourceRemoved() { libraryView.reload() }
        function onImportFinished() { libraryView.reload() }
        // What Undo and Redo would do, as the engine says it.
        function onHistoryChanged(undoKind, undoCount, redoKind, redoCount) {
            History.undoKind = undoKind
            History.undoCount = undoCount
            History.redoKind = redoKind
            History.redoCount = redoCount
        }
        // An action was undone or redone: the grid shows the photos it touched.
        function onSeriesChanged() { libraryView.seriesChanged() }
        function onCollectionsChanged() { libraryView.collectionsWereChanged() }
        function onHistoryApplied(photoIds) { libraryView.historyApplied(photoIds === "" ? [] : photoIds.split(",")) }
        function onJobCancelled() { libraryView.reload() }
        // Photos with a change other software made to the XMP file beside the original, waiting for an answer.
        function onExternalChanges(photos) { externalBanner.count = photos }
        function onPhotoChanged(photoId) { libraryView.photoChanged(photoId) }
    }
    // A question that waited for a dialog is put once no dialog is open.
    onDialogOpenChanged: if (!dialogOpen) catalogueFlow.askWaitingQuestion()
    // Photos may have arrived while another task was showing.
    onCurrentTaskChanged: if (currentTask === "cull" && inWorkspace) libraryView.reload()

    property int visibilityBeforeFullScreen: Window.Windowed

    // F in the image view: the window goes full screen, and comes back (also when the view is closed).
    Connections {
        target: libraryView
        function onFullScreenToggled() {
            if (window.visibility === Window.FullScreen) {
                window.visibility = window.visibilityBeforeFullScreen
            } else {
                // The window comes back the way it was: maximised, or its own size.
                window.visibilityBeforeFullScreen = window.visibility === Window.Maximized ? Window.Maximized
                                                                                            : Window.Windowed
                window.visibility = Window.FullScreen
            }
        }
        function onComparingChanged() {
            if (!libraryView.viewing && !libraryView.comparing && window.visibility === Window.FullScreen)
                window.visibility = window.visibilityBeforeFullScreen
        }
        function onViewingChanged() {
            if (!libraryView.viewing && !libraryView.comparing && window.visibility === Window.FullScreen)
                window.visibility = window.visibilityBeforeFullScreen
        }
    }

    // Alt and a section's mnemonic open it.
    Repeater {
        model: 4
        Item {
            required property int index
            Shortcut {
                sequence: appMenu.sectionKey(index)
                enabled: !window.nativeDialogOpen && sequence !== ""
                onActivated: appMenu.openSection(index)
            }
        }
    }

    // The image view in full screen has the whole window: no menu, no tabs.
    header: ToolBar {
        visible: !((libraryView.viewing || libraryView.comparing) && window.visibility === Window.FullScreen)
        RowLayout {
            anchors.fill: parent
            spacing: 0
            AppToolButton {
                id: hamburger
                iconName: "menu"
                iconSize: 18
                Accessible.name: qsTr("Menu")
                onClicked: appMenu.opened ? appMenu.close() : appMenu.popup(hamburger, 0, hamburger.height)
                AppMenu {
                    id: appMenu
                    parent: hamburger
                    actions: actions
                    onAboutToShow: window.fieldBeforeMenu = window.isField(window.focusItem) ? window.focusItem : null
                }
            }
            TabBar {
                id: tabs
                visible: window.inWorkspace
                background: null
                currentIndex: window.currentTask === "catalogue" ? 0 : 1
                AppTabButton {
                    text: qsTr("Catalogue")
                    enabled: !window.dialogOpen
                    width: implicitWidth
                    onClicked: window.currentTask = "catalogue"
                }
                AppTabButton {
                    text: qsTr("Cull")
                    enabled: !window.dialogOpen
                    width: implicitWidth
                    onClicked: window.currentTask = "cull"
                }
                AppTabButton { text: qsTr("Develop"); enabled: false; width: implicitWidth }
                AppTabButton { text: qsTr("Publish"); enabled: false; width: implicitWidth }
            }
            Item { Layout.fillWidth: true }
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        NoticeBar {
            id: noticeBar
            Layout.fillWidth: true
            text: window.notice
            onDismissed: window.notice = ""
        }

        CardBanner {
            id: cardBanner
            Layout.fillWidth: true
            form: importForm
            host: window
            onImportRequested: path => window.showImport(path)
        }

        ExternalBanner {
            id: externalBanner
            Layout.fillWidth: true
            external: externalChanges
            host: window
            onReviewRequested: externalDialog.open()
        }

        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            Welcome {
                id: welcomeView
                anchors.fill: parent
                visible: launcher.screen === "welcome"
                launcher: launcher
                known: known
                onNewRequested: window.newWorkspace()
                onOpenRequested: window.openWorkspace()
                onKnownRequested: row => window.openFolder(known.pathAt(row))
            }
            Catalogue {
                id: catalogueView
                anchors.fill: parent
                flow: catalogueFlow
                sources: sourceList
                visible: window.inWorkspace && window.currentTask === "catalogue"
            }
            Library {
                id: libraryView
                anchors.fill: parent
                visible: window.inWorkspace && window.currentTask === "cull"
                photoGrid: photoGrid
                launcher: launcher
                hostWindow: window
            }
        }
    }

    NewWorkspaceDialog {
        id: newDialog
        launcher: launcher
        hostWindow: window
        onCreated: window.currentTask = "catalogue"
    }
    SettingsDialog { id: settingsDialog; launcher: launcher }
    ImportDialog {
        id: importDialog
        form: importForm
        sources: sourceList
        flow: catalogueFlow
        host: window
        hostWindow: window
    }
    AboutDialog { id: aboutDialog; launcher: launcher }
    DuplicatesDialog { id: duplicatesDialog; duplicates: duplicates; hostWindow: window }
    Duplicates { id: duplicates }
    ExternalChangesDialog { id: externalDialog; external: externalChanges; hostWindow: window }
    ExternalChanges { id: externalChanges }

    Popup {
        id: waiting
        parent: Overlay.overlay
        x: 0
        y: 0
        width: parent ? parent.width : 0
        height: parent ? parent.height : 0
        modal: true
        dim: true
        closePolicy: Popup.NoAutoClose
        visible: window.nativeDialogOpen
        padding: 0
        background: Rectangle { color: Theme.scrimDim }
        contentItem: Label {
            text: qsTr("Choose a folder in the folder dialog…")
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            font.pixelSize: 16
        }
    }

    // The system's own folder dialog (a Qt Quick one where the platform has none), modal to the window.
    FolderPicker {
        id: openDialog
        hostWindow: window
        title: qsTr("Open a workspace")
        rememberAs: "open-workspace"
        onChosen: path => window.openFolder(path)
    }
}
