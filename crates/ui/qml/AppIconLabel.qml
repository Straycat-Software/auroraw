// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// A line of text whose `sentence` may hold the characters the interface used before it had icons (the star, the tick,
// the cross...), shown as the icons that replace them (D-137): for a sentence that comes from elsewhere, such as the
// summary the engine writes under the grid. It is made of runs, the icons in the icon font, each named by `family`, and
// the rest in the text's own, because no other way chooses the font on every platform and Qt (`Text.StyledText` ignores
// the `face` of a `<font>` tag, `Text.RichText` does not elide, and a font's list of fallbacks is not in Qt 6.4: left to
// the system, a private-use character is drawn by whichever font maps it). When the line is too narrow, the last run of
// text is elided.
Control {
    id: control

    property string sentence: ""
    property color color: Theme.surface.text
    property alias runItems: runs

    readonly property var runList: Icons.runs(sentence)
    // The run that gives room: the last of text (an icon does not shrink).
    readonly property int elidable: {
        for (let i = runList.length - 1; i >= 0; i--)
            if (!runList[i].icons)
                return i
        return -1
    }

    Accessible.role: Accessible.StaticText
    Accessible.name: sentence

    contentItem: RowLayout {
        spacing: 0
        Repeater {
            id: runs
            model: control.runList
            Label {
                required property var modelData
                required property int index
                Layout.alignment: Qt.AlignVCenter
                Layout.fillWidth: index === control.elidable
                text: modelData.text
                textFormat: Text.PlainText   // a '<' is what it is
                color: control.color
                elide: Text.ElideRight
                font.family: modelData.icons ? Icons.family : control.font.family
                font.hintingPreference: modelData.icons ? Font.PreferNoHinting : control.font.hintingPreference
                Accessible.ignored: true
            }
        }
    }
}
