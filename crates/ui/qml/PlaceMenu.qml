// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// The place menu of the library's filter bar (design note 008 §5, D-151): a button that opens the tree Country ▸ Region ▸
// City of the places the photos in view are in, with the number of photos at each node; choosing a node lists what is
// under it ("everything from Quebec"), and "Any place" lifts the filter. It sits beside the flag and the series filters
// and is made like them: flat, a caret, `highlighted` while it filters.
//
// It talks to the grid through four things, the contract with the engine's slice 3 (the filter of the note's §5):
//
//   grid.placeFacets()          the tree, as JSON text: `{ "placed": N, "countries": [Node] }`, N the photos in view that
//                               have a country, and a Node `{ "label": "Quebec", "count": 80, "filter": {...},
//                               "children": [Node] }` (countries hold regions, regions hold cities; a city has none).
//                               `label` is the node's spelling to show: the most frequent one among the photos of the
//                               node, and when two are as frequent the way a person would choose: mixed case first
//                               (`Montréal` over `MONTRÉAL` and `montréal`), then the most accents kept (`Montréal` over
//                               `Montreal`), then the smallest text, so that it does not flicker (the grouping is made on
//                               the text folded for case and diacritics, so `Montreal` and `Montréal` are one node). `count` is the photos under the node, `filter` what
//                               selects them, `{ "country": key, "region": key, "city": key }` from the most general
//                               down, each key **opaque** (the engine's: the ISO code for a country that has one, the
//                               folded text otherwise; the menu only gives it back). The counts are those of the photos
//                               in view that pass every filter *but this one*, so that the tree stays one to move
//                               around in while a place is chosen.
//                               **A country with no region** (Singapore, or a photo whose region is empty) has its cities
//                               directly under it, with `filter: { country, region: "", city }`: the empty key of a region
//                               means "no region", so that the node selects exactly its count (`{ country, city }` would
//                               also select that city in every region of the country). The menu only gives it back.
//                               **A photo with a city or a region but no country** is not in the tree: `placed` does not
//                               count it and the filter cannot select it ("placed" is not "has any place field").
//                               **`pending`** (`"pending": true`) is whether the engine is still filling the places of the
//                               photos from their files (the first open after an upgrade, about a minute on a large
//                               library): the tree is then a part of the places, or none, and `placed: 0` does not mean
//                               that no photo has one. The button says so instead of saying how to get a place.
//   grid.placeFilter            the filter in force, the `filter` of a node as JSON text, or an empty text for none.
//   grid.setPlaceFilter(text)   puts the `filter` of a node in force (an empty text lifts it).
//   grid.placesChanged()        a signal, emitted when the place fields of photos in view may have changed without the
//                               photos in view changing (a run of place names, a hand-typed city, Undo and Redo of either,
//                               an accepted external change): the tree is read again, once things have settled (200 ms,
//                               restarted by each signal, so that 10,000 photos changed one by one are one reading).
//
// The menu never makes a key itself and never decides what a place is: it shows what the engine says and gives it back.
// Until the engine has `placeFacets` there is nothing to ask, and the menu is not there (`available`): it appears with
// the engine, with no change here.
Item {
    id: menu
    required property var grid
    readonly property bool available: typeof menu.grid.placeFacets === "function"
    visible: menu.available

    property alias button: button
    property alias popup: popup
    property alias list: list

    // What the engine last said: the photos in view that have a place, whether it is still reading them, and the tree.
    property var facets: ({ placed: 0, pending: false, countries: [] })
    // The nodes opened, by their key (the filter as text).
    property var opened: ({})

    implicitWidth: button.implicitWidth
    implicitHeight: button.implicitHeight

    // The filter in force, as an object, or null.
    readonly property var active: {
        const text = menu.available ? menu.grid.placeFilter : ""
        if (!text)
            return null
        try {
            return JSON.parse(text)
        } catch (e) {
            return null
        }
    }

    function keyOf(filter) {
        return JSON.stringify([filter.country || "", filter.region || "", filter.city || ""])
    }

    // The labels from the country down to the node of `filter`, or what the filter itself says when the tree has no
    // such node (the photos in view changed under it).
    function pathOf(filter) {
        const wanted = menu.keyOf(filter)
        const found = []
        const walk = nodes => {
            for (const node of nodes) {
                found.push(node.label)
                if (menu.keyOf(node.filter) === wanted)
                    return true
                if (node.children && walk(node.children))
                    return true
                found.pop()
            }
            return false
        }
        if (walk(menu.facets.countries))
            return found
        return [filter.city, filter.region, filter.country].filter(part => part).reverse()
    }

    // What the button says: "Any place", or the deepest place that filters.
    readonly property string choiceText: {
        const path = menu.active ? menu.pathOf(menu.active) : []
        return path.length > 0 ? path[path.length - 1] : qsTr("Any place")
    }
    // The whole path, for the one who cannot see the tree (a screen reader, the tooltip).
    readonly property string choicePath: menu.active ? menu.pathOf(menu.active).join(", ") : ""

    // The rows of the tree as they are shown: "Any place", then each node, its children after it while it is open.
    readonly property var rows: {
        const out = [{ any: true, label: qsTr("Any place"), depth: 0, count: 0, hasChildren: false, open: false, key: "" }]
        const walk = (nodes, depth) => {
            // (Sorted as JavaScript's `localeCompare` does, in the system's locale: the interface's language is not set as the
            // default locale, which is immaterial for English and French place names.)
            for (const node of nodes.slice().sort((a, b) => a.label.localeCompare(b.label))) {
                const key = menu.keyOf(node.filter)
                const children = node.children || []
                const open = children.length > 0 && menu.opened[key] === true
                out.push({ any: false, label: node.label, depth: depth, count: node.count, filter: node.filter,
                           hasChildren: children.length > 0, open: open, key: key })
                if (open)
                    walk(children, depth + 1)
            }
        }
        walk(menu.facets.countries, 0)
        return out
    }

    function selected(row) {
        return row.any ? menu.active === null : (menu.active !== null && menu.keyOf(menu.active) === row.key)
    }

    function refresh() {
        if (!menu.available)
            return
        try {
            menu.facets = JSON.parse(menu.grid.placeFacets())
        } catch (e) {
            menu.facets = ({ placed: 0, pending: false, countries: [] })
        }
    }

    // The path to the filter in force is open, so that the choice can be seen in the tree.
    function openActivePath() {
        if (!menu.active)
            return
        const next = Object.assign({}, menu.opened)
        const filter = menu.active
        if (filter.region)
            next[menu.keyOf({ country: filter.country })] = true
        if (filter.city)
            next[menu.keyOf({ country: filter.country, region: filter.region })] = true
        menu.opened = next
    }

    // The row the keyboard is on when the rows are made again (a node opened or closed, the tree read again): the list
    // starts over with its new model, and the keyboard goes back to it.
    property string keep: ""
    function keepCurrent(key) {
        menu.keep = key
    }
    onRowsChanged: {
        if (menu.keep === "" || !popup.visible)
            return
        const key = menu.keep
        menu.keep = ""
        Qt.callLater(() => {
            const at = menu.rows.findIndex(row => row.key === key)
            if (at >= 0)
                list.currentIndex = at
        })
    }

    function toggle(row) {
        menu.keepCurrent(row.key)
        const next = Object.assign({}, menu.opened)
        next[row.key] = !row.open
        menu.opened = next
    }

    function choose(row) {
        menu.grid.setPlaceFilter(row.any ? "" : JSON.stringify(row.filter))
        popup.close()
    }

    // The photos in view changed (a filter, a scan: `count`), or their places did without them (a run of place names, an
    // edit, Undo: `placesChanged`): the tree follows, once things have settled. Without the second, a button disabled for
    // lack of places could not learn that a run just gave some, since a disabled button cannot be opened to refresh it.
    Timer {
        id: settle
        interval: 200
        onTriggered: menu.refresh()
    }
    Connections {
        target: menu.available ? menu.grid : null
        ignoreUnknownSignals: true
        function onCountChanged() { settle.restart() }
        function onPlacesChanged() { settle.restart() }
    }
    Component.onCompleted: menu.refresh()

    AppButton {
        id: button
        focusPolicy: Qt.NoFocus
        enabled: menu.facets.placed > 0 || menu.active !== null
        highlighted: menu.active !== null
        // A name can be long: the bar does not grow with it.
        width: Math.min(implicitWidth, 220)
        leftPadding: 12
        rightPadding: 10
        Accessible.name: qsTr("Show photos by place")
        Accessible.description: menu.active ? menu.choicePath : qsTr("Any place")
        ToolTip.visible: hovered
        ToolTip.text: menu.active ? menu.choicePath
                                  : (menu.facets.placed > 0 ? qsTr("Show the photos of one country, region or city")
                                  : menu.facets.pending ? qsTr("Reading the places of your photos…")
                                                        : qsTr("No photo has a place yet. Find place names, in the Tools menu, looks them up."))
        onClicked: popup.opened ? popup.close() : popup.open()
        contentItem: RowLayout {
            spacing: 6
            Label {
                Layout.fillWidth: true
                text: menu.choiceText
                color: button.filled ? Theme.white : Theme.surface.text
                elide: Text.ElideRight
                verticalAlignment: Text.AlignVCenter
            }
            AppIcon {
                name: "caret-down"
                size: 11
                color: button.filled ? Theme.white : Theme.quiet
            }
        }
    }

    Popup {
        id: popup
        y: button.height + 2
        width: 320
        padding: 4
        focus: true
        implicitHeight: Math.min(list.contentHeight, 360) + topPadding + bottomPadding
        onAboutToShow: {
            menu.refresh()
            menu.openActivePath()
        }
        // The keyboard starts on the choice (on "Any place" when there is none), once the list has its rows.
        onOpened: list.currentIndex = Math.max(0, menu.rows.findIndex(row => menu.selected(row)))
        contentItem: AppListView {
            id: list
            implicitHeight: contentHeight
            focus: true
            keyNavigationEnabled: true
            model: popup.visible ? menu.rows : []
            Keys.onRightPressed: {
                const row = menu.rows[list.currentIndex]
                if (row && row.hasChildren && !row.open)
                    menu.toggle(row)
            }
            Keys.onLeftPressed: {
                const row = menu.rows[list.currentIndex]
                if (row && row.open) {
                    menu.toggle(row)
                } else if (row && row.depth > 0) {
                    // To the node above: the last row before this one that is less deep.
                    for (let i = list.currentIndex - 1; i >= 0; i--) {
                        if (menu.rows[i].depth < row.depth) {
                            list.currentIndex = i
                            break
                        }
                    }
                }
            }
            Keys.onReturnPressed: menu.choose(menu.rows[list.currentIndex])
            Keys.onEnterPressed: menu.choose(menu.rows[list.currentIndex])
            delegate: AppItemDelegate {
                id: row
                required property var modelData
                required property int index
                width: list.rowWidth
                highlighted: ListView.isCurrentItem
                leftPadding: 8 + modelData.depth * 16
                readonly property bool chosen: menu.selected(modelData)
                Accessible.name: modelData.any ? modelData.label : qsTr("%1, %n photo(s)", "", modelData.count).arg(modelData.label)
                Accessible.description: modelData.hasChildren ? (modelData.open ? qsTr("Open") : qsTr("Closed")) : ""
                onClicked: menu.choose(modelData)
                contentItem: RowLayout {
                    spacing: 6
                    // The twisty: a node that has places under it opens and closes here, without choosing it.
                    Item {
                        Layout.preferredWidth: 15
                        Layout.preferredHeight: 15
                        AppIcon {
                            anchors.centerIn: parent
                            visible: row.modelData.hasChildren
                            name: row.modelData.open ? "caret-down" : "caret-right"
                            size: 11
                            color: Theme.quiet
                        }
                        MouseArea {
                            anchors.fill: parent
                            enabled: row.modelData.hasChildren
                            onClicked: menu.toggle(row.modelData)
                        }
                    }
                    Label {
                        Layout.fillWidth: true
                        text: row.modelData.label
                        font.bold: row.chosen
                        elide: Text.ElideRight
                        verticalAlignment: Text.AlignVCenter
                    }
                    AppIcon {
                        visible: row.chosen
                        name: "check"
                        size: 11
                        color: Theme.accent
                    }
                    Label {
                        visible: !row.modelData.any
                        text: row.modelData.count
                        color: Theme.quiet
                        verticalAlignment: Text.AlignVCenter
                    }
                }
            }
        }
        background: AppShadow {
            shadowRadius: Theme.radiusContainer
            Rectangle {
                anchors.fill: parent
                radius: Theme.radiusContainer
                color: Theme.surface.raised
                border.width: 1
                border.color: Theme.surface.border
            }
        }
    }
}
