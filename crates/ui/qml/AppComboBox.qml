// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import org.auroraw.ui

// A combo box (D-136): the flat face of AppButton (a `hover` fill, a hairline edge, `radius-control`) with a small
// ▾ at the right, so that it sits among the buttons of a bar as one of the same family instead of Fusion's own
// bevelled one. Its list is a popup on the `raised` tier with the container radius and a soft shadow, like a
// menu, and its rows are AppItemDelegate. Not editable: every combo box of the interface picks from a fixed list.
//
// `sizingTexts` are every text the box can show: its width is the widest of them, so that it does not change
// width, and move what is beside it, as the choice changes (Qt sizes a combo box from the model's own texts,
// which a model of numbers with a `displayText` of its own does not have).
ComboBox {
    id: control
    property var sizingTexts: []

    implicitWidth: Math.max(96, Math.max(sizing.maxWidth, contentItem.implicitWidth) + leftPadding + rightPadding)
    implicitHeight: Math.max(28, contentItem.implicitHeight + topPadding + bottomPadding)
    leftPadding: 10
    rightPadding: 26
    topPadding: 4
    bottomPadding: 4
    opacity: enabled ? 1 : 0.4

    TextMetrics {
        id: sizing
        property real maxWidth: 0
        font: control.font
        function measure() {
            let widest = 0
            for (const text of control.sizingTexts) {
                sizing.text = text
                widest = Math.max(widest, sizing.advanceWidth)
            }
            sizing.maxWidth = widest
        }
        Component.onCompleted: measure()
    }
    onSizingTextsChanged: sizing.measure()
    onFontChanged: sizing.measure()

    delegate: AppItemDelegate {
        required property int index
        width: ListView.view ? ListView.view.width : control.width
        // `textAt` follows `textRole` and the model itself: a list of texts, of objects, a ListModel or any item model.
        text: control.textAt(index)
        highlighted: control.highlightedIndex === index
    }
    indicator: AppIcon {
        x: control.width - width - 10
        y: (control.height - height) / 2
        name: "caret-down"
        size: 11
        color: Theme.quiet
    }
    contentItem: Label {
        text: control.displayText
        font: control.font
        color: Theme.surface.text
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
    background: Rectangle {
        radius: Theme.radiusControl
        border.width: 1
        color: control.popup.visible || control.pressed ? Theme.surface.sunken : Theme.surface.hover
        border.color: control.hovered || control.popup.visible ? Theme.quiet : Theme.surface.border
        Behavior on color { ColorAnimation { duration: Theme.motion } }
        Behavior on border.color { ColorAnimation { duration: Theme.motion } }
        Rectangle {
            anchors.fill: parent
            anchors.margins: -3
            radius: parent.radius + 3
            color: "transparent"
            border.width: 2
            border.color: Theme.accent
            visible: control.visualFocus
        }
    }
    popup: Popup {
        y: control.height + 2
        width: control.width
        implicitHeight: Math.min(list.contentHeight, 320) + topPadding + bottomPadding
        padding: 4
        contentItem: ListView {
            id: list
            clip: true
            implicitHeight: contentHeight
            model: control.popup.visible ? control.delegateModel : null
            currentIndex: control.highlightedIndex
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: AppScrollBar {}
        }
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
    }
}
