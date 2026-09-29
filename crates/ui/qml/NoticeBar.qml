// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// A sentence at the top of the window that says what could not be done, until it is dismissed.
Rectangle {
    id: root
    property alias text: label.text
    signal dismissed
    property alias dismissButton: dismissButton

    visible: text !== ""
    implicitHeight: 40
    // D-127: retuned to sit with the cool-neutral surfaces; still a muted amber, matching `warning`.
    color: "#433319"
    border.color: "#7a5c29"

    RowLayout {
        anchors.fill: parent
        anchors.margins: 6
        spacing: 8
        Label {
            id: label
            Layout.fillWidth: true
            elide: Text.ElideRight
            color: "white"
            Accessible.role: Accessible.StaticText
        }
        AppButton {
            id: dismissButton
            text: qsTr("Dismiss")
            onClicked: root.dismissed()
        }
    }
}
