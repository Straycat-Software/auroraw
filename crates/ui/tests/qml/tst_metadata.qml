// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.ui

// The metadata panel (spec §5.7; WP10, slice 1): a field applied to one photo or a whole selection as it
// is left, "Multiple values" over a selection that disagrees, undo and redo with the panel following, a
// custom field (through the engine directly: no UI makes one yet), and French. The 40-photo machine,
// shared by the tests: each one uses fields of its own so they do not interfere.
AppTestCase {
    name: "Metadata"

    property var grid: null

    function init() {
        launchWithPhotos(40)
        grid = app.library.grid
        app.keywordPanel.tabs.currentIndex = 1
    }

    function cleanup() {
        if (app)
            app.launcher.chooseLanguage("en")
        quit()
    }

    function cell(index) {
        grid.positionViewAtIndex(index, GridView.Contain)
        wait(30)
        const item = grid.itemAtIndex(index)
        verify(item, "the cell " + index + " exists")
        return item
    }

    function click(index, modifiers) {
        const item = cell(index)
        mouseClick(item, item.width / 2, item.height / 2, Qt.LeftButton, modifiers === undefined ? Qt.NoModifier : modifiers)
        wait(40)
    }

    // The first `count` photos, selected.
    function selectFirst(count) {
        click(0)
        if (count > 1)
            click(count - 1, Qt.ShiftModifier)
        compare(app.photos.selectedCount, count)
    }

    // The row for `key` (the fixed, known order `MetadataPanel.qml`'s own `fields` lists), scrolled
    // into view first: the list is virtualised, a row off screen does not exist yet.
    function rowFor(key) {
        const list = app.keywordPanel.metadataPanel.list
        const fields = app.keywordPanel.metadataPanel.fields
        for (let i = 0; i < fields.length; i++) {
            if (fields[i].key === key) {
                list.positionViewAtIndex(i, ListView.Contain)
                wait(30)
                return list.itemAtIndex(i)
            }
        }
        return null
    }

    // Types `text` in the field `key` (a single line) and blurs it, the way leaving the field does.
    function typeField(key, text) {
        const row = rowFor(key)
        verify(row, "the row for " + key + " exists")
        row.singleLine.forceActiveFocus()
        row.singleLine.text = text
        grid.forceActiveFocus()
        wait(60)
    }

    function test_a_field_typed_and_left_applies_to_one_photo() {
        click(0)
        typeField("title", "Heron at dawn")
        compare(JSON.parse(app.photos.metadataOf("title")).value, "Heron at dawn")
        tryVerify(() => app.actions.undo.enabled, 5000)
        compare(app.actions.undo.text, "Undo the title", "one photo has no count in its own sentence")
        typeField("caption", "A grey heron & its reflection")
        typeField("credit", "Marie Tremblay")
        snapshot("metadata-en")
    }

    function test_a_field_typed_over_a_selection_applies_to_all_of_it_as_one_step() {
        // A caption of its own, not the one other tests leave on photo 0 (this suite's machine, and so
        // its photos' metadata, is shared): the same value again on an already-set photo is not a step,
        // which would undercount here.
        selectFirst(3)
        typeField("caption", "Three herons at the lake")
        compare(JSON.parse(app.photos.metadataOf("caption")).value, "Three herons at the lake")
        compare(app.actions.undo.text, "Undo the caption of 3 photos")
    }

    function test_a_mixed_selection_shows_multiple_values_and_a_uniform_one_does_not() {
        click(0)
        typeField("event", "Wedding")
        click(1)
        typeField("event", "Anniversary")
        selectFirst(2)
        // The panel refreshes off the same debounced timer the keyword panel's own usage already does
        // (Library.qml's usageTimer): a bare selection change needs a moment to reach it.
        tryVerify(() => rowFor("event").mixed === true, 5000)
        const row = rowFor("event")
        compare(row.singleLine.placeholderText, "Multiple values")
        compare(row.singleLine.text, "", "no value is guessed at")

        click(0)
        tryVerify(() => rowFor("event").mixed === false, 5000)
        compare(rowFor("event").singleLine.text, "Wedding")
    }

    function test_undo_and_redo_the_panel_shows_it_too() {
        click(0)
        typeField("headline", "Dawn")
        app.actions.undo.trigger()
        tryVerify(() => JSON.parse(app.photos.metadataOf("headline")).value === "", 5000)
        tryVerify(() => rowFor("headline").singleLine.text === "", 5000, "the panel follows the undo")
        app.actions.redo.trigger()
        tryVerify(() => JSON.parse(app.photos.metadataOf("headline")).value === "Dawn", 5000)
        tryVerify(() => rowFor("headline").singleLine.text === "Dawn", 5000, "the panel follows the redo")
    }

    function test_the_multiline_list_field_takes_one_name_a_line() {
        const row = rowFor("persons")
        verify(row.multiLine.visible && !row.singleLine.visible, "a multi-line field")
        click(0)
        row.multiLine.forceActiveFocus()
        row.multiLine.text = "Marie Tremblay\nJean Roy"
        grid.forceActiveFocus()
        wait(60)
        const meta = JSON.parse(app.photos.metadataOf("persons"))
        compare(meta.value, "Marie Tremblay\nJean Roy")
    }

    function test_creator_is_a_single_line_field_though_it_is_a_list_underneath() {
        // Patrick's own review, the morning after: almost every photo has exactly one creator.
        const row = rowFor("creator")
        verify(!row.multiLine.visible && row.singleLine.visible, "a single-line field")
        click(0)
        typeField("creator", "Marie Tremblay")
        compare(JSON.parse(app.photos.metadataOf("creator")).value, "Marie Tremblay")
    }

    function test_an_empty_value_clears_the_field() {
        click(0)
        typeField("source", "Own work")
        compare(JSON.parse(app.photos.metadataOf("source")).value, "Own work")
        typeField("source", "")
        compare(JSON.parse(app.photos.metadataOf("source")).value, "")
    }

    // Custom fields have no UI yet (WP10's own custom-field support): through the engine directly.
    function test_a_custom_field_round_trips_through_the_engine_though_the_panel_has_no_field_for_it() {
        click(0)
        verify(app.photos.setMetadataSelection("custom:Model release", "on file") === 1)
        tryVerify(() => JSON.parse(app.photos.metadataOf("custom:Model release")).value === "on file", 5000)
        for (const f of app.keywordPanel.metadataPanel.fields)
            verify(f.key !== "custom:Model release", "no row for a custom field yet")
    }

    function test_the_dialog_speaks_french() {
        app.launcher.chooseLanguage("fr")
        wait(250)
        click(0)
        typeField("title", "Un héron à l’aube")
        compare(app.actions.undo.text.indexOf("Annuler"), 0, app.actions.undo.text)
        snapshot("metadata-fr")
    }
}
