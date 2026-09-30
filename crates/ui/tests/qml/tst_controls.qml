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
        verify(Qt.colorEqual(f.background.border.color, Theme.surface.border), "not focused: the hairline")
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
}
