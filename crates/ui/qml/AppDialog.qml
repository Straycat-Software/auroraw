// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import org.auroraw.ui

// A modal dialog centred on the window, as wide as it asks up to the window's width. Every dialog of
// the application is one, so that they look and behave alike (Escape closes it).
//
// D-129 (the visual refresh) moved it to the `raised` tier — lighter than the window behind it, not
// Fusion's own default (`window`, the same mid-tone as the canvas) — with the larger of the two
// radius tiers, a hairline border instead of Fusion's flat 2px corner, more room throughout
// (Patrick's own review: Fusion's own padding read as cramped), and a soft shadow (`AppShadow.qml`,
// his own second review) to sell it actually floating above the window behind it.
//
// `resizable` (D-112) is opt-in: a dialog whose content benefits from more room (a list, a report)
// sets it to true and draws its own drag handle in its own `contentItem` (a generic one here would
// end up in `contentData` instead, orphaned the moment a dialog gives its own `contentItem`, which
// every one with a list or a report does) — `minWidth`/`minHeight` are its bounds to clamp against,
// the same corner shape as `DuplicatesDialog.qml`'s own. Every other dialog is unaffected. The size
// is not remembered across sessions.
Dialog {
    id: control
    property int preferredWidth: 640
    property bool resizable: false
    readonly property int minWidth: 360
    readonly property int minHeight: 200

    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(preferredWidth, (Overlay.overlay ? Overlay.overlay.width : preferredWidth) - 32)
    closePolicy: Popup.CloseOnEscape
    padding: 24

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
    // Fusion's own default header paints an opaque, square, `window`-coloured strip flush with the
    // top edge — it covered the background's rounded corners and border there (Patrick's own
    // review). A plain, transparent Label lets the rounded `raised` background show through whole.
    header: Label {
        visible: control.title !== ""
        text: control.title
        font.bold: true
        elide: Text.ElideRight
        leftPadding: 24
        rightPadding: 24
        topPadding: 20
        bottomPadding: 12
    }
}
