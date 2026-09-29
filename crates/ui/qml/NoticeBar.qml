// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// A sentence at the top of the window that says what could not be done, until it is dismissed.
AppBanner {
    id: root
    tone: "notice"
    property alias text: label.text
    signal dismissed
    property alias dismissButton: dismissButton

    visible: text !== ""
    implicitHeight: 40

    RowLayout {
        anchors.fill: parent
        anchors.margins: 6
        spacing: 8
        Label {
            id: label
            Layout.fillWidth: true
            elide: Text.ElideRight
            color: Theme.white
            Accessible.role: Accessible.StaticText
        }
        AppButton {
            id: dismissButton
            text: qsTr("Dismiss")
            onClicked: root.dismissed()
        }
    }
}
