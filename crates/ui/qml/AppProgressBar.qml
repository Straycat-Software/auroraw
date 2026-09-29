// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import org.auroraw.ui

// A progress bar (D-136): an 8px groove on `sunken` with a hairline edge and a rounded `accentFill` bar, `value`
// from 0 to 1. Every progress of the interface is determinate (a share of a scan, an import or a batch), so there
// is no indeterminate animation: nothing here moves but the bar's own end.
ProgressBar {
    id: control
    implicitWidth: 200
    implicitHeight: 8

    background: Rectangle {
        radius: height / 2
        color: Theme.surface.sunken
        border.width: 1
        border.color: Theme.surface.border
    }
    contentItem: Item {
        Rectangle {
            width: Math.max(0, Math.min(1, control.visualPosition)) * parent.width
            height: parent.height
            radius: height / 2
            color: Theme.accentFill
            visible: width > 0
        }
    }
}
