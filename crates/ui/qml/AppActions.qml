// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import org.auroraw.ui

// Every command the menus and the keyboard reach (spec §3, `commands.rs`): one Action each, with its
// shortcut and when it is available. The menu lists them and the shortcuts fire them, so the two
// cannot drift apart. `host` is the main window: it says whether a dialog is open, whether a
// workspace is, and which text field the Edit commands act on, and it carries out the commands.
QtObject {
    id: root
    required property var host

    readonly property Action newWorkspace: Action {
        property string commandId: "file.new-workspace"
        text: qsTr("New workspace…")
        shortcut: StandardKey.New
        enabled: !root.host.dialogOpen
        onTriggered: root.host.newWorkspace()
    }
    readonly property Action openWorkspace: Action {
        property string commandId: "file.open-workspace"
        text: qsTr("Open workspace…")
        shortcut: StandardKey.Open
        enabled: !root.host.dialogOpen
        onTriggered: root.host.openWorkspace()
    }
    readonly property Action importPhotos: Action {
        property string commandId: "file.import"
        text: qsTr("Import…")
        shortcut: "Ctrl+I"
        enabled: root.host.inWorkspace && !root.host.dialogOpen
        onTriggered: root.host.showImport()
    }
    readonly property Action settings: Action {
        property string commandId: "file.settings"
        text: qsTr("Settings…")
        shortcut: "Ctrl+,"
        enabled: !root.host.dialogOpen
        onTriggered: root.host.showSettings()
    }
    readonly property Action quit: Action {
        property string commandId: "file.quit"
        text: qsTr("Quit")
        shortcut: StandardKey.Quit
        onTriggered: Qt.quit()
    }

    // Undo and Redo: a text field that has the keyboard has its own (what was typed in it is the most recent
    // thing the person did); otherwise they undo and redo what was done to the photos (D-096), which the
    // engine keeps and `History` reports. The menu says what: "Undo rating".
    readonly property bool fieldCanUndo: !!root.host.editTarget && root.host.editTarget.canUndo === true
    readonly property bool fieldCanRedo: !!root.host.editTarget && root.host.editTarget.canRedo === true

    function undoText(kind, count) {
        switch (kind) {
        case "rating": return qsTr("Undo %n rating(s)", "", count)
        case "flag": return qsTr("Undo %n flag(s)", "", count)
        case "label": return qsTr("Undo %n label(s)", "", count)
        case "series-group": return qsTr("Undo grouping %n photo(s)", "", count)
        case "series-ungroup": return qsTr("Undo taking photos out of a series")
        case "series-resolve": return qsTr("Undo resolving the series")
        case "series-reopen": return qsTr("Undo reopening the series")
        case "keywords": return qsTr("Undo keywords of %n photo(s)", "", count)
        case "keyword-create": return qsTr("Undo creating the keyword")
        case "keyword-rename": return qsTr("Undo renaming the keyword")
        case "keyword-move": return qsTr("Undo moving the keyword")
        case "keyword-delete": return qsTr("Undo deleting %n keyword(s)", "", count)
        case "external-changes": return qsTr("Undo accepting external changes to %n photo(s)", "", count)
        case "keyword-properties": return qsTr("Undo keyword properties")
        case "collection-create": return qsTr("Undo creating the collection")
        case "collection-rename": return qsTr("Undo renaming the collection")
        case "collection-move": return qsTr("Undo moving the collection")
        case "collection-delete": return qsTr("Undo deleting %n collection(s)", "", count)
        case "collection-add": return qsTr("Undo adding %n photo(s) to a collection", "", count)
        case "collection-remove": return qsTr("Undo taking %n photo(s) out of a collection", "", count)
        case "meta-title": return qsTr("Undo the title of %n photo(s)", "", count)
        case "meta-caption": return qsTr("Undo the caption of %n photo(s)", "", count)
        case "meta-creator": return qsTr("Undo the creator of %n photo(s)", "", count)
        case "meta-rights": return qsTr("Undo the copyright notice of %n photo(s)", "", count)
        case "meta-usage-terms": return qsTr("Undo the usage terms of %n photo(s)", "", count)
        case "meta-web-statement": return qsTr("Undo the web statement of rights of %n photo(s)", "", count)
        case "meta-credit": return qsTr("Undo the credit line of %n photo(s)", "", count)
        case "meta-source": return qsTr("Undo the source of %n photo(s)", "", count)
        case "meta-headline": return qsTr("Undo the headline of %n photo(s)", "", count)
        case "meta-instructions": return qsTr("Undo the instructions of %n photo(s)", "", count)
        case "meta-sublocation": return qsTr("Undo the sublocation of %n photo(s)", "", count)
        case "meta-city": return qsTr("Undo the city of %n photo(s)", "", count)
        case "meta-region": return qsTr("Undo the region of %n photo(s)", "", count)
        case "meta-country": return qsTr("Undo the country of %n photo(s)", "", count)
        case "meta-country-code": return qsTr("Undo the country code of %n photo(s)", "", count)
        case "meta-persons": return qsTr("Undo the persons shown of %n photo(s)", "", count)
        case "meta-event": return qsTr("Undo the event of %n photo(s)", "", count)
        case "meta-custom": return qsTr("Undo the custom field of %n photo(s)", "", count)
        case "batch": return qsTr("Undo the change to %n photo(s)", "", count)
        }
        return qsTr("Undo")
    }

    function redoText(kind, count) {
        switch (kind) {
        case "rating": return qsTr("Redo %n rating(s)", "", count)
        case "flag": return qsTr("Redo %n flag(s)", "", count)
        case "label": return qsTr("Redo %n label(s)", "", count)
        case "series-group": return qsTr("Redo grouping %n photo(s)", "", count)
        case "series-ungroup": return qsTr("Redo taking photos out of a series")
        case "series-resolve": return qsTr("Redo resolving the series")
        case "series-reopen": return qsTr("Redo reopening the series")
        case "keywords": return qsTr("Redo keywords of %n photo(s)", "", count)
        case "keyword-create": return qsTr("Redo creating the keyword")
        case "keyword-rename": return qsTr("Redo renaming the keyword")
        case "keyword-move": return qsTr("Redo moving the keyword")
        case "keyword-delete": return qsTr("Redo deleting %n keyword(s)", "", count)
        case "external-changes": return qsTr("Redo accepting external changes to %n photo(s)", "", count)
        case "keyword-properties": return qsTr("Redo keyword properties")
        case "collection-create": return qsTr("Redo creating the collection")
        case "collection-rename": return qsTr("Redo renaming the collection")
        case "collection-move": return qsTr("Redo moving the collection")
        case "collection-delete": return qsTr("Redo deleting %n collection(s)", "", count)
        case "collection-add": return qsTr("Redo adding %n photo(s) to a collection", "", count)
        case "collection-remove": return qsTr("Redo taking %n photo(s) out of a collection", "", count)
        case "meta-title": return qsTr("Redo the title of %n photo(s)", "", count)
        case "meta-caption": return qsTr("Redo the caption of %n photo(s)", "", count)
        case "meta-creator": return qsTr("Redo the creator of %n photo(s)", "", count)
        case "meta-rights": return qsTr("Redo the copyright notice of %n photo(s)", "", count)
        case "meta-usage-terms": return qsTr("Redo the usage terms of %n photo(s)", "", count)
        case "meta-web-statement": return qsTr("Redo the web statement of rights of %n photo(s)", "", count)
        case "meta-credit": return qsTr("Redo the credit line of %n photo(s)", "", count)
        case "meta-source": return qsTr("Redo the source of %n photo(s)", "", count)
        case "meta-headline": return qsTr("Redo the headline of %n photo(s)", "", count)
        case "meta-instructions": return qsTr("Redo the instructions of %n photo(s)", "", count)
        case "meta-sublocation": return qsTr("Redo the sublocation of %n photo(s)", "", count)
        case "meta-city": return qsTr("Redo the city of %n photo(s)", "", count)
        case "meta-region": return qsTr("Redo the region of %n photo(s)", "", count)
        case "meta-country": return qsTr("Redo the country of %n photo(s)", "", count)
        case "meta-country-code": return qsTr("Redo the country code of %n photo(s)", "", count)
        case "meta-persons": return qsTr("Redo the persons shown of %n photo(s)", "", count)
        case "meta-event": return qsTr("Redo the event of %n photo(s)", "", count)
        case "meta-custom": return qsTr("Redo the custom field of %n photo(s)", "", count)
        case "batch": return qsTr("Redo the change to %n photo(s)", "", count)
        }
        return qsTr("Redo")
    }

    readonly property Action undo: Action {
        property string commandId: "edit.undo"
        text: root.fieldCanUndo ? qsTr("Undo") : root.undoText(History.undoKind, History.undoCount)
        shortcut: StandardKey.Undo
        enabled: root.fieldCanUndo || (!root.host.dialogOpen && History.undoKind !== "")
        onTriggered: root.fieldCanUndo ? root.host.editTarget.undo() : History.undo()
    }
    readonly property Action redo: Action {
        property string commandId: "edit.redo"
        text: root.fieldCanRedo ? qsTr("Redo") : root.redoText(History.redoKind, History.redoCount)
        shortcut: StandardKey.Redo
        enabled: root.fieldCanRedo || (!root.host.dialogOpen && History.redoKind !== "")
        onTriggered: root.fieldCanRedo ? root.host.editTarget.redo() : History.redo()
    }
    readonly property Action cut: Action {
        property string commandId: "edit.cut"
        text: qsTr("Cut")
        shortcut: StandardKey.Cut
        enabled: root.host.editTarget !== null
        onTriggered: root.host.editTarget.cut()
    }
    readonly property Action copy: Action {
        property string commandId: "edit.copy"
        text: qsTr("Copy")
        shortcut: StandardKey.Copy
        enabled: root.host.editTarget !== null
        onTriggered: root.host.editTarget.copy()
    }
    readonly property Action paste: Action {
        property string commandId: "edit.paste"
        text: qsTr("Paste")
        shortcut: StandardKey.Paste
        enabled: root.host.editTarget && root.host.editTarget.canPaste === true
        onTriggered: root.host.editTarget.paste()
    }
    readonly property Action deleteSelection: Action {
        property string commandId: "edit.delete"
        text: qsTr("Delete")
        shortcut: StandardKey.Delete
        enabled: root.host.editTarget !== null
        onTriggered: {
            // By the selected text and the cursor, which is at one end of it: Qt 6.4 leaves
            // selectionStart and selectionEnd stale (0) after an undo restored a selection.
            const field = root.host.editTarget
            const length = field.selectedText.length
            if (length === 0)
                return
            const cursor = field.cursorPosition
            const before = field.getText(cursor - length, cursor) === field.selectedText
            field.remove(before ? cursor - length : cursor, before ? cursor : cursor + length)
        }
    }
    // Select all is the text field's when one has the keyboard, else the grid's (D-097).
    readonly property Action selectAll: Action {
        property string commandId: "edit.select-all"
        text: qsTr("Select all")
        shortcut: StandardKey.SelectAll
        enabled: root.host.editTarget !== null || root.host.gridActive
        onTriggered: root.host.editTarget !== null ? root.host.editTarget.selectAll() : root.host.selectPhotos("all")
    }
    readonly property Action selectNone: Action {
        property string commandId: "edit.select-none"
        text: qsTr("Select none")
        shortcut: "Ctrl+Shift+A"
        enabled: root.host.gridActive && root.host.selectedPhotos > 0
        onTriggered: root.host.selectPhotos("none")
    }
    readonly property Action editKeywords: Action {
        property string commandId: "edit.keywords"
        text: qsTr("Keywords")
        shortcut: "Ctrl+K"
        enabled: root.host.gridActive
        onTriggered: root.host.focusKeywords()
    }
    readonly property Action duplicates: Action {
        property string commandId: "edit.duplicates"
        text: qsTr("Duplicate photos…")
        shortcut: "Ctrl+D"
        enabled: root.host.inWorkspace && !root.host.dialogOpen
        onTriggered: root.host.showDuplicates()
    }
    readonly property Action invertSelection: Action {
        property string commandId: "edit.invert-selection"
        text: qsTr("Invert selection")
        shortcut: "Ctrl+Shift+I"
        enabled: root.host.gridActive
        onTriggered: root.host.selectPhotos("invert")
    }

    readonly property Action about: Action {
        property string commandId: "help.about"
        text: qsTr("About Auroraw")
        shortcut: StandardKey.HelpContents
        enabled: !root.host.dialogOpen
        onTriggered: root.host.showAbout()
    }
}
