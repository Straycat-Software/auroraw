// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import org.auroraw.ui

// A progress bar (D-136): an 8px groove on `sunken` with a hairline edge and a rounded `accentFill` bar, `value`
// from 0 to 1. Every progress of the interface is determinate (a share of a scan, an import or a batch), so there
// is no indeterminate animation: nothing here moves but the bar's own end. A progress that is not would show an empty groove
// that never moves, so `indeterminate` warns, and draws the groove full at half strength, not empty.
ProgressBar {
    id: control
    implicitWidth: 200
    implicitHeight: 8
    onIndeterminateChanged: {
        if (indeterminate)
            console.warn("AppProgressBar: indeterminate is not drawn (every progress of the interface is determinate); it shows a half-strength full bar")
    }

    background: Rectangle {
        radius: height / 2
        color: Theme.surface.sunken
        border.width: 1
        border.color: Theme.surface.border
    }
    contentItem: Item {
        Rectangle {
            width: control.indeterminate ? parent.width : Math.max(0, Math.min(1, control.visualPosition)) * parent.width
            height: parent.height
            radius: height / 2
            color: Theme.accentFill
            opacity: control.indeterminate ? 0.5 : 1
            visible: width > 0
        }
    }
}
