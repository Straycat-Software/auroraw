// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtTest
import org.auroraw.ui

// The place menu of the library's filter bar (design note 008 §5, D-151) on its own, against a stand-in for the engine's
// slice 3: the four things the menu asks of the grid (`placeFacets()`, `placeFilter`, `setPlaceFilter()` and the signal
// `placesChanged()`), with a small tree of places. What a person can do: see the tree with its counts, open a country and a region, choose a node, lift the
// filter, and move in the tree by keyboard; and that the menu is not there while the grid cannot answer.
TestCase {
    id: tc
    name: "PlaceMenu"
    width: 520
    height: 520
    visible: true
    when: windowShown

    // Canada (Québec: Montréal, Québec; Ontario: Ottawa) and France (Île-de-France: Paris), as the engine would count them:
    // the filter of each node is what selects it, in the keys the engine folded.
    readonly property var tree: ({
        placed: 130,
        countries: [
            { label: "France", count: 30, filter: { country: "france" }, children: [
                { label: "Île-de-France", count: 30, filter: { country: "france", region: "ile-de-france" }, children: [
                    { label: "Paris", count: 30, filter: { country: "france", region: "ile-de-france", city: "paris" }, children: [] } ] } ] },
            { label: "Canada", count: 100, filter: { country: "canada" }, children: [
                { label: "Québec", count: 80, filter: { country: "canada", region: "quebec" }, children: [
                    { label: "Québec", count: 20, filter: { country: "canada", region: "quebec", city: "quebec" }, children: [] },
                    { label: "Montréal", count: 60, filter: { country: "canada", region: "quebec", city: "montreal" }, children: [] } ] },
                { label: "Ontario", count: 20, filter: { country: "canada", region: "ontario" }, children: [
                    { label: "Ottawa", count: 20, filter: { country: "canada", region: "ontario", city: "ottawa" }, children: [] } ] } ] }
        ]
    })

    Component {
        id: gridComponent
        QtObject {
            property string placeFilter: ""
            property int count: 130
            property int asked: 0
            property var answer: tc.tree
            signal placesChanged()
            function placeFacets() { asked++; return JSON.stringify(answer) }
            function setPlaceFilter(text) { placeFilter = text }
        }
    }
    Component { id: menuComponent; PlaceMenu { } }
    // A grid that does not have the engine's part yet.
    Component { id: bareGridComponent; QtObject { property int count: 5 } }

    function make(component, properties) {
        const item = createTemporaryObject(component, tc, properties || {})
        verify(item, "made: " + component.errorString())
        return item
    }

    function menuOn(grid) {
        const menu = make(menuComponent, { grid: grid })
        menu.x = 20
        menu.y = 10
        return menu
    }

    function open(menu) {
        mouseClick(menu.button)
        tryVerify(() => menu.popup.opened)
        tryVerify(() => menu.list.count > 0)
        // (The first frame with the rows in it: a click before it lands where they are not yet.)
        wait(60)
    }

    function texts(menu) {
        return menu.rows.map(row => " ".repeat(row.depth) + row.label + (row.any ? "" : " " + row.count))
    }

    function rowItem(menu, index) {
        tryVerify(() => menu.list.itemAtIndex(index) !== null)
        return menu.list.itemAtIndex(index)
    }

    function test_it_is_not_there_while_the_grid_cannot_answer() {
        const menu = menuOn(make(bareGridComponent))
        verify(!menu.available)
        verify(!menu.visible, "no menu for what the engine does not have yet")
        compare(menu.facets.placed, 0)
    }

    function test_closed_it_says_any_place_and_is_an_ordinary_filter_button() {
        const grid = make(gridComponent)
        const menu = menuOn(grid)
        verify(menu.available && menu.visible)
        compare(menu.choiceText, "Any place")
        verify(!menu.button.highlighted)
        verify(menu.button.enabled)
        compare(menu.button.Accessible.name, "Show photos by place")
        verify(grid.asked >= 1, "it asked for the tree")
    }

    function test_without_a_photo_that_has_a_place_it_is_disabled_and_says_how_to_get_one() {
        const grid = make(gridComponent)
        grid.answer = { placed: 0, countries: [] }
        const menu = menuOn(grid)
        verify(!menu.button.enabled)
        verify(menu.button.ToolTip.text.indexOf("Find place names") >= 0, menu.button.ToolTip.text)
    }

    function test_open_it_lists_the_countries_in_order_with_their_counts_and_nothing_below_them_yet() {
        const menu = menuOn(make(gridComponent))
        open(menu)
        compare(texts(menu), ["Any place", "Canada 100", "France 30"])
        verify(menu.rows[1].hasChildren && !menu.rows[1].open)
        // (No translation is installed in this suite: the source text, "%n photo(s)", is what it says.)
        compare(menu.list.itemAtIndex(1).Accessible.name.indexOf("Canada, 100 photo"), 0)
        compare(menu.list.itemAtIndex(0).Accessible.name, "Any place")
        menu.popup.close()
    }

    function test_the_twisty_opens_a_country_and_a_region_without_choosing_them() {
        const grid = make(gridComponent)
        const menu = menuOn(grid)
        open(menu)
        const twisty = (index) => {
            const item = rowItem(menu, index)
            return { x: 8 + menu.rows[index].depth * 16 + 7, y: item.height / 2, item: item }
        }
        let t = twisty(1)
        mouseClick(t.item, t.x, t.y)
        tryVerify(() => menu.rows.length === 5)
        compare(texts(menu), ["Any place", "Canada 100", " Ontario 20", " Québec 80", "France 30"])
        t = twisty(3)
        mouseClick(t.item, t.x, t.y)
        tryVerify(() => menu.rows.length === 7)
        compare(texts(menu), ["Any place", "Canada 100", " Ontario 20", " Québec 80", "  Montréal 60", "  Québec 20", "France 30"])
        compare(grid.placeFilter, "", "opening a node does not filter")
        verify(menu.popup.opened, "and the menu stays open")
        menu.popup.close()
    }

    function test_choosing_a_city_filters_by_it_and_closes_the_menu() {
        const grid = make(gridComponent)
        const menu = menuOn(grid)
        open(menu)
        menu.toggle(menu.rows[1])
        tryVerify(() => menu.rows.length === 5)
        menu.toggle(menu.rows[3])
        tryVerify(() => menu.rows.length === 7)
        const index = menu.rows.findIndex(row => row.label === "Montréal")
        mouseClick(rowItem(menu, index), 60, 12)
        tryVerify(() => !menu.popup.opened)
        compare(JSON.parse(grid.placeFilter), { country: "canada", region: "quebec", city: "montreal" })
        compare(menu.choiceText, "Montréal")
        compare(menu.choicePath, "Canada, Québec, Montréal")
        verify(menu.button.highlighted, "a filter in force is shown as one")
        compare(menu.button.Accessible.description, "Canada, Québec, Montréal")
    }

    function test_choosing_a_country_filters_by_everything_under_it() {
        const grid = make(gridComponent)
        const menu = menuOn(grid)
        open(menu)
        mouseClick(rowItem(menu, 2), 60, 12)
        tryVerify(() => !menu.popup.opened)
        compare(JSON.parse(grid.placeFilter), { country: "france" })
        compare(menu.choiceText, "France")
    }

    function test_any_place_lifts_the_filter() {
        const grid = make(gridComponent)
        grid.placeFilter = JSON.stringify({ country: "canada", region: "quebec" })
        const menu = menuOn(grid)
        compare(menu.choiceText, "Québec")
        open(menu)
        mouseClick(rowItem(menu, 0), 60, 12)
        tryVerify(() => !menu.popup.opened)
        compare(grid.placeFilter, "")
        compare(menu.choiceText, "Any place")
        verify(!menu.button.highlighted)
    }

    function test_the_menu_opens_with_the_path_of_the_filter_in_force_open_and_the_choice_marked() {
        const grid = make(gridComponent)
        grid.placeFilter = JSON.stringify({ country: "canada", region: "quebec", city: "montreal" })
        const menu = menuOn(grid)
        open(menu)
        compare(texts(menu), ["Any place", "Canada 100", " Ontario 20", " Québec 80", "  Montréal 60", "  Québec 20", "France 30"])
        const chosen = menu.rows.findIndex(row => row.label === "Montréal")
        verify(menu.selected(menu.rows[chosen]))
        verify(!menu.selected(menu.rows[0]), "Any place is not the choice")
        compare(menu.list.currentIndex, chosen, "the keyboard starts on the choice")
        menu.popup.close()
    }

    function test_the_tree_follows_the_photos_in_view() {
        const grid = make(gridComponent)
        const menu = menuOn(grid)
        const before = grid.asked
        grid.answer = { placed: 30, countries: [tc.tree.countries[0]] }
        grid.count = 30
        tryVerify(() => menu.facets.placed === 30, 2000, "the tree was asked for again")
        verify(grid.asked > before)
        open(menu)
        compare(texts(menu), ["Any place", "France 30"])
        menu.popup.close()
    }

    function test_a_run_of_place_names_enables_the_button_though_the_photos_in_view_are_the_same() {
        const grid = make(gridComponent)
        grid.answer = { placed: 0, countries: [] }
        const menu = menuOn(grid)
        verify(!menu.button.enabled, "no photo has a place: disabled, and a disabled button cannot be opened to refresh it")
        // The run changed the places of the photos, not the photos in view: `count` stays where it is.
        grid.answer = tc.tree
        grid.placesChanged()
        tryVerify(() => menu.facets.placed === 130, 2000, "the tree was asked for again")
        compare(grid.count, 130)
        verify(menu.button.enabled)
        open(menu)
        compare(texts(menu), ["Any place", "Canada 100", "France 30"])
        menu.popup.close()
        // And back: Undo of the run takes the places away again.
        grid.answer = { placed: 0, countries: [] }
        grid.placesChanged()
        tryVerify(() => !menu.button.enabled, 2000)
    }

    function test_many_changes_of_places_are_one_reading_of_the_tree() {
        const grid = make(gridComponent)
        const menu = menuOn(grid)
        wait(300)
        const before = grid.asked
        // (A run of 10,000 photos sends a signal for each.)
        for (let n = 0; n < 50; n++)
            grid.placesChanged()
        wait(450)
        compare(grid.asked, before + 1)
    }

    function test_a_country_with_no_region_has_its_cities_directly_under_it() {
        const grid = make(gridComponent)
        grid.answer = { placed: 7, countries: [
            { label: "Singapore", count: 7, filter: { country: "SG" }, children: [
                { label: "Singapore", count: 4, filter: { country: "SG", region: "", city: "singapore" }, children: [] },
                { label: "Jurong", count: 3, filter: { country: "SG", region: "", city: "jurong" }, children: [] } ] } ] }
        const menu = menuOn(grid)
        open(menu)
        compare(texts(menu), ["Any place", "Singapore 7"])
        const country = rowItem(menu, 1)
        mouseClick(country, 8 + 7, country.height / 2)
        tryVerify(() => menu.rows.length === 4)
        compare(texts(menu), ["Any place", "Singapore 7", " Jurong 3", " Singapore 4"])
        mouseClick(rowItem(menu, 2), 60, 12)
        tryVerify(() => !menu.popup.opened)
        compare(JSON.parse(grid.placeFilter), { country: "SG", region: "", city: "jurong" }, "the empty region key means no region: the keys are the engine's, given back")
        compare(menu.choiceText, "Jurong")
        compare(menu.choicePath, "Singapore, Jurong")
        // Opened again, the path to the choice is open: the country, and under it the city.
        open(menu)
        compare(texts(menu), ["Any place", "Singapore 7", " Jurong 3", " Singapore 4"])
        verify(menu.selected(menu.rows[2]))
        menu.popup.close()
    }

    function test_a_city_with_no_region_and_the_same_city_in_a_region_are_two_nodes() {
        // The empty key of a region means "no region": the two Dups select different photos, and each is the choice alone.
        const grid = make(gridComponent)
        grid.answer = { placed: 5, countries: [
            { label: "Xland", count: 5, filter: { country: "XX" }, children: [
                { label: "Dup", count: 2, filter: { country: "XX", region: "", city: "dup" }, children: [] },
                { label: "North", count: 3, filter: { country: "XX", region: "north" }, children: [
                    { label: "Dup", count: 3, filter: { country: "XX", region: "north", city: "dup" }, children: [] } ] } ] } ] }
        grid.placeFilter = JSON.stringify({ country: "XX", region: "", city: "dup" })
        const menu = menuOn(grid)
        open(menu)
        compare(texts(menu), ["Any place", "Xland 5", " Dup 2", " North 3"], "the country is open, the region is not")
        compare(menu.rows.filter(row => !row.any && menu.selected(row)).map(row => row.count), [2])
        compare(menu.choicePath, "Xland, Dup")
        menu.popup.close()
        tryVerify(() => !menu.popup.opened)
        grid.placeFilter = JSON.stringify({ country: "XX", region: "north", city: "dup" })
        open(menu)
        compare(texts(menu), ["Any place", "Xland 5", " Dup 2", " North 3", "  Dup 3"], "now the region is open too")
        compare(menu.rows.filter(row => !row.any && menu.selected(row)).map(row => row.count), [3])
        compare(menu.choicePath, "Xland, North, Dup")
        menu.popup.close()
    }

    function test_the_keyboard_moves_opens_and_chooses() {
        const grid = make(gridComponent)
        const menu = menuOn(grid)
        open(menu)
        compare(menu.list.currentIndex, 0)
        keyClick(Qt.Key_Down)
        compare(menu.list.currentIndex, 1)          // Canada
        keyClick(Qt.Key_Right)
        tryVerify(() => menu.rows.length === 5)
        keyClick(Qt.Key_Down)                        // Ontario
        keyClick(Qt.Key_Down)                        // Québec (the region)
        compare(menu.rows[menu.list.currentIndex].label, "Québec")
        keyClick(Qt.Key_Right)
        tryVerify(() => menu.rows.length === 7)
        keyClick(Qt.Key_Down)                        // Montréal
        keyClick(Qt.Key_Left)                        // on a city: to the region above it
        compare(menu.rows[menu.list.currentIndex].depth, 1)
        keyClick(Qt.Key_Left)                        // an open region closes
        tryVerify(() => menu.rows.length === 5)
        keyClick(Qt.Key_Return)                      // chooses the region
        tryVerify(() => !menu.popup.opened)
        compare(JSON.parse(grid.placeFilter), { country: "canada", region: "quebec" })
    }

    function test_escape_closes_it_without_choosing() {
        const grid = make(gridComponent)
        const menu = menuOn(grid)
        open(menu)
        keyClick(Qt.Key_Escape)
        tryVerify(() => !menu.popup.opened)
        compare(grid.placeFilter, "")
    }

    function test_a_filter_the_tree_no_longer_holds_is_still_said() {
        const grid = make(gridComponent)
        grid.placeFilter = JSON.stringify({ country: "norway", region: "vestland" })
        const menu = menuOn(grid)
        // (The photos in view changed under it: the label is what the filter says, the keys as the engine folded them.)
        compare(menu.choiceText, "vestland")
        compare(menu.choicePath, "norway, vestland")
        verify(menu.button.highlighted && menu.button.enabled, "it can still be lifted")
    }
}
