// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls

// The hamburger menu (D-090; a Tools section added by issue #14): File, Edit, Tools and Help as
// cascading submenus, each command with its shortcut. Alt and the letter marked with & in a
// section's title open it (`openSection`), so the letter follows the language (Alt+F, Alt+E,
// Alt+T, Alt+H; Alt+F, Alt+É, Alt+O, Alt+A in French).
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
