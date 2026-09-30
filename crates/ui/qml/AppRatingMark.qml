// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import org.auroraw.ui

// A photo's rating as a mark (D-137): the number and the star icon after it ("3" and a star), in one colour, of the
// size that the text around it is. It stands for "3★" wherever the interface wrote that: a chip over a thumbnail, a
// pane of the comparison, a frame of the filmstrip. A screen reader reads it as "3 stars".
Item {
    id: mark
    property int rating: 0
    property int size: 15
    property color color: Theme.rating

    implicitWidth: number.implicitWidth + star.width + 1
    implicitHeight: Math.max(number.implicitHeight, star.height)
    Accessible.role: Accessible.StaticText
    Accessible.name: qsTr("%n star(s)", "", mark.rating)

    Text {
        id: number
        text: mark.rating
        color: mark.color
        font.pixelSize: mark.size
        Accessible.ignored: true   // the mark says it once, above
    }
    AppIcon {
        id: star
        x: number.width + 1
        anchors.verticalCenter: number.verticalCenter
        name: "star"
        size: mark.size
        color: mark.color
    }
}
