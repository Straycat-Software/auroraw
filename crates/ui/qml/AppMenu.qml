// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls

// The hamburger menu (D-090; a Tools section added by issue #14, a View section by D-153): File, Edit, View, Tools
// and Help as cascading submenus, each command with its shortcut. Alt and the letter marked with & in a
// section's title open it (`openSection`), so the letter follows the language (Alt+F, Alt+E, Alt+V,
// Alt+T, Alt+H; Alt+F, Alt+É, Alt+H, Alt+O, Alt+A in French).
AppSubMenu {
    id: root
    required property var actions
    // Every row is one of ours (shortcuts written out, the mnemonics of the sections underlined).
    delegate: AppMenuItem {}

    // The key sequence that opens section `index`: Alt and its mnemonic.
    function sectionKey(index) {
        const title = itemAt(index) ? itemAt(index).subMenu.title : ""
        const at = title.indexOf("&")
        return at >= 0 && at + 1 < title.length ? "Alt+" + title[at + 1].toUpperCase() : ""
    }

    function openSection(index) {
        popup(parent, 0, parent ? parent.height : 0)
        itemAt(index).subMenu.popup()
    }

    AppSubMenu {
        title: qsTr("&File")
        AppMenuItem { action: root.actions.newWorkspace }
        AppMenuItem { action: root.actions.openWorkspace }
        MenuSeparator {}
        AppMenuItem { action: root.actions.settings }
        MenuSeparator {}
        AppMenuItem { action: root.actions.quit }
    }
    AppSubMenu {
        title: qsTr("&Edit")
        AppMenuItem { action: root.actions.undo }
        AppMenuItem { action: root.actions.redo }
        MenuSeparator {}
        AppMenuItem { action: root.actions.cut }
        AppMenuItem { action: root.actions.copy }
        AppMenuItem { action: root.actions.paste }
        AppMenuItem { action: root.actions.deleteSelection }
        MenuSeparator {}
        AppMenuItem { action: root.actions.selectAll }
        AppMenuItem { action: root.actions.selectNone }
        AppMenuItem { action: root.actions.invertSelection }
    }
    // The filters of the library's bar and what its other buttons do (D-153), so that a keyboard reaches them: the
    // rows that choose a filter show a check mark on the one in force. (Its own rows are ours too: this section
    // holds submenus, which the default row would draw the platform's way.)
    AppSubMenu {
        title: qsTr("&View")
        delegate: AppMenuItem {}
        AppSubMenu {
            title: qsTr("Filter by rating")
            AppMenuItem { action: root.actions.ratingAny }
            AppMenuItem { action: root.actions.rating1 }
            AppMenuItem { action: root.actions.rating2 }
            AppMenuItem { action: root.actions.rating3 }
            AppMenuItem { action: root.actions.rating4 }
            AppMenuItem { action: root.actions.rating5 }
        }
        AppSubMenu {
            title: qsTr("Filter by flag")
            AppMenuItem { action: root.actions.flagsNotRejected }
            AppMenuItem { action: root.actions.flagsAll }
            AppMenuItem { action: root.actions.flagsPicked }
            AppMenuItem { action: root.actions.flagsRejected }
        }
        AppSubMenu {
            title: qsTr("Filter by colour")
            AppMenuItem { action: root.actions.colourRed }
            AppMenuItem { action: root.actions.colourYellow }
            AppMenuItem { action: root.actions.colourGreen }
            AppMenuItem { action: root.actions.colourBlue }
            AppMenuItem { action: root.actions.colourPurple }
            MenuSeparator {}
            AppMenuItem { action: root.actions.colourAny }
        }
        AppSubMenu {
            title: qsTr("Filter by series")
            AppMenuItem { action: root.actions.seriesAny }
            AppMenuItem { action: root.actions.seriesIn }
            AppMenuItem { action: root.actions.seriesUnresolved }
            AppMenuItem { action: root.actions.seriesResolved }
        }
        AppMenuItem { action: root.actions.clearFilters }
        MenuSeparator {}
        AppMenuItem { action: root.actions.openAllSeries }
        AppMenuItem { action: root.actions.closeAllSeries }
        MenuSeparator {}
        AppMenuItem { action: root.actions.refreshList }
        AppMenuItem { action: root.actions.exportList }
    }
    AppSubMenu {
        title: qsTr("&Tools")
        AppMenuItem { action: root.actions.importPhotos }
        AppMenuItem { action: root.actions.exportXmp }
        AppMenuItem { action: root.actions.findPlaceNames }
        MenuSeparator {}
        AppMenuItem { action: root.actions.editKeywords }
        MenuSeparator {}
        AppMenuItem { action: root.actions.duplicates }
    }
    AppSubMenu {
        title: qsTr("&Help")
        AppMenuItem { action: root.actions.about }
    }
}
