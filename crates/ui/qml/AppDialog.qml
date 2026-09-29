// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import org.auroraw.ui

// A modal dialog centred on the window, as wide as it asks up to the window's width. Every dialog of
// the application is one, so that they look and behave alike (Escape closes it).
//
// D-127 (the visual refresh) moved it to the `raised` tier — lighter than the window behind it, not
// Fusion's own default (`window`, the same mid-tone as the canvas) — with the larger of the two
// radius tiers and a hairline border instead of Fusion's flat 2px corner.
//
// `resizable` (D-112) is opt-in: a dialog whose content benefits from more room (a list, a report)
// sets it to true and draws its own drag handle in its own `contentItem` (a generic one here would
// end up in `contentData` instead, orphaned the moment a dialog gives its own `contentItem`, which
// every one with a list or a report does) — `minWidth`/`minHeight` are its bounds to clamp against,
// the same corner shape as `DuplicatesDialog.qml`'s own. Every other dialog is unaffected. The size
// is not remembered across sessions.
Dialog {
    property int preferredWidth: 640
    property bool resizable: false
    readonly property int minWidth: 360
    readonly property int minHeight: 200

    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(preferredWidth, (Overlay.overlay ? Overlay.overlay.width : preferredWidth) - 32)
    closePolicy: Popup.CloseOnEscape
    padding: 16

    background: Rectangle {
        radius: Theme.radiusContainer
        color: Theme.surface.raised
        border.width: 1
        border.color: Theme.surface.border
    }
}
