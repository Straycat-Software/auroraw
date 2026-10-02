// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// A menu row that shows its command's shortcut on the right (Qt Quick Controls' own rows do not),
// written the way the platform writes it (Ctrl+N, ⌘N).
MenuItem {
    id: item
    // A row that chooses a filter says whether it is the choice in force (`FilterAction.marked`, D-153): a check mark at
    // the front, and room for it in the rows that have one and in those only. A screen reader is told it is checked.
    readonly property bool markable: item.action !== null && item.action.marked !== undefined
    readonly property bool marked: markable && item.action.marked === true
    leftPadding: markable ? 30 : 8
    rightPadding: 8
    Accessible.checkable: markable
    Accessible.checked: marked

    indicator: AppIcon {
        x: 8
        y: (item.height - height) / 2
        name: "check"
        size: 14
        visible: item.marked
    }

    // D-129: a soft rounded fill on hover, the interface's one motion exception — replacing
    // Fusion's own square, edge-to-edge highlight.
    background: Rectangle {
        radius: Theme.radiusControl
        color: item.highlighted ? Qt.rgba(item.palette.windowText.r, item.palette.windowText.g, item.palette.windowText.b, 0.08) : "transparent"
        Behavior on color { ColorAnimation { duration: Theme.motion } }
    }

    function underlined(title) {
        const escape = t => t.replace(/&/g, "&amp;").replace(/</g, "&lt;")
        const at = title.indexOf("&")
        if (at < 0 || at + 1 >= title.length)
            return escape(title)
        return escape(title.substring(0, at)) + "<u>" + escape(title.charAt(at + 1)) + "</u>"
               + escape(title.substring(at + 2))
    }

    contentItem: RowLayout {
        spacing: 24
        Label {
            Layout.fillWidth: true
            // A section's mnemonic (`&File`) is underlined: Alt and that letter open it.
            textFormat: item.subMenu ? Text.StyledText : Text.PlainText
            text: item.subMenu ? item.underlined(item.text) : item.text
            elide: Text.ElideRight
            color: item.enabled ? item.palette.windowText : item.palette.placeholderText
        }
        Label {
            text: {
                const key = item.action ? item.action.shortcut : undefined
                return typeof key === "number" ? Shortcuts.text(key, "") : Shortcuts.text(-1, key ? String(key) : "")
            }
            color: item.enabled ? Theme.quiet : item.palette.placeholderText
        }
    }
}
