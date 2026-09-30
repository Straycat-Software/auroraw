// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtTest
import org.auroraw.ui

// The controls of the interface on their own (D-136): what a person can do with each one, and the geometry that
// the rest of the interface counts on. No application window here, just the controls in a window of their own.
TestCase {
    id: tc
    name: "Controls"
    width: 420
    height: 320
    visible: true
    when: windowShown

    Component { id: toolButtonComponent; AppToolButton { text: "Peaking"; checkable: true } }
    Component { id: fieldComponent; AppTextField { width: 200; placeholderText: "Hint" } }
    Component { id: areaComponent; AppTextArea { width: 200; height: 60 } }
    Component { id: boxComponent; AppCheckBox { text: "Add this folder" } }
    Component { id: bareBoxComponent; AppCheckBox { tristate: true } }
    Component { id: radioComponent; Column { property alias first: a; property alias second: b
        AppRadioButton { id: a; text: "Keep mine"; checked: true }
        AppRadioButton { id: b; text: "Take the file's" } } }
    Component { id: sliderComponent; AppSlider { width: 200; from: 96; to: 256; value: 160; stepSize: 8 } }
    Component { id: progressComponent; AppProgressBar { width: 200; value: 0.25 } }
    Component { id: comboComponent; AppComboBox { width: 160; model: ["All", "Picked", "Rejected"]
        property var chosen: []
        onActivated: index => chosen.push(index) } }
    Component { id: iconComponent; AppIcon { name: "star"; size: 20 } }
    Component { id: ratingComponent; AppRatingMark { rating: 3 } }
    Component { id: iconLabelComponent; AppIconLabel { sentence: "a<b> \u2605\u2714 c"; font.pixelSize: 20 } }
    Component { id: iconButtonComponent; AppToolButton { iconName: "close"; iconSize: 13 } }
    FontMetrics { id: iconMetrics; font.family: Icons.family; font.pixelSize: 100 }
    Component { id: listModelComboComponent; AppComboBox { width: 200; textRole: "name"
        model: ListModel { ListElement { name: "Alpha" } ListElement { name: "Beta" } } } }
    Component { id: busyProgressComponent; AppProgressBar { width: 200; indeterminate: true } }
    Component { id: recordComboComponent; AppComboBox { width: 200; model: [{ path: "Animals" }, { path: "Animals / Birds" }]; textRole: "path" } }

    function make(component, properties) {
        const item = createTemporaryObject(component, tc, properties || {})
        verify(item, "made: " + component.errorString())
        return item
    }

    function test_a_tool_button_toggles_and_a_disabled_one_does_not() {
        const b = make(toolButtonComponent)
        verify(!b.checked)
        mouseClick(b)
        verify(b.checked, "a click turns a checkable tool on")
        mouseClick(b)
        verify(!b.checked)
        b.enabled = false
        mouseClick(b)
        verify(!b.checked, "and a disabled one stays as it was")
        compare(b.opacity, 0.35, "dimmed once")
    }

    function test_a_tool_button_is_as_high_as_a_text_needs_and_as_wide_as_its_label_and_padding() {
        const b = make(toolButtonComponent)
        // (What a line of text is high depends on the platform's default font, which this suite does not replace with
        // the application's own: so the height is the label's and its padding, not a number of pixels.)
        compare(b.implicitHeight, b.contentItem.implicitHeight + b.topPadding + b.bottomPadding)
        compare(b.topPadding, 6)
        verify(b.implicitWidth > b.contentItem.implicitWidth + 12)
    }

    function test_a_text_field_takes_typing_and_its_edge_shows_the_focus() {
        const f = make(fieldComponent)
        compare(f.implicitHeight, 28)
        verify(Qt.colorEqual(f.background.border.color, Theme.controlEdge), "not focused: the control's edge")
        f.forceActiveFocus()
        keyClick("a"); keyClick("b")
        compare(f.text, "ab")
        tryVerify(() => Qt.colorEqual(f.background.border.color, Theme.accent), 1000, "focused: the accent edge")
        f.enabled = false
        compare(f.opacity, 0.5)
    }

    function test_a_text_area_wraps_and_starts_at_the_top() {
        const a = make(areaComponent)
        a.forceActiveFocus()
        keyClick("x")
        compare(a.text, "x")
        compare(a.topPadding, 6)
        verify(a.wrapMode !== TextEdit.NoWrap)
    }

    function test_a_check_box_toggles_and_its_box_is_15_pixels() {
        const c = make(boxComponent)
        compare(c.indicator.width, 15)
        compare(c.indicator.height, 15)
        verify(!c.checked)
        mouseClick(c)
        verify(c.checked)
        mouseClick(c)
        verify(!c.checked)
    }

    // The 7 by 2 pixel bar an AppCheckBox draws when it is partly checked, visible or not.
    function hasDash(indicator) {
        for (let i = 0; i < indicator.children.length; i++) {
            const child = indicator.children[i]
            if (child.visible && child.width === 7 && child.height === 2)
                return true
        }
        return false
    }

    function test_a_tristate_check_box_without_text_is_only_its_box() {
        const c = make(bareBoxComponent)
        compare(c.implicitWidth, 15, "no label, no room kept for one")
        c.checkState = Qt.PartiallyChecked
        compare(c.checkState, Qt.PartiallyChecked)
        verify(hasDash(c.indicator), "a dash")
        c.checkState = Qt.Checked
        verify(!hasDash(c.indicator), "no dash when checked")
    }

    function test_radio_buttons_choose_one() {
        const col = make(radioComponent)
        verify(col.first.checked && !col.second.checked)
        mouseClick(col.second)
        verify(col.second.checked && !col.first.checked)
        compare(col.first.indicator.width, 15)
    }

    function test_a_slider_follows_a_drag_and_keeps_its_steps() {
        const s = make(sliderComponent)
        compare(s.value, 160)
        mousePress(s, s.leftPadding + 1, s.height / 2)
        mouseMove(s, s.width - 2, s.height / 2)
        mouseRelease(s, s.width - 2, s.height / 2)
        compare(s.value, 256, "dragged to the end")
        mouseClick(s, s.leftPadding + 1, s.height / 2)
        compare(s.value, 96, "and back to the start")
        compare(s.handle.width, 14)
    }

    function test_a_progress_bar_fills_its_share() {
        const p = make(progressComponent)
        compare(p.implicitHeight, 8)
        const bar = p.contentItem.children[0]
        fuzzyCompare(bar.width, p.contentItem.width * 0.25, 1)
        p.value = 1
        fuzzyCompare(bar.width, p.contentItem.width, 1)
        p.value = 0
        verify(!bar.visible, "nothing to draw at 0")
    }

    function test_a_combo_box_opens_its_list_and_a_row_chooses() {
        const c = make(comboComponent)
        compare(c.currentIndex, 0)
        compare(c.displayText, "All")
        mouseClick(c)
        tryVerify(() => c.popup.visible)
        const list = c.popup.contentItem
        tryVerify(() => list.count === 3)
        tryVerify(() => list.itemAtIndex(2) !== null)
        const row = list.itemAtIndex(2)
        compare(row.height, 32)
        compare(row.text, "Rejected")
        mouseClick(row)
        tryVerify(() => !c.popup.visible)
        compare(c.currentIndex, 2)
        compare(c.displayText, "Rejected")
        compare(c.chosen, [2], "activated once, with the row's index")
    }

    function test_a_combo_box_lists_the_texts_of_a_role_of_its_rows() {
        const c = make(recordComboComponent)
        compare(c.displayText, "Animals")
        mouseClick(c)
        tryVerify(() => c.popup.visible)
        tryVerify(() => c.popup.contentItem.itemAtIndex(1) !== null)
        compare(c.popup.contentItem.itemAtIndex(1).text, "Animals / Birds")
        c.popup.close()
    }

    function test_a_combo_box_keeps_its_width_whatever_it_shows() {
        const c = make(comboComponent, { sizingTexts: ["All", "Picked", "Rejected photos only"] })
        const w = c.implicitWidth
        c.currentIndex = 1
        compare(c.implicitWidth, w)
        c.currentIndex = 0
        compare(c.implicitWidth, w)
        const narrow = make(comboComponent)
        verify(w >= narrow.implicitWidth, "widest text of sizingTexts")
    }

    // ---- the icons (D-137)

    function test_the_icon_font_is_loaded_and_every_icon_is_one_private_use_character() {
        tryCompare(Icons.loader, "status", FontLoader.Ready)
        compare(Icons.family, "Auroraw Icons")
        verify(Icons.names.length >= 10)
        const seen = {}
        for (const name of Icons.names) {
            const glyph = Icons.glyph(name)
            compare(glyph.length, 1, name + " is one character")
            const code = glyph.charCodeAt(0)
            verify(code >= 0xE000 && code <= 0xF8FF, name + " is in the private-use area")
            verify(!seen[code], name + " has a code of its own")
            seen[code] = true
        }
        compare(Icons.glyph("no such icon"), "")
    }

    function test_every_icon_is_in_the_font_and_is_a_square_of_the_size_set() {
        tryCompare(Icons.loader, "status", FontLoader.Ready)
        for (const name of Icons.names)
            fuzzyCompare(iconMetrics.advanceWidth(Icons.glyph(name)), 100, 0.5, name + " is in the icon font, one em wide")
    }

    function test_an_icon_is_a_square_of_its_size_and_says_nothing_to_a_screen_reader() {
        const i = make(iconComponent)
        compare(i.width, 20)
        compare(i.height, 20)
        compare(i.text, Icons.glyph("star"))
        compare(i.font.family, Icons.family)
        verify(i.Accessible.ignored)
        i.name = "close"
        compare(i.text, Icons.glyph("close"))
    }

    function test_the_characters_the_interface_used_are_cut_out_of_a_text_from_elsewhere_as_runs_of_icons() {
        compare(Icons.legacy["\u2605"], "star")
        compare(Icons.legacy["\u2714"], "check")
        const star = Icons.glyph("star")
        compare(JSON.stringify(Icons.runs("3 \u2605\u2605 <a> & \u2716")),
                JSON.stringify([{ text: "3 ", icons: false }, { text: star + star, icons: true },
                                { text: " <a> & ", icons: false }, { text: Icons.glyph("close"), icons: true }]))
        compare(JSON.stringify(Icons.runs("plain")), JSON.stringify([{ text: "plain", icons: false }]))
        compare(JSON.stringify(Icons.runs("\u2605")), JSON.stringify([{ text: star, icons: true }]))
        compare(Icons.runs("").length, 0)
    }

    function test_a_label_sets_the_old_characters_as_icons_in_the_icon_font_one_em_wide() {
        tryCompare(Icons.loader, "status", FontLoader.Ready)
        const l = make(iconLabelComponent)
        compare(l.runItems.count, 3)
        const before = l.runItems.itemAt(0), icons = l.runItems.itemAt(1), after = l.runItems.itemAt(2)
        compare(before.text, "a<b> ")
        compare(after.text, " c")
        compare(icons.text, Icons.glyph("star") + Icons.glyph("check"))
        compare(icons.font.family, Icons.family, "the icons are in the icon font, not in what the system finds")
        verify(before.font.family !== Icons.family, "the rest is in the text's own")
        compare(before.textFormat, Text.PlainText, "a '<' is what it is")
        // Two icons, two em (the pixel size of the label's font).
        fuzzyCompare(icons.contentWidth, 2 * l.font.pixelSize, 1)
        fuzzyCompare(l.implicitWidth, before.implicitWidth + icons.implicitWidth + after.implicitWidth, 1)
        verify(icons.Accessible.ignored, "the line says its sentence once")
        compare(l.Accessible.name, "a<b> \u2605\u2714 c")
    }

    function test_a_label_that_is_too_narrow_elides_its_last_text_and_keeps_its_icons() {
        const l = make(iconLabelComponent)
        const icons = l.runItems.itemAt(1), after = l.runItems.itemAt(2)
        verify(!after.truncated)
        l.width = l.implicitWidth - 12
        tryVerify(() => after.truncated, 5000, "the text that follows is elided")
        fuzzyCompare(icons.width, icons.implicitWidth, 1, "an icon is not")
        verify(!l.runItems.itemAt(0).truncated, "and what comes before it is kept")
        compare(make(iconLabelComponent, { sentence: "" }).runItems.count, 0)
    }

    function test_a_rating_mark_is_a_number_and_a_star_and_reads_as_stars() {
        const m = make(ratingComponent)
        verify(m.implicitWidth > 15 && m.implicitHeight >= 15)
        // (No translation is installed in this suite: the source text, "%n star(s)", is what it says.)
        compare(m.Accessible.name.indexOf("3 star"), 0)
        m.rating = 1
        compare(m.Accessible.name.indexOf("1 star"), 0)
        const number = m.children[0]
        compare(number.text, "1")
        verify(number.Accessible.ignored, "the number is not read a second time")
    }

    function test_a_tool_button_with_an_icon_is_the_icon_and_its_padding() {
        const b = make(iconButtonComponent)
        compare(b.contentItem.implicitWidth, 13)
        compare(b.implicitWidth, 13 + b.leftPadding + b.rightPadding)
        compare(b.text, "", "no label under the icon")
    }

    // ---- the review of #24

    function test_a_combo_box_reads_the_rows_of_a_list_model_by_its_text_role() {
        const c = make(listModelComboComponent)
        compare(c.displayText, "Alpha")
        mouseClick(c)
        tryVerify(() => c.popup.visible)
        tryVerify(() => c.popup.contentItem.itemAtIndex(1) !== null)
        compare(c.popup.contentItem.itemAtIndex(1).text, "Beta")
        c.popup.close()
    }

    // The WCAG contrast of two colours, from their relative luminance.
    function luminance(color) {
        const lin = v => v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4)
        return 0.2126 * lin(color.r) + 0.7152 * lin(color.g) + 0.0722 * lin(color.b)
    }
    function contrast(a, b) {
        const la = luminance(Qt.color(a)), lb = luminance(Qt.color(b))
        return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05)
    }

    function test_the_edge_of_a_control_holds_three_to_one_on_what_it_sits_on() {
        // WCAG 1.4.11: what identifies a control. The frame's and the well's hairline (`surface.border`) is decoration and
        // stays fainter; a control with nothing but its edge to show where it is takes `controlEdge`.
        verify(contrast(Theme.controlEdge, Theme.surface.sunken) >= 3, "on a field's fill")
        verify(contrast(Theme.controlEdge, Theme.surface.window) >= 3, "on the window")
        const box = make(boxComponent)
        verify(Qt.colorEqual(box.indicator.border.color, Theme.controlEdge), "a check box's edge")
        const field = make(fieldComponent)
        verify(Qt.colorEqual(field.background.border.color, Theme.controlEdge), "a text field's edge")
        const area = make(areaComponent)
        verify(Qt.colorEqual(area.background.border.color, Theme.controlEdge), "a text area's edge")
        const slider = make(sliderComponent)
        verify(Qt.colorEqual(slider.background.border.color, Theme.controlEdge), "a slider's groove")
    }

    // Nothing of what a control draws lies outside it: a view that clips (a list, a scroll view) would cut it.
    function insideItsIndicator(indicator) {
        for (let i = 0; i < indicator.children.length; i++) {
            const child = indicator.children[i]
            if (child.visible && (child.x < 0 || child.y < 0 || child.x + child.width > indicator.width || child.y + child.height > indicator.height))
                return false
        }
        return true
    }

    function test_the_keyboard_focus_of_a_check_box_and_a_radio_is_on_their_own_edge_inside_them() {
        const box = make(boxComponent)
        box.forceActiveFocus(Qt.TabFocusReason)
        tryVerify(() => box.visualFocus)
        compare(box.indicator.border.width, 2)
        tryVerify(() => Qt.colorEqual(box.indicator.border.color, Theme.accent))   // (it fades in)
        verify(insideItsIndicator(box.indicator), "a check box's focus is not outside its box")
        const col = make(radioComponent)
        col.second.forceActiveFocus(Qt.TabFocusReason)
        tryVerify(() => col.second.visualFocus)
        compare(col.second.indicator.border.width, 2)
        tryVerify(() => Qt.colorEqual(col.second.indicator.border.color, Theme.accent))
        verify(insideItsIndicator(col.second.indicator), "a radio's focus is not outside its circle")
    }

    function test_a_progress_bar_that_is_indeterminate_says_so_and_is_not_an_empty_groove() {
        ignoreWarning(/AppProgressBar: indeterminate is not drawn/)
        const p = make(busyProgressComponent)
        const bar = p.contentItem.children[0]
        verify(bar.visible, "something is drawn")
        fuzzyCompare(bar.width, p.contentItem.width, 1)
        compare(bar.opacity, 0.5)
    }
}
