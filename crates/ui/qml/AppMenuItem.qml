// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// A menu row that shows its command's shortcut on the right (Qt Quick Controls' own rows do not),
// written the way the platform writes it (Ctrl+N, ⌘N).
MenuItem {
    id: item
    leftPadding: 8
    rightPadding: 8

    // D-127: a soft fill on hover, the interface's one motion exception, square like the menu
    // itself (Patrick's own review) rather than Fusion's own edge-to-edge highlight.
    background: Rectangle {
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
            color: item.palette.placeholderText
        }
    }
}
